"use strict";

const assert = require("node:assert/strict");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const { spawnSync } = require("node:child_process");
const test = require("node:test");
const { agentRuntimeCacheTarget } = require("../lib/agent-runtime-cache");

const CLI = path.resolve(__dirname, "../bin/m1nd.js");
const POSIX_SHEBANG_ONLY = {
  skip: process.platform === "win32" && "POSIX shebang fixture cannot be spawned by Windows CreateProcess",
};

function fixture() {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "m1nd-agent-cold-context-"));
  const repo = path.join(root, "repo");
  fs.mkdirSync(repo);
  fs.writeFileSync(path.join(repo, "signal.js"), "export const signal = 1;\n");
  const binary = path.join(root, "fake-mcp.js");
  fs.writeFileSync(binary, `#!${process.execPath}\n` + `
const fs = require("node:fs");
const readline = require("node:readline");
if (process.argv.includes("--version")) { console.log("m1nd-mcp 1.6.3"); process.exit(0); }
const log = process.env.FIXTURE_CALL_LOG;
const rl = readline.createInterface({ input: process.stdin });
rl.on("line", (line) => {
  const req = JSON.parse(line);
  let result = {};
  if (req.method === "tools/list") result = { tools: ["ingest", "trust_selftest", "session_handshake", "surgical_context_v2"].map(name => ({name})) };
  if (req.method === "tools/call") {
    const name = req.params.name;
    fs.appendFileSync(log, name + "\\n");
    let payload;
    let isError = false;
    if (name === "ingest") payload = { ok: false, refused: "no_declared_root", reason: "no graph exists" };
    if (name === "trust_selftest") payload = process.env.FIXTURE_CONTEXT_MODE
      ? { verdict: "full_trust", graph_state: { node_count: 1 } }
      : { verdict: "needs_ingest", graph_state: { node_count: 0 } };
    if (name === "session_handshake") payload = process.env.FIXTURE_CONTEXT_MODE
      ? { trust_mode: "full_trust", graph_state: { node_count: 1 } }
      : { trust_mode: "needs_ingest", graph_state: { node_count: 0 } };
    if (name === "surgical_context_v2") {
      isError = process.env.FIXTURE_CONTEXT_MODE === "error";
      payload = { ok: false, refused: "context_unavailable", reason: "fixture context refusal" };
    }
    result = { isError, content: [{ type: "text", text: JSON.stringify(payload) }] };
  }
  process.stdout.write(JSON.stringify({ jsonrpc: "2.0", id: req.id, result }) + "\\n");
});
`, { mode: 0o700 });
  const cache = path.join(root, "cache");
  const env = { ...process.env, M1ND_AGENT_CACHE_DIR: cache, M1ND_REGISTRY_DIR: path.join(root, "registry"), FIXTURE_CALL_LOG: path.join(root, "calls.log") };
  return { root, repo, binary, env };
}

function run(f, command, extra = [], env = f.env) {
  const result = spawnSync(process.execPath,
    [CLI, "agent", command, "--repo", f.repo, "--binary", f.binary, "--no-attach", "--query", "signal.js", ...extra, "--json"],
    { encoding: "utf8", timeout: 15000, env });
  assert.equal(result.signal, null, result.stderr);
  assert.equal(result.error, undefined, result.error?.message);
  return { ...result, envelope: JSON.parse(result.stdout) };
}

test("failed cold bootstrap remains eligible for initial graph creation on the next invocation", POSIX_SHEBANG_ONLY, () => {
  const f = fixture();
  try {
    const first = run(f, "first-minute");
    assert.equal(first.status, 1, first.stderr);
    assert.equal(first.envelope.status, "needs_authority");
    const runtime = agentRuntimeCacheTarget(f.repo, f.env).runtimeDir;
    assert.equal(fs.existsSync(path.join(runtime, "graph_snapshot.json")), false);
    assert.equal(fs.existsSync(path.join(runtime, "agent-cache-identity.json")), true);
    fs.writeFileSync(f.env.FIXTURE_CALL_LOG, "");
    const second = run(f, "first-minute");
    assert.equal(second.status, 1, second.stderr);
    assert.equal(second.envelope.status, "needs_authority", "a missing graph must not be mistaken for a warm refresh refusal");
    assert.equal(second.envelope.trust.verdict, "needs_authority");
    assert.equal(fs.readFileSync(f.env.FIXTURE_CALL_LOG, "utf8").includes("ingest\n"), false,
      "refresh may only run after a graph was persisted");
  } finally { fs.rmSync(f.root, { recursive: true, force: true }); }
});

for (const mode of ["error", "refused"]) {
  test(`context ${mode} envelope is not a ready capsule`, POSIX_SHEBANG_ONLY, () => {
    const f = fixture();
    try {
      const result = run(f, "context", ["--anchor", "signal.js", "--shared-runtime"],
        { ...f.env, FIXTURE_CONTEXT_MODE: mode });
      assert.equal(result.status, 1, result.stderr);
      assert.equal(result.envelope.ok, false);
      assert.notEqual(result.envelope.action?.trigger?.kind, "context_capsule_ready");
      assert.notEqual(result.envelope.proof_boundary?.m1nd_proved,
        "m1nd built a bounded context capsule for a concrete source anchor");
      assert.equal(result.envelope.calls.at(-1).tool, "surgical_context_v2");
      assert.equal(result.envelope.calls.at(-1).ok, false);
    } finally { fs.rmSync(f.root, { recursive: true, force: true }); }
  });
}
