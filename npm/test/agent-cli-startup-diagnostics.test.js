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

test("automatic cache refuses source drift across lease, initialization, tools and shutdown", () => {
  const { execFileSync } = require("node:child_process");
  for (const mode of ["lease", "initialize", "tool_revert", "close", "stable"]) {
    const fixture = fs.mkdtempSync(path.join(os.tmpdir(), "m1nd-source-boundary-"));
    try {
      const repo = path.join(fixture, "repo-alpha"), state = path.join(fixture, "state");
      const preload = path.join(fixture, "boundary-fixture.cjs"), marker = path.join(fixture, "started");
      fs.mkdirSync(repo);
      const git = (...args) => execFileSync("git", ["-C", repo, ...args], { stdio: "pipe" }).toString().trim();
      git("init", "-q");
      fs.writeFileSync(path.join(repo, "signal.js"), "export const boundary_signal = 1;\n");
      git("add", ".");
      git("-c", "user.name=Fixture User", "-c", "user.email=fixture@example.invalid", "commit", "-qm", "initial");
      const initialBranch = git("symbolic-ref", "--short", "HEAD");
      const target = agentRuntimeCacheTarget(repo, { M1ND_AGENT_CACHE_DIR: path.join(state, "agent-runtimes") });
      fs.writeFileSync(preload, `
const fs = require('node:fs');
const { execFileSync } = require('node:child_process');
const { McpRuntimeClient } = require(${JSON.stringify(path.resolve(__dirname, "../lib/mcp-runtime-client"))});
const cache = require(${JSON.stringify(path.resolve(__dirname, "../lib/agent-runtime-cache"))});
const mode = process.env.BOUNDARY_MODE, repo = process.env.BOUNDARY_REPO;
const git = (...args) => execFileSync('git', ['-C', repo, ...args], { stdio: 'pipe' });
let changed = false;
function change() { if (!changed) { git('checkout', '-qb', 'changed-source'); changed = true; } }
const acquire = cache.acquireAgentRuntimeLease;
cache.acquireAgentRuntimeLease = async (...args) => { const lease = await acquire(...args); if (mode === 'lease') change(); return lease; };
McpRuntimeClient.prototype.start = async function () { fs.writeFileSync(process.env.BOUNDARY_MARKER, 'started'); if (mode === 'initialize') change(); return this; };
McpRuntimeClient.prototype.request = async function () { return { result: { tools: ['trust_selftest', 'session_handshake'].map(name => ({ name })) } }; };
McpRuntimeClient.prototype.callTool = async function () { if (mode === 'tool_revert') change(); return { result: { content: [{ type: 'text', text: JSON.stringify({ trust_mode: 'full_trust', verdict: 'full_trust' }) }] } }; };
McpRuntimeClient.prototype.close = async function () { if (mode === 'tool_revert' && changed) git('checkout', '-q', process.env.BOUNDARY_INITIAL_BRANCH); if (mode === 'close') change(); this.processClosed = true; this.closeStatus = { code: 0, signal: null }; };
`);
      const env = Object.fromEntries(Object.entries(process.env).filter(([key]) => !key.startsWith("M1ND_") && !key.startsWith("GIT_")));
      Object.assign(env, { NODE_OPTIONS: `--require=${preload}`, BOUNDARY_MODE: mode, BOUNDARY_REPO: repo,
        BOUNDARY_MARKER: marker, BOUNDARY_INITIAL_BRANCH: initialBranch });
      const run = spawnSync(process.execPath, [CLI, "agent", "trust", "--repo", repo, "--binary", process.execPath,
        "--no-attach", "--state-dir", state, "--json"], { env, encoding: "utf8", timeout: 15000 });
      assert.equal(run.signal, null, run.stderr);
      if (mode === "stable") {
        assert.equal(run.status, 0, run.stderr);
        const payload = JSON.parse(run.stdout);
        assert.equal(payload.runtime.source_identity.status, "verified_at_checkpoints");
        assert.equal(payload.runtime.source_identity.source_revision.branch, initialBranch);
        assert.equal(payload.runtime.source_identity.working_tree_contents, "not_attested_by_identity");
        assert.equal(payload.runtime.source_identity.atomic_snapshot, false);
      } else {
        assert.equal(run.status, 1, run.stdout);
        assert.match(run.stderr, /source identity changed/);
        assert.equal(run.stdout, "", "source drift must not print a successful trust envelope");
      }
      if (mode === "lease") {
        assert.equal(fs.existsSync(marker), false, "source drift after lease must stop before startup");
        assert.equal(fs.existsSync(path.join(target.runtimeDir, "agent-cache-identity.json")), false);
      }
      assert.equal(fs.existsSync(path.join(target.runtimeDir, ".agent-cache-owner-v1")), false);
      if (mode === "tool_revert") assert.equal(git("symbolic-ref", "--short", "HEAD"), initialBranch);
    } finally { fs.rmSync(fixture, { recursive: true, force: true }); }
  }
});

