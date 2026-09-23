"use strict";

const assert = require("node:assert/strict");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const test = require("node:test");
const {
  acquireAgentRuntimeLease,
  agentRuntimeCacheTarget,
  ensureAgentRuntimeIdentity,
  releaseAgentRuntimeLease,
} = require("../lib/agent-runtime-cache");
const symlinkTestOptions = {
  skip: process.platform === "win32" && "Windows symlink creation requires host privilege",
};

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
