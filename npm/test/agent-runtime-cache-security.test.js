"use strict";

const assert = require("node:assert/strict");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const test = require("node:test");
const {
  acquireAgentRuntimeLease,
  agentRuntimeCacheTarget,
  agentStatePaths,
  prepareAgentState,
  ensureAgentRuntimeIdentity,
  releaseAgentRuntimeLease,
} = require("../lib/agent-runtime-cache");
const symlinkTestOptions = {
  skip: process.platform === "win32" && "Windows symlink creation requires host privilege",
};

test("state-dir plans storage without writes and prepares private registry/cache siblings", () => {
  const fixture = fs.mkdtempSync(path.join(os.tmpdir(), "m1nd-state-plan-"));
  try {
    const repo = path.join(fixture, "repo-alpha");
    fs.mkdirSync(repo);
    const state = path.join(fixture, "private-state");
    const plan = agentStatePaths(repo, state);
    assert.equal(fs.existsSync(state), false);
    prepareAgentState(plan);
    const target = agentRuntimeCacheTarget(repo, { M1ND_AGENT_CACHE_DIR: plan.cache_base });
    assert.equal(path.dirname(target.runtimeDir), plan.cache_base);
    assert.equal(fs.statSync(plan.registry_root).isDirectory(), true);
    if (process.platform !== "win32") assert.equal(fs.statSync(plan.state_root).mode & 0o077, 0);
  } finally { fs.rmSync(fixture, { recursive: true, force: true }); }
});

test("state-dir rejects source-tree state before creating it", () => {
  const fixture = fs.mkdtempSync(path.join(os.tmpdir(), "m1nd-state-source-"));
  try {
    const state = path.join(fixture, "derived");
    assert.throws(() => agentStatePaths(fixture, state), /outside the indexed repository/);
    assert.equal(fs.existsSync(state), false);
  } finally { fs.rmSync(fixture, { recursive: true, force: true }); }
});

test("state-dir refuses symlink roots and a symlinked registry without adoption", symlinkTestOptions, () => {
  const fixture = fs.mkdtempSync(path.join(os.tmpdir(), "m1nd-state-symlink-"));
  try {
    const repo = path.join(fixture, "repo-alpha"), outside = path.join(fixture, "outside");
    const state = path.join(fixture, "state"), alias = path.join(fixture, "alias");
    for (const p of [repo, outside, state]) fs.mkdirSync(p, { mode: 0o700 });
    fs.symlinkSync(outside, alias, "dir");
    assert.throws(() => prepareAgentState(agentStatePaths(repo, alias)), /non-symlink/);
    fs.symlinkSync(outside, path.join(state, "registry"), "dir");
    assert.throws(() => prepareAgentState(agentStatePaths(repo, state)), /non-symlink/);
    const sourceAlias = path.join(fixture, "source-alias");
    fs.symlinkSync(repo, sourceAlias, "dir");
    assert.throws(() => agentStatePaths(repo, path.join(sourceAlias, "new-state")), /outside the indexed repository/);
    assert.deepEqual(fs.readdirSync(outside), []);
    assert.deepEqual(fs.readdirSync(repo), []);
  } finally { fs.rmSync(fixture, { recursive: true, force: true }); }
});

test("state-dir CLI scope is read-only and conflicting launch overrides fail before writes", () => {
  const { spawnSync } = require("node:child_process");
  const fixture = fs.mkdtempSync(path.join(os.tmpdir(), "m1nd-state-cli-"));
  try {
    const repo = path.join(fixture, "repo-alpha"), state = path.join(fixture, "state");
    fs.mkdirSync(repo);
    const env = Object.fromEntries(Object.entries(process.env).filter(([k]) => !k.startsWith("M1ND_")));
    const base = [path.resolve(__dirname, "../bin/m1nd.js"), "agent", "scope", "--repo", repo, "--binary", process.execPath, "--state-dir", state, "--json"];
    function run(extra, additions = {}) { return spawnSync(process.execPath, [...base, ...extra], { env: { ...env, ...additions }, encoding: "utf8", timeout: 10000 }); }
    const plan = run(["--no-attach"]);
    assert.equal(plan.status, 0, plan.stderr);
    assert.equal(JSON.parse(plan.stdout).runtime.state_selection.state_root, fs.realpathSync(fixture) + path.sep + "state");
    assert.equal(fs.existsSync(state), false);
    for (const extra of [[], ["--no-attach", "--shared-runtime"], ["--no-attach", "--runtime-dir", state]]) {
      const result = run(extra);
      assert.equal(result.status, 1);
      assert.match(result.stderr, /requires --no-attach/);
    }
    for (const key of ["M1ND_MCP_ARGS", "M1ND_RUNTIME_DIR", "M1ND_RUNTIME_BASE", "M1ND_GRAPH_SOURCE", "M1ND_PLASTICITY_STATE", "M1ND_ATTACH_URL"]) {
      const result = run(["--no-attach"], { [key]: "conflicting-override" });
      assert.equal(result.status, 1);
      assert.ok(result.stderr.includes(key), result.stderr);
      assert.equal(fs.existsSync(state), false);
    }
  } finally { fs.rmSync(fixture, { recursive: true, force: true }); }
});