test("startup failure keeps the primary error when teardown also fails", POSIX_SHEBANG_ONLY, () => {
  const fixture = fs.mkdtempSync(path.join(os.tmpdir(), "m1nd-agent-startup-diagnostic-"));
  try {
    const repo = path.join(fixture, "repo-alpha");
    const binary = path.join(fixture, "failing-m1nd-mcp.js");
    const cache = path.join(fixture, "cache");
    fs.mkdirSync(repo, { recursive: true });
    fs.writeFileSync(path.join(repo, "signal.js"), "export const startup_signal = 1;\n");
    fs.writeFileSync(
      binary,
      `#!${process.execPath}\n` +
        `"use strict";\n` +
        `if (process.argv.includes("--version")) { console.log("m1nd-mcp 1.6.3"); process.exit(0); }\n` +
        `let answered = false;\n` +
        `process.stdin.on("data", () => {\n` +
        `  if (answered) return;\n` +
        `  answered = true;\n` +
        `  process.stderr.write("fixture-primary-stderr\\n");\n` +
        `  process.stdout.write("{not-json}\\n");\n` +
        `});\n` +
        `process.stdin.on("end", () => {\n` +
        `  process.stderr.write("fixture-cleanup-stderr\\n");\n` +
        `  process.exit(1);\n` +
        `});\n`,
      { mode: 0o700 }
    );
    const env = {
      HOME: path.join(fixture, "home"),
      TMPDIR: path.join(fixture, "tmp"),
      TMP: path.join(fixture, "tmp"),
      TEMP: path.join(fixture, "tmp"),
      PATH: process.env.PATH || "/usr/bin:/bin",
      XDG_CACHE_HOME: cache,
      M1ND_AGENT_CACHE_DIR: cache,
      M1ND_REGISTRY_DIR: path.join(fixture, "registry"),
      NPM_CONFIG_OFFLINE: "true",
      NPM_CONFIG_UPDATE_NOTIFIER: "false",
    };
    for (const key of ["HOME", "TMPDIR", "XDG_CACHE_HOME", "M1ND_REGISTRY_DIR"]) {
      fs.mkdirSync(env[key], { recursive: true });
    }
    const target = agentRuntimeCacheTarget(repo, env);
    const run = spawnSync(
      process.execPath,
      [CLI, "agent", "first-minute", "--repo", repo, "--binary", binary, "--query", "startup_signal", "--no-attach", "--json"],
      { encoding: "utf8", timeout: 15_000, env }
    );

    assert.equal(run.signal, null, run.stderr);
    assert.equal(run.status, 1, "the failed runtime was reported as success");
    assert.match(run.stderr, /invalid MCP JSON response/, "the primary protocol failure was lost");
    assert.match(run.stderr, /child teardown was not clean: exit=1 signal=none/, "cleanup failure was lost");
    assert.match(run.stderr, /fixture-primary-stderr/, "native stderr before the protocol failure was lost");
    assert.match(run.stderr, /fixture-cleanup-stderr/, "native stderr during cleanup was lost");
    assert.equal(
      fs.existsSync(path.join(target.runtimeDir, ".agent-cache-owner-v1")),
      false,
      "a child that has exited cannot strand its automatic cache lease"
    );
  } finally {
    fs.rmSync(fixture, { recursive: true, force: true });
  }
});

