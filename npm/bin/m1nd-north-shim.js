#!/usr/bin/env node
"use strict";

// m1nd-north-shim: the single stable command that SessionStart-family host hooks
// call. It prints the exact hook envelope every host expects:
//   {"hookSpecificOutput":{"hookEventName":"<event>","additionalContext":"<text>"}}
//
// v3 — north-first voice card. When a served m1nd owner is reachable (discovered
// the same read-only way `--attach auto` finds one: the instance registry, then a
// short probe of the default serve ports), the shim calls the `north` tool over
// HTTP and OPENS with that owner's human_view card (the m1nd voice: pulse + lines)
// before the machine-legible summary. north is READ-ONLY, so the newcomer's first
// breath never rebinds/ingests/mutates the shared owner.
//
// When no served owner answers with a human_view, it falls back EXACTLY to the
// prior behavior: `m1nd agent first-minute ... --json` rendered to the same
// summary line (standalone, no card).
//
// FAIL-OPEN: any error / timeout / non-zero exit / parse failure prints nothing
// and exits 0, so a broken or absent runtime never blocks the host session.

const fs = require("fs");
const os = require("os");
const path = require("path");
const http = require("http");
const { spawnSync } = require("child_process");

function parseArgs(argv) {
  const parsed = {
    repo: process.cwd(),
    query: "orient",
    event: "SessionStart",
    mode: "short",
  };
  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    if (arg === "--repo" || arg === "--query" || arg === "--event" || arg === "--mode") {
      const value = argv[index + 1];
      if (value !== undefined && !value.startsWith("--")) {
        parsed[arg.slice(2)] = value;
        index += 1;
      }
    }
  }
  return parsed;
}

function safeParse(text) {
  try {
    return JSON.parse(text);
  } catch (_) {
    return null;
  }
}

function truncate(text, max) {
  const value = String(text || "").replace(/\s+/g, " ").trim();
  return value.length > max ? `${value.slice(0, max - 3)}...` : value;
}

// The north card's human-facing voice: `human_view.lines`. It rides the north
// packet (top level), and — when first-minute's orientation pass was north — it
// is also nested under `results[0]`. Return up to 4 trimmed lines, or [].
function humanViewLines(data) {
  const candidates = [
    data && data.human_view,
    data && data.context && data.context.human_view,
    data && Array.isArray(data.results) && data.results[0] && data.results[0].human_view,
  ];
  for (const hv of candidates) {
    if (hv && Array.isArray(hv.lines)) {
      const lines = hv.lines
        .map((line) => String(line || "").replace(/\s+/g, " ").trim())
        .filter(Boolean)
        .slice(0, 4);
      if (lines.length > 0) return lines;
    }
  }
  return [];
}

// Cap to `max` characters WITHOUT ever splitting a line mid-way: keep whole lines
// until the next one would overflow, then drop any trailing blank separator left
// by the cut. Whole-line-or-nothing keeps the voice card legible.
function capWholeLines(text, max) {
  const value = String(text || "");
  if (value.length <= max) return value;
  const lines = value.split("\n");
  const kept = [];
  let length = 0;
  for (const line of lines) {
    const add = (kept.length > 0 ? 1 : 0) + line.length;
    if (length + add > max) break;
    kept.push(line);
    length += add;
  }
  while (kept.length > 0 && kept[kept.length - 1] === "") kept.pop();
  return kept.join("\n");
}

function renderNorthPacket(data) {
  if (!data || typeof data !== "object") return "";

  // Open with the human_view card (the human-facing voice) when the packet
  // carries one, then a blank line, then the machine-legible summary.
  const head = humanViewLines(data);

  const fields = [];

  const trust = (data.trust && data.trust.verdict) || (data.binding && data.binding.trust_mode) || "unknown";
  const usageMode = data.scope_alignment && data.scope_alignment.recommended_usage_mode;
  fields.push(`trust=${trust}${usageMode ? ` usage=${usageMode}` : ""}`);

  const anchorList = Array.isArray(data.anchors)
    ? data.anchors
    : data.context && Array.isArray(data.context.anchors)
      ? data.context.anchors
      : [];
  const anchors = anchorList
    .map((entry) => (entry && (entry.path || entry.label)) || "")
    .filter(Boolean)
    .slice(0, 4);
  if (anchors.length > 0) fields.push(`anchors: ${anchors.join(", ")}`);

  if (Array.isArray(data.memory)) {
    const memory = data.memory
      .slice(0, 3)
      .map((entry) => `${truncate(entry && entry.claim, 70)} [${(entry && entry.source_agent) || "?"}]`)
      .filter((entry) => entry.replace(/\[\?\]$/, "").trim());
    if (memory.length > 0) fields.push(`memory: ${memory.join(" | ")}`);
  }

  const gapSource = Array.isArray(data.honest_gaps)
    ? data.honest_gaps
    : data.proof_boundary && Array.isArray(data.proof_boundary.still_needs_direct_proof)
      ? data.proof_boundary.still_needs_direct_proof
      : [];
  const gaps = gapSource.map((entry) => truncate(entry, 80)).filter(Boolean).slice(0, 2);
  if (gaps.length > 0) fields.push(`honest gaps: ${gaps.join("; ")}`);

  const nextMove = data.next_move || data.recommended_next_command;
  if (nextMove) fields.push(`suggested first move: ${truncate(nextMove, 100)}`);

  const summary = fields.length > 0 ? `[m1nd north] ${fields.join(" · ")}` : "";
  const blocks = [];
  if (head.length > 0) blocks.push(head.join("\n"));
  if (summary) blocks.push(summary);
  if (blocks.length === 0) return "";
  // Blank line between the voice card and the summary.
  return capWholeLines(blocks.join("\n\n"), 1200);
}