test("state-dir continuation commands retain the selected isolated storage", () => {
  const { spawnSync } = require("node:child_process");
  const fixture = fs.mkdtempSync(path.join(os.tmpdir(), "m1nd-state-continuation-"));
  try {
    const repo = path.join(fixture, "repo-alpha"), state = path.join(fixture, "state with space");
    fs.mkdirSync(repo);
    fs.mkdirSync(path.join(repo, "src"));
    fs.writeFileSync(path.join(repo, "src/config.js"), "export const configuration = 1;\n");
    const env = Object.fromEntries(Object.entries(process.env).filter(([k]) => !k.startsWith("M1ND_")));
    for (const command of [["scope"], ["handoff"], ["auto", "--query", "locate configuration"],
      ["auto", "--query", "understand this repo"], ["next", "--query", "src/config.js"],
      ["recover", "--from", "transport_closed"], ["recover", "--from", "wrong_workspace_binding"]]) {
      const run = spawnSync(process.execPath, [path.resolve(__dirname, "../bin/m1nd.js"), "agent", ...command,
        "--repo", repo, "--binary", process.execPath, "--no-attach", "--state-dir", state, "--json"],
      { env, encoding: "utf8", timeout: 10000 });
      assert.equal(run.status, 0, run.stderr);
      const payload = JSON.parse(run.stdout);
      const instructions = [payload.action?.action?.command, payload.handoff?.resume_hint,
        ...(payload.next_actions || []),
        ...(payload.recovery_plan || []).map((step) => step.command)].filter((text) => /(?:^| )m1nd agent /.test(text || ""));
      assert.ok(instructions.length > 0, JSON.stringify(payload));
      for (const instruction of instructions) {
        assert.match(instruction, /--state-dir /, instruction);
        assert.match(instruction, /--no-attach(?: |$)/, instruction);
        assert.ok(instruction.includes("state with space"), instruction);
      }
    }
    assert.equal(fs.existsSync(state), false);
  } finally { fs.rmSync(fixture, { recursive: true, force: true }); }
});

test("runtime cache target refuses a symlinked cache base", symlinkTestOptions, () => {
  const fixture = fs.mkdtempSync(path.join(os.tmpdir(), "m1nd-agent-cache-base-symlink-"));
  try {
    const repo = path.join(fixture, "repo");
    const outside = path.join(fixture, "outside");
    const cacheBase = path.join(fixture, "cache");
    fs.mkdirSync(repo);
    fs.mkdirSync(outside);
    fs.symlinkSync(outside, cacheBase, "dir");

    assert.throws(
      () => agentRuntimeCacheTarget(repo, { ...process.env, M1ND_AGENT_CACHE_DIR: cacheBase }),
      /agent runtime cache base must be a non-symlink directory/
    );
  } finally {
    fs.rmSync(fixture, { recursive: true, force: true });
  }
});

test("runtime cache target refuses a cache base writable by group or other users", { skip: process.platform === "win32" }, () => {
  const fixture = fs.mkdtempSync(path.join(os.tmpdir(), "m1nd-agent-cache-insecure-base-"));
  try {
    const repo = path.join(fixture, "repo");
    const cacheBase = path.join(fixture, "cache");
    fs.mkdirSync(repo);
    fs.mkdirSync(cacheBase);
    fs.chmodSync(cacheBase, 0o777);

    assert.throws(
      () => agentRuntimeCacheTarget(repo, { ...process.env, M1ND_AGENT_CACHE_DIR: cacheBase }),
      /agent runtime cache base must not be writable by group or other users/
    );
  } finally {
    fs.rmSync(fixture, { recursive: true, force: true });
  }
});

test("runtime cache target refuses a cache base beneath a non-sticky writable parent", { skip: process.platform === "win32" }, () => {
  const fixture = fs.mkdtempSync(path.join(os.tmpdir(), "m1nd-agent-cache-insecure-parent-"));
  try {
    const repo = path.join(fixture, "repo");
    const parent = path.join(fixture, "writable-parent");
    const cacheBase = path.join(parent, "cache");
    fs.mkdirSync(repo);
    fs.mkdirSync(parent);
    fs.chmodSync(parent, 0o777);

    assert.throws(
      () => agentRuntimeCacheTarget(repo, { ...process.env, M1ND_AGENT_CACHE_DIR: cacheBase }),
      /agent runtime cache ancestor must not be writable by group or other users unless sticky/
    );
  } finally {
    fs.rmSync(fixture, { recursive: true, force: true });
  }
});