test("unconfirmed child close retains owner lease and blocks another CLI", POSIX_SHEBANG_ONLY, () => {
  const fixture = fs.mkdtempSync(path.join(os.tmpdir(), "m1nd-agent-unconfirmed-close-"));
  try {
    const repo = path.join(fixture, "repo");
    const binary = path.join(fixture, "failing-m1nd-mcp.js");
    const preload = path.join(fixture, "unconfirmed-close.cjs");
    const cache = path.join(fixture, "cache");
    fs.mkdirSync(repo);
    fs.writeFileSync(path.join(repo, "signal.js"), "export const signal = 1;\n");
    fs.writeFileSync(binary,
      `#!${process.execPath}\nprocess.stdin.once("data", () => process.stdout.write("{not-json}\\n")); process.stdin.on("end", () => process.exit(0));\n`,
      { mode: 0o700 });
    fs.writeFileSync(preload,
      `const { McpRuntimeClient } = require(${JSON.stringify(path.resolve(__dirname, "../lib/mcp-runtime-client.js"))});\n` +
      `McpRuntimeClient.prototype.closeAndWait = async function () { this.proc.stdin.end(); throw new Error("child streams did not close after EOF, SIGTERM, and SIGKILL"); };\n`);
    const env = {
      HOME: path.join(fixture, "home"), TMPDIR: path.join(fixture, "tmp"),
      PATH: process.env.PATH || "/usr/bin:/bin", XDG_CACHE_HOME: cache,
      M1ND_AGENT_CACHE_DIR: cache, M1ND_REGISTRY_DIR: path.join(fixture, "registry"),
      NODE_OPTIONS: `--require=${preload}`,
    };
    for (const key of ["HOME", "TMPDIR", "XDG_CACHE_HOME", "M1ND_REGISTRY_DIR"]) fs.mkdirSync(env[key], { recursive: true });
    const target = agentRuntimeCacheTarget(repo, env);
    const args = [CLI, "agent", "first-minute", "--repo", repo, "--binary", binary,
      "--query", "signal", "--no-attach", "--json"];
    const first = spawnSync(process.execPath, args, { encoding: "utf8", timeout: 15_000, env });
    assert.equal(first.status, 1, first.stderr);
    assert.match(first.stderr, /child streams did not close/);
    assert.equal(fs.existsSync(path.join(target.runtimeDir, ".agent-cache-owner-v1")), true,
      "without confirmed child termination the owner proof must remain");
    const second = spawnSync(process.execPath, args,
      { encoding: "utf8", timeout: 15_000, env: { ...env, NODE_OPTIONS: "", M1ND_AGENT_CACHE_OWNER_WAIT_MS: "1000" } });
    assert.equal(second.status, 1, second.stderr);
    assert.match(second.stderr, /agent runtime cache is busy/);
  } finally {
    fs.rmSync(fixture, { recursive: true, force: true });
  }
});

