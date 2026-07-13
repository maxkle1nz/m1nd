#!/usr/bin/env node
"use strict";

// m1nd-north-shim: the single stable command that SessionStart-family host hooks
// call. It runs `m1nd agent first-minute ... --json`, renders the orientation packet
// to compact human text, and prints the exact hook envelope every host expects:
//   {"hookSpecificOutput":{"hookEventName":"<event>","additionalContext":"<text>"}}
// FAIL-OPEN: any error / timeout / non-zero exit / parse failure prints nothing and
// exits 0, so a broken or absent runtime never blocks the host session.

const path = require("path");
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

function renderNorthPacket(data) {
  if (!data || typeof data !== "object") return "";

  // v2: open with the human_view card (the human-facing voice) when the packet
  // carries one, BEFORE the machine-legible summary line.
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
  const packet = [...head, summary].filter(Boolean).join("\n");
  if (!packet.trim()) return "";
  return packet.length > 1200 ? `${packet.slice(0, 1197)}...` : packet;
}

function main() {
  const args = parseArgs(process.argv.slice(2));
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
  process.stdout.write(
    `${JSON.stringify({ hookSpecificOutput: { hookEventName: args.event, additionalContext: text } })}\n`
  );
  process.exit(0);
}

main();