test("runtime cache target refuses a foreign-owned private ancestor", {
  skip: process.platform === "win32" || typeof process.getuid !== "function",
}, (t) => {
  const fixture = fs.mkdtempSync(path.join(os.tmpdir(), "m1nd-agent-cache-foreign-parent-"));
  try {
    const repo = path.join(fixture, "repo");
    const parent = path.join(fixture, "private-parent");
    const cacheBase = path.join(parent, "cache");
    fs.mkdirSync(repo);
    fs.mkdirSync(parent, { mode: 0o700 });
    const realStatSync = fs.statSync;
    const canonicalParent = fs.realpathSync(parent);
    const foreignUid = process.getuid() === 1 ? 2 : 1;
    t.mock.method(fs, "statSync", (target, ...args) => {
      const stat = realStatSync(target, ...args);
      return target === canonicalParent ? Object.assign(Object.create(stat), { uid: foreignUid }) : stat;
    });

    assert.throws(
      () => agentRuntimeCacheTarget(repo, { ...process.env, M1ND_AGENT_CACHE_DIR: cacheBase }),
      /agent runtime cache ancestor must be owned by the current user or root/
    );
  } finally {
    t.mock.restoreAll();
    fs.rmSync(fixture, { recursive: true, force: true });
  }
});

test("runtime cache rejects a symlinked runtime directory without writing through it", symlinkTestOptions, async () => {
  const fixture = fs.mkdtempSync(path.join(os.tmpdir(), "m1nd-agent-cache-symlink-"));
  try {
    const cacheRoot = path.join(fixture, "cache");
    const runtimeDir = path.join(cacheRoot, "runtime");
    const outside = path.join(fixture, "outside");
    fs.mkdirSync(cacheRoot, { recursive: true });
    fs.mkdirSync(outside, { recursive: true });
    fs.symlinkSync(outside, runtimeDir, "dir");

    await assert.rejects(
      acquireAgentRuntimeLease(runtimeDir),
      /agent runtime cache runtime directory must be a non-symlink directory/
    );
    assert.deepEqual(fs.readdirSync(outside), [], "cache lease wrote through the symlink");
  } finally {
    fs.rmSync(fixture, { recursive: true, force: true });
  }
});

test("cancelled cache waiter removes its waiter proof without acquiring another owner", async () => {
  const fixture = fs.mkdtempSync(path.join(os.tmpdir(), "m1nd-agent-cache-cancelled-waiter-"));
  const runtimeDir = path.join(fixture, "runtime");
  const holder = await acquireAgentRuntimeLease(runtimeDir);
  const ownerProof = path.join(holder.ownerDir, "owner.json");
  const ownerBefore = fs.readFileSync(ownerProof, "utf8");
  const interrupted = Object.assign(new Error("m1nd agent interrupted by SIGTERM"), {
    code: "M1ND_AGENT_INTERRUPTED",
    signal: "SIGTERM",
    exitCode: 143,
  });
  const cancellation = { error: null };
  const originalSetTimeout = global.setTimeout;
  let waiterObserved = false;
  try {
    global.setTimeout = (callback, delay, ...args) => {
      if (delay === 25 && !waiterObserved) {
        waiterObserved = true;
        cancellation.error = interrupted;
      }
      return originalSetTimeout(callback, delay, ...args);
    };
    await assert.rejects(
      acquireAgentRuntimeLease(runtimeDir, { cancellation }),
      (error) => error === interrupted
    );
    assert.equal(waiterObserved, true, "fixture did not reach the real lease waiter");
    assert.equal(fs.readFileSync(ownerProof, "utf8"), ownerBefore, "cancelled waiter modified the proven owner");
    assert.deepEqual(
      fs.readdirSync(runtimeDir).filter((name) => name.startsWith(".agent-cache-waiter-v1-")),
      [],
      "cancelled waiter left ownership state behind"
    );
  } finally {
    global.setTimeout = originalSetTimeout;
    releaseAgentRuntimeLease(holder);
    fs.rmSync(fixture, { recursive: true, force: true });
  }
});