test(
  "asynchronous EACCES spawn failure reports the cause and releases its cache lease",
  { skip: process.platform === "win32" },
  () => {
    const fixture = fs.mkdtempSync(path.join(os.tmpdir(), "m1nd-agent-spawn-eacces-"));
    try {
      const repo = path.join(fixture, "repo-alpha");
      const binary = path.join(fixture, "not-executable-m1nd-mcp");
      const cache = path.join(fixture, "cache");
      fs.mkdirSync(repo, { recursive: true });
      fs.writeFileSync(path.join(repo, "signal.js"), "export const startup_signal = 1;\n");
      fs.writeFileSync(binary, "not executable\n", { mode: 0o600 });
      const env = {
        HOME: path.join(fixture, "home"),
        TMPDIR: path.join(fixture, "tmp"),
        TMP: path.join(fixture, "tmp"),
        TEMP: path.join(fixture, "tmp"),
        PATH: process.env.PATH || "/usr/bin:/bin",
        XDG_CACHE_HOME: cache,
        M1ND_AGENT_CACHE_DIR: cache,
        M1ND_REGISTRY_DIR: path.join(fixture, "registry"),
        NPM_CONFIG_OFFLINE: "true",
        NPM_CONFIG_UPDATE_NOTIFIER: "false",
      };
      for (const key of ["HOME", "TMPDIR", "XDG_CACHE_HOME", "M1ND_REGISTRY_DIR"]) {
        fs.mkdirSync(env[key], { recursive: true });
      }
      const target = agentRuntimeCacheTarget(repo, env);
      const run = spawnSync(
        process.execPath,
        [CLI, "agent", "first-minute", "--repo", repo, "--binary", binary, "--query", "startup_signal", "--no-attach", "--json"],
        { encoding: "utf8", timeout: 15_000, env }
      );

      assert.equal(run.signal, null, run.stderr);
      assert.equal(run.status, 1, `spawn failure exited ${run.status}: ${run.stderr}\n${run.stdout}`);
      assert.match(run.stderr, /m1nd-mcp spawn failed: EACCES/, run.stderr);
      assert.equal(
        fs.existsSync(path.join(target.runtimeDir, ".agent-cache-owner-v1")),
        false,
        "a failed spawn left a cache lease behind"
      );
    } finally {
      fs.rmSync(fixture, { recursive: true, force: true });
    }
  }
);

test(
  "synchronous ENOEXEC spawn failure reports the cause and releases its cache lease",
  () => {
    const fixture = fs.mkdtempSync(path.join(os.tmpdir(), "m1nd-agent-spawn-enoexec-"));
    try {
      const repo = path.join(fixture, "repo-alpha");
      const binary = path.join(fixture, "invalid-m1nd-mcp");
      const preload = path.join(fixture, "throw-enoexec.cjs");
      const cache = path.join(fixture, "cache");
      fs.mkdirSync(repo, { recursive: true });
      fs.writeFileSync(path.join(repo, "signal.js"), "export const startup_signal = 1;\n");
      fs.writeFileSync(binary, "not an executable format\n", { mode: 0o700 });
      fs.writeFileSync(preload, [
        `const childProcess = require("node:child_process");`,
        `const originalSpawn = childProcess.spawn;`,
        `childProcess.spawn = function(file) {`,
        `  if (file === ${JSON.stringify(binary)}) {`,
        `    const error = new Error("fixture-ENOEXEC");`,
        `    error.code = "ENOEXEC";`,
        `    throw error;`,
        `  }`,
        `  return originalSpawn.apply(this, arguments);`,
        `};`,
      ].join("\n"));
      const env = {
        NODE_OPTIONS: `--require=${preload}`,
        HOME: path.join(fixture, "home"),
        TMPDIR: path.join(fixture, "tmp"),
        TMP: path.join(fixture, "tmp"),
        TEMP: path.join(fixture, "tmp"),
        PATH: process.env.PATH || "/usr/bin:/bin",
        XDG_CACHE_HOME: cache,
        M1ND_AGENT_CACHE_DIR: cache,
        M1ND_REGISTRY_DIR: path.join(fixture, "registry"),
        NPM_CONFIG_OFFLINE: "true",
        NPM_CONFIG_UPDATE_NOTIFIER: "false",
      };
      for (const key of ["HOME", "TMPDIR", "XDG_CACHE_HOME", "M1ND_REGISTRY_DIR"]) {
        fs.mkdirSync(env[key], { recursive: true });
      }
      const target = agentRuntimeCacheTarget(repo, env);
      const run = spawnSync(
        process.execPath,
        [CLI, "agent", "first-minute", "--repo", repo, "--binary", binary, "--query", "startup_signal", "--no-attach", "--json"],
        { encoding: "utf8", timeout: 15_000, env }
      );

      assert.equal(run.signal, null, run.stderr);
      assert.equal(run.status, 1, `spawn failure exited ${run.status}: ${run.stderr}\n${run.stdout}`);
      assert.match(run.stderr, /m1nd-mcp spawn failed: ENOEXEC/, run.stderr);
      assert.equal(
        fs.existsSync(path.join(target.runtimeDir, ".agent-cache-owner-v1")),
        false,
        "a failed spawn left a cache lease behind"
      );
    } finally {
      fs.rmSync(fixture, { recursive: true, force: true });
    }
  }
);