// Discover reachable served-owner base URLs the SAME read-only way `--attach auto`
// does: inspect the instance registry (`~/.m1nd/registry/instances/*.json`) for
// live serve ReadWrite owners that publish a port, freshest-first — then always
// append the default serve ports as a probe fallback (the registry entry may lack
// a port even while an owner serves). NEVER acquires a lease or mutates anything.
function servedOwnerBaseUrls() {
  const urls = [];
  try {
    const dir = path.join(os.homedir(), ".m1nd", "registry", "instances");
    const entries = fs
      .readdirSync(dir)
      .filter((name) => name.endsWith(".json"))
      .map((name) => safeParse(fs.readFileSync(path.join(dir, name), "utf8")))
      .filter(
        (entry) =>
          entry &&
          entry.mode === "read_write" &&
          entry.owner_live === true &&
          !entry.stale &&
          entry.port
      )
      .sort((a, b) => (b.last_heartbeat_ms || 0) - (a.last_heartbeat_ms || 0));
    for (const entry of entries) {
      const bind = !entry.bind || entry.bind === "0.0.0.0" ? "127.0.0.1" : entry.bind;
      urls.push(`http://${bind}:${entry.port}`);
    }
  } catch (_) {
    // no registry / unreadable — fall through to the fixed probe ports.
  }
  urls.push("http://127.0.0.1:1337", "http://127.0.0.1:1338");
  return [...new Set(urls)];
}

// POST the read-only `north` tool to one served owner; resolve its unwrapped
// payload, or null on any error / non-200 / timeout / parse failure.
function northViaHttp(baseUrl, repo, timeoutMs) {
  return new Promise((resolve) => {
    let settled = false;
    const finish = (value) => {
      if (!settled) {
        settled = true;
        resolve(value);
      }
    };
    try {
      const body = JSON.stringify({
        agent_id: "m1nd-north-shim",
        task: "orient this session on the bound workspace",
      });
      const url = new URL(`${baseUrl.replace(/\/$/, "")}/api/tools/north`);
      const req = http.request(
        {
          hostname: url.hostname,
          port: url.port,
          path: url.pathname,
          method: "POST",
          headers: {
            "Content-Type": "application/json",
            "Content-Length": Buffer.byteLength(body),
            // Declare the caller's repo the same way the --attach bridge does, so
            // the owner can align its reception to this session's workspace.
            "m1nd-caller-root": String(repo || ""),
          },
          timeout: timeoutMs,
        },
        (res) => {
          if (res.statusCode !== 200) {
            res.resume();
            return finish(null);
          }
          let data = "";
          res.on("data", (chunk) => {
            data += chunk;
            if (data.length > 262144) req.destroy();
          });
          res.on("end", () => {
            const parsed = safeParse(data);
            finish(parsed && parsed.result ? parsed.result : parsed);
          });
        }
      );
      req.on("error", () => finish(null));
      req.on("timeout", () => {
        req.destroy();
        finish(null);
      });
      req.write(body);
      req.end();
    } catch (_) {
      finish(null);
    }
  });
}

function emitEnvelope(event, text) {
  process.stdout.write(
    `${JSON.stringify({ hookSpecificOutput: { hookEventName: event, additionalContext: text } })}\n`
  );
}

// Fallback: the prior standalone behavior — run `agent first-minute --json` and
// render its summary line (no human_view card).
function firstMinuteFallback(args) {
  const cli = path.join(__dirname, "m1nd.js");
  let res;
  try {
    res = spawnSync(
      process.execPath,
      [cli, "agent", "first-minute", "--repo", args.repo, "--query", args.query, "--mode", args.mode, "--json"],
      { encoding: "utf8", timeout: 8000, killSignal: "SIGKILL" }
    );
  } catch (_) {
    process.exit(0);
  }
  if (!res || res.error || res.status !== 0 || !res.stdout || !res.stdout.trim()) {
    process.exit(0);
  }
  const data = safeParse(res.stdout);
  if (!data) process.exit(0);
  const text = renderNorthPacket(data);
  if (!text || !text.trim()) process.exit(0);
  emitEnvelope(args.event, text);
  process.exit(0);
}

async function main() {
  const args = parseArgs(process.argv.slice(2));

  // v3: open with the served owner's north voice card when one is reachable.
  // A dead/refused port fails in ~3ms (ECONNREFUSED, before the timeout bites),
  // so the timeout only guards a hung owner. north itself measures ~0.3–0.85s
  // here, so 2.5s leaves headroom under host-startup load while staying far below
  // the 8s first-minute fallback — a 1s cap would flap to first-minute (which
  // rebinds the shared owner) exactly when north is a hair slow.
  try {
    for (const baseUrl of servedOwnerBaseUrls()) {
      const payload = await northViaHttp(baseUrl, args.repo, 2500);
      if (!payload) continue;
      if (humanViewLines(payload).length === 0) continue;
      const text = renderNorthPacket(payload);
      if (text && text.trim()) {
        emitEnvelope(args.event, text);
        process.exit(0);
      }
    }
  } catch (_) {
    // fall through to the standalone first-minute path
  }

  firstMinuteFallback(args);
}

if (require.main === module) {
  main().catch(() => process.exit(0));
}

module.exports = {
  parseArgs,
  humanViewLines,
  capWholeLines,
  renderNorthPacket,
  servedOwnerBaseUrls,
  northViaHttp,
};