test("runtime cache refuses a preexisting symlinked owner directory", symlinkTestOptions, async () => {
  const fixture = fs.mkdtempSync(path.join(os.tmpdir(), "m1nd-agent-cache-owner-symlink-"));
  try {
    const runtimeDir = path.join(fixture, "runtime");
    const outside = path.join(fixture, "outside-owner");
    fs.mkdirSync(runtimeDir, { recursive: true });
    fs.mkdirSync(outside, { recursive: true });
    fs.symlinkSync(outside, path.join(runtimeDir, ".agent-cache-owner-v1"), "dir");

    await assert.rejects(
      acquireAgentRuntimeLease(runtimeDir),
      /agent runtime cache owner directory must be a non-symlink directory/
    );
    assert.deepEqual(fs.readdirSync(outside), [], "cache lease wrote through the owner symlink");
  } finally {
    fs.rmSync(fixture, { recursive: true, force: true });
  }
});

test("runtime cache refuses a symlinked identity manifest", symlinkTestOptions, async () => {
  const fixture = fs.mkdtempSync(path.join(os.tmpdir(), "m1nd-agent-cache-identity-symlink-"));
  try {
    const runtimeDir = path.join(fixture, "runtime");
    const outsideIdentity = path.join(fixture, "outside-identity.json");
    const identity = {
      schema: "m1nd-agent-runtime-cache-v1",
      canonical_root: path.join(fixture, "repo"),
      source_revision: { kind: "non_git", head: null, branch: null },
    };
    const lease = await acquireAgentRuntimeLease(runtimeDir);
    fs.writeFileSync(outsideIdentity, `${JSON.stringify(identity)}\n`);
    fs.symlinkSync(outsideIdentity, path.join(runtimeDir, "agent-cache-identity.json"));

    assert.throws(
      () => ensureAgentRuntimeIdentity(lease, identity),
      /agent runtime cache identity must be a regular non-symlink file/
    );
    assert.equal(fs.readFileSync(outsideIdentity, "utf8"), `${JSON.stringify(identity)}\n`);
    releaseAgentRuntimeLease(lease);
  } finally {
    fs.rmSync(fixture, { recursive: true, force: true });
  }
});

test("runtime cache refuses a symlinked owner proof without creating identity state", symlinkTestOptions, async () => {
  const fixture = fs.mkdtempSync(path.join(os.tmpdir(), "m1nd-agent-cache-owner-proof-symlink-"));
  try {
    const runtimeDir = path.join(fixture, "runtime");
    const outsideProof = path.join(fixture, "outside-owner.json");
    const identity = {
      schema: "m1nd-agent-runtime-cache-v1",
      canonical_root: path.join(fixture, "repo"),
      source_revision: { kind: "non_git", head: null, branch: null },
    };
    const lease = await acquireAgentRuntimeLease(runtimeDir);
    const owner = { schema: "m1nd-agent-runtime-owner-v1", token: lease.token, pid: process.pid };
    const ownerProof = path.join(lease.ownerDir, "owner.json");
    fs.unlinkSync(ownerProof);
    fs.writeFileSync(outsideProof, `${JSON.stringify(owner)}\n`);
    fs.symlinkSync(outsideProof, ownerProof);

    assert.throws(
      () => ensureAgentRuntimeIdentity(lease, identity),
      /agent runtime cache owner proof must be a regular non-symlink file/
    );
    assert.equal(fs.existsSync(path.join(runtimeDir, "agent-cache-identity.json")), false);
    assert.equal(fs.readFileSync(outsideProof, "utf8"), `${JSON.stringify(owner)}\n`);
  } finally {
    fs.rmSync(fixture, { recursive: true, force: true });
  }
});

test("runtime cache release refuses an owner directory swapped for a symlink", symlinkTestOptions, async () => {
  const fixture = fs.mkdtempSync(path.join(os.tmpdir(), "m1nd-agent-cache-owner-swap-"));
  try {
    const runtimeDir = path.join(fixture, "runtime");
    const outsideOwner = path.join(fixture, "outside-owner");
    const lease = await acquireAgentRuntimeLease(runtimeDir);
    const owner = { schema: "m1nd-agent-runtime-owner-v1", token: lease.token, pid: process.pid };
    fs.mkdirSync(outsideOwner, { recursive: true });
    fs.writeFileSync(path.join(outsideOwner, "owner.json"), `${JSON.stringify(owner)}\n`);
    fs.rmSync(lease.ownerDir, { recursive: true, force: true });
    fs.symlinkSync(outsideOwner, lease.ownerDir, "dir");

    assert.throws(
      () => releaseAgentRuntimeLease(lease),
      /agent runtime cache owner proof is unreadable; refusing mutation/
    );
    assert.equal(fs.existsSync(path.join(outsideOwner, "owner.json")), true, "release unlinked the outside proof");
  } finally {
    fs.rmSync(fixture, { recursive: true, force: true });
  }
});