test("stdin EPIPE rejects startup and releases its confirmed cache lease", POSIX_SHEBANG_ONLY, () => {
  const fixture = fs.mkdtempSync(path.join(os.tmpdir(), "m1nd-agent-stdin-epipe-"));
  try {
    const repo = path.join(fixture, "repo");
    const binary = path.join(fixture, "idle-m1nd-mcp.js");
    const preload = path.join(fixture, "broken-pipe.cjs");
    const cache = path.join(fixture, "cache");
    fs.mkdirSync(repo);
    fs.writeFileSync(path.join(repo, "signal.js"), "export const signal = 1;\n");
    fs.writeFileSync(binary,
      `#!${process.execPath}\n` +
      `if (process.argv.includes("--version")) { console.log("m1nd-mcp 1.6.4"); process.exit(0); }\n` +
      `process.stdin.resume();\n`, { mode: 0o700 });
    fs.writeFileSync(preload, [
      `const { McpRuntimeClient } = require(${JSON.stringify(path.resolve(__dirname, "../lib/mcp-runtime-client.js"))});`,
      `const realStart = McpRuntimeClient.prototype.start;`,
      `McpRuntimeClient.prototype.start = function () {`,
      `  const pendingStart = realStart.call(this);`,
      `  this.proc.prependOnceListener("spawn", () => {`,
      `    const stdin = this.proc.stdin;`,
      `    stdin.write = () => {`,
      `      process.nextTick(() => stdin.emit("error", Object.assign(new Error("fixture-EPIPE"), { code: "EPIPE" })));`,
      `      return false;`,
      `    };`,
      `  });`,
      `  return pendingStart;`,
      `};`,
    ].join("\n"));
    const env = {
      HOME: path.join(fixture, "home"), TMPDIR: path.join(fixture, "tmp"),
      PATH: process.env.PATH || "/usr/bin:/bin", XDG_CACHE_HOME: cache,
      M1ND_AGENT_CACHE_DIR: cache, M1ND_REGISTRY_DIR: path.join(fixture, "registry"),
      NODE_OPTIONS: `--require=${preload}`,
    };
    for (const key of ["HOME", "TMPDIR", "XDG_CACHE_HOME", "M1ND_REGISTRY_DIR"]) fs.mkdirSync(env[key], { recursive: true });
    const target = agentRuntimeCacheTarget(repo, env);
    const run = spawnSync(process.execPath,
      [CLI, "agent", "first-minute", "--repo", repo, "--binary", binary, "--query", "signal", "--no-attach", "--json"],
      { encoding: "utf8", timeout: 15_000, env });
    assert.equal(run.signal, null, run.stderr);
    assert.equal(run.status, 1, run.stderr);
    assert.doesNotMatch(run.stderr, /Unhandled 'error' event/, run.stderr);
    assert.match(run.stderr, /fixture-EPIPE/, run.stderr);
    assert.equal(fs.existsSync(path.join(target.runtimeDir, ".agent-cache-owner-v1")), false,
      "a failed startup with a closed child must not strand its cache lease");
  } finally {
    fs.rmSync(fixture, { recursive: true, force: true });
  }
});

test("reused-cache refresh refusal is an honest failing envelope before trust", POSIX_SHEBANG_ONLY, () => {
  const fixture = fs.mkdtempSync(path.join(os.tmpdir(), "m1nd-agent-refresh-refusal-"));
  try {
    const repo = path.join(fixture, "repo-alpha");
    const binary = path.join(fixture, "refusing-m1nd-mcp.js");
    const cache = path.join(fixture, "cache");
    fs.mkdirSync(repo, { recursive: true });
    fs.writeFileSync(path.join(repo, "signal.js"), "export const startup_signal = 1;\n");
    fs.writeFileSync(
      binary,
      `#!${process.execPath}\n` +
        `"use strict";\n` +
        `if (process.argv.includes("--version")) { console.log("m1nd-mcp 1.6.3"); process.exit(0); }\n` +
        `const readline = require("node:readline");\n` +
        `const rl = readline.createInterface({ input: process.stdin });\n` +
        `rl.on("line", (line) => {\n` +
        `  const req = JSON.parse(line);\n` +
        `  let result = {};\n` +
        `  if (req.method === "tools/list") result = { tools: [{ name: "ingest" }, { name: "trust_selftest" }, { name: "session_handshake" }] };\n` +
        `  if (req.method === "tools/call") {\n` +
        `    const payload = { ok: false, action: "graph.ingest.refresh_declared_root", refused: "refresh_would_shrink_graph", reason: "fixture refusal" };\n` +
        `    result = { isError: false, content: [{ type: "text", text: JSON.stringify(payload) }] };\n` +
        `  }\n` +
        `  process.stdout.write(JSON.stringify({ jsonrpc: "2.0", id: req.id, result }) + "\\n");\n` +
        `});\n`,
      { mode: 0o700 }
    );
    const env = {
      HOME: path.join(fixture, "home"),
      TMPDIR: path.join(fixture, "tmp"),
      TMP: path.join(fixture, "tmp"),
      TEMP: path.join(fixture, "tmp"),
      PATH: process.env.PATH || "/usr/bin:/bin",
      XDG_CACHE_HOME: cache,
      M1ND_AGENT_CACHE_DIR: cache,
      M1ND_REGISTRY_DIR: path.join(fixture, "registry"),
      NPM_CONFIG_OFFLINE: "true",
      NPM_CONFIG_UPDATE_NOTIFIER: "false",
    };
    for (const key of ["HOME", "TMPDIR", "XDG_CACHE_HOME", "M1ND_REGISTRY_DIR"]) {
      fs.mkdirSync(env[key], { recursive: true });
    }
    const target = agentRuntimeCacheTarget(repo, env);
    fs.mkdirSync(target.runtimeDir, { recursive: true });
    fs.writeFileSync(
      path.join(target.runtimeDir, "agent-cache-identity.json"),
      `${JSON.stringify(target.identity, null, 2)}\n`,
      { mode: 0o600 }
    );
    fs.writeFileSync(path.join(target.runtimeDir, "graph_snapshot.json"), "{}\n");

    const run = spawnSync(
      process.execPath,
      [CLI, "agent", "first-minute", "--repo", repo, "--binary", binary, "--query", "startup_signal", "--no-attach", "--json"],
      { encoding: "utf8", timeout: 15_000, env }
    );
    assert.equal(run.signal, null, run.stderr);
    assert.equal(run.status, 1, `refusal exited ${run.status}: ${run.stderr}\n${run.stdout}`);
    const envelope = JSON.parse(run.stdout);
    assert.equal(envelope.ok, false);
    assert.equal(envelope.status, "refresh_refused");
    assert.equal(envelope.trust.verdict, "not_evaluated");
    assert.equal(envelope.trust.freshness, "refresh_refused");
    assert.deepEqual(envelope.freshness, {
      isError: false,
      ok: false,
      action: "graph.ingest.refresh_declared_root",
      refused: "refresh_would_shrink_graph",
      reason: "fixture refusal",
    });
    assert.deepEqual(envelope.calls.map((call) => call.tool), ["tools/list", "ingest"]);
  } finally {
    fs.rmSync(fixture, { recursive: true, force: true });
  }
});
