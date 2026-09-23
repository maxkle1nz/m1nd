"use strict";

const crypto = require("crypto");
const fs = require("fs");
const os = require("os");
const path = require("path");
const { spawnSync } = require("child_process");

const CACHE_IDENTITY_SCHEMA = "m1nd-agent-runtime-cache-v1";
const CACHE_OWNER_SCHEMA = "m1nd-agent-runtime-owner-v1";
const CACHE_OWNER_DIR = ".agent-cache-owner-v1";
const CACHE_WAITER_PREFIX = ".agent-cache-waiter-v1-";
const DEFAULT_CACHE_OWNER_WAIT_MS = 60_000;
const MIN_CACHE_OWNER_WAIT_MS = 1_000;
const MAX_CACHE_OWNER_WAIT_MS = 300_000;

function cacheOwnerWaitMs(env = process.env) {
  const configured = env.M1ND_AGENT_CACHE_OWNER_WAIT_MS;
  if (configured === undefined || configured === "") return DEFAULT_CACHE_OWNER_WAIT_MS;
  const waitMs = Number(configured);
  if (!Number.isInteger(waitMs) || waitMs < MIN_CACHE_OWNER_WAIT_MS || waitMs > MAX_CACHE_OWNER_WAIT_MS) {
    throw new Error(
      "M1ND_AGENT_CACHE_OWNER_WAIT_MS must be an integer between " +
        `${MIN_CACHE_OWNER_WAIT_MS} and ${MAX_CACHE_OWNER_WAIT_MS} milliseconds`
    );
  }
  return waitMs;
}

function cacheLstat(target) {
  try {
    return fs.lstatSync(target);
  } catch (error) {
    if (error.code === "ENOENT") return null;
    throw error;
  }
}

function assertCacheDirectory(target, label) {
  const stat = cacheLstat(target);
  if (!stat || !stat.isDirectory() || stat.isSymbolicLink()) {
    throw new Error(`agent runtime cache ${label} must be a non-symlink directory`);
  }
}

function noFollowFlag() {
  return process.platform === "win32" ? 0 : fs.constants.O_NOFOLLOW || 0;
}

function readRegularCacheFile(target, label) {
  const initial = cacheLstat(target);
  if (!initial) {
    throw new Error(`agent runtime cache ${label} is unreadable; refusing mutation: ENOENT`);
  }
  if (!initial.isFile() || initial.isSymbolicLink()) {
    throw new Error(`agent runtime cache ${label} must be a regular non-symlink file`);
  }
  let descriptor;
  try {
    descriptor = fs.openSync(target, fs.constants.O_RDONLY | noFollowFlag());
    const opened = fs.fstatSync(descriptor);
    if (!opened.isFile()) {
      throw new Error(`agent runtime cache ${label} must be a regular non-symlink file`);
    }
    return fs.readFileSync(descriptor, "utf8");
  } catch (error) {
    if (String(error.message || "").startsWith(`agent runtime cache ${label}`)) throw error;
    if (error.code === "ELOOP") {
      throw new Error(`agent runtime cache ${label} must be a regular non-symlink file`);
    }
    throw new Error(`agent runtime cache ${label} is unreadable; refusing mutation: ${error.message}`);
  } finally {
    if (descriptor !== undefined) fs.closeSync(descriptor);
  }
}

function writeExclusiveCacheFile(target, contents) {
  let descriptor;
  try {
    descriptor = fs.openSync(
      target,
      fs.constants.O_WRONLY | fs.constants.O_CREAT | fs.constants.O_EXCL | noFollowFlag(),
      0o600
    );
    if (!fs.fstatSync(descriptor).isFile()) {
      throw new Error("agent runtime cache refused a non-regular manifest target");
    }
    fs.writeFileSync(descriptor, contents);
  } finally {
    if (descriptor !== undefined) fs.closeSync(descriptor);
  }
}

function gitResult(root, args) {
  return spawnSync("git", ["-C", root, ...args], {
    encoding: "utf8",
    timeout: 5000,
    stdio: ["ignore", "pipe", "pipe"],
    env: { ...process.env, LC_ALL: "C", LANG: "C" },
  });
}

function gitFailure(operation, result) {
  if (result.error) {
    const reason = result.error.code === "ETIMEDOUT" ? "timed out" : `is unavailable (${result.error.message})`;
    return new Error(`Git ${reason} while reading cache identity (${operation}); refusing cache adoption or creation`);
  }
  const detail = String(result.stderr || "").trim();
  return new Error(
    `Git failed while reading cache identity (${operation}, exit ${result.status ?? "unknown"})` +
      `${detail ? `: ${detail}` : ""}; refusing cache adoption or creation`
  );
}

function hasGitMetadataHint(root) {
  if (["GIT_DIR", "GIT_WORK_TREE", "GIT_COMMON_DIR"].some((key) => process.env[key])) return true;
  for (let current = root; ; current = path.dirname(current)) {
    try {
      fs.lstatSync(path.join(current, ".git"));
      return true;
    } catch (error) {
      if (error.code !== "ENOENT") {
        throw new Error(`Git metadata cannot be inspected for cache identity: ${error.message}`);
      }
    }
    if (path.dirname(current) === current) return false;
  }
}

function sourceRevision(root) {
  const repository = gitResult(root, ["rev-parse", "--is-inside-work-tree"]);
  if (repository.status !== 0) {
    const detail = String(repository.stderr || "");
    if (!repository.error && /not a git repository/i.test(detail) && !hasGitMetadataHint(root)) {
      return { kind: "non_git", head: null, branch: null };
    }
    throw gitFailure("repository probe", repository);
  }

  const branchResult = gitResult(root, ["symbolic-ref", "--quiet", "--short", "HEAD"]);
  let branch;
  if (branchResult.status === 0 && String(branchResult.stdout || "").trim()) {
    branch = String(branchResult.stdout).trim();
  } else if (branchResult.status === 1 && !branchResult.error && !String(branchResult.stderr || "").trim()) {
    branch = null;
  } else {
    throw gitFailure("branch query", branchResult);
  }

  const headResult = gitResult(root, ["rev-parse", "--verify", "HEAD"]);
  if (headResult.status === 0 && String(headResult.stdout || "").trim()) {
    return {
      kind: "git",
      head: String(headResult.stdout).trim(),
      branch: branch || "(detached)",
    };
  }
  const headError = String(headResult.stderr || "");
  if (branch && !headResult.error && headResult.status === 128 && /Needed a single revision/i.test(headError)) {
    return { kind: "git_unborn", head: null, branch };
  }
  throw gitFailure("HEAD query", headResult);
}

function defaultCacheBase(env = process.env) {
  if (env.M1ND_AGENT_CACHE_DIR) return path.resolve(env.M1ND_AGENT_CACHE_DIR);
  if (env.XDG_CACHE_HOME) return path.join(path.resolve(env.XDG_CACHE_HOME), "m1nd", "agent-runtimes");
  return path.join(os.homedir(), ".cache", "m1nd", "agent-runtimes");
}

function assertPrivateCacheAncestors(canonicalBase) {
  if (process.platform === "win32") return;
  let ancestor = path.dirname(canonicalBase);
  while (true) {
    const stat = fs.statSync(ancestor);
    if (!stat.isDirectory()) {
      throw new Error("agent runtime cache ancestor must be a directory");
    }
    // Even a mode-0700 foreign-owned parent can rebind the path to our cache.
    // Trust only our own or root-owned ancestors; root-owned sticky parents
    // (e.g. /tmp) protect our entries from other unprivileged users.
    // These path checks are not atomic with later opens: a same-UID process
    // (or root) can still race/rebind them. This is not a same-UID boundary.
    if (typeof process.getuid === "function" && stat.uid !== process.getuid() && stat.uid !== 0) {
      throw new Error("agent runtime cache ancestor must be owned by the current user or root");
    }
    const writableByGroupOrOther = (stat.mode & 0o022) !== 0;
    const sticky = (stat.mode & 0o1000) !== 0;
    if (writableByGroupOrOther && !sticky) {
      throw new Error(
        "agent runtime cache ancestor must not be writable by group or other users unless sticky"
      );
    }
    const parent = path.dirname(ancestor);
    if (parent === ancestor) return;
    ancestor = parent;
  }
}

function assertPrivateCacheBase(base) {
  fs.mkdirSync(base, { recursive: true, mode: 0o700 });
  assertCacheDirectory(base, "base");
  const canonical = fs.realpathSync(base);
  const stat = fs.statSync(canonical);
  if (typeof process.getuid === "function" && stat.uid !== process.getuid()) {
    throw new Error("agent runtime cache base must be owned by the current user");
  }
  if (process.platform !== "win32" && (stat.mode & 0o022) !== 0) {
    throw new Error("agent runtime cache base must not be writable by group or other users");
  }
  assertPrivateCacheAncestors(canonical);
  return canonical;
}

function sameIdentity(left, right) {
  return (
    left &&
    right &&
    left.schema === right.schema &&
    left.canonical_root === right.canonical_root &&
    left.source_revision &&
    right.source_revision &&
    left.source_revision.kind === right.source_revision.kind &&
    left.source_revision.head === right.source_revision.head &&
    left.source_revision.branch === right.source_revision.branch
  );
}

function readOwnerProof(lease) {
  let owner;
  try {
    assertCacheDirectory(lease.ownerDir, "owner directory");
    owner = JSON.parse(readRegularCacheFile(path.join(lease.ownerDir, "owner.json"), "owner proof"));
  } catch (error) {
    if (String(error.message || "").startsWith("agent runtime cache owner proof")) throw error;
    throw new Error(`agent runtime cache owner proof is unreadable; refusing mutation: ${error.message}`);
  }
  if (owner.schema !== CACHE_OWNER_SCHEMA || owner.token !== lease.token) {
    throw new Error("agent runtime cache owner proof does not match this process; refusing mutation");
  }
}

function ensureAgentRuntimeIdentity(lease, identity) {
  readOwnerProof(lease);
  const runtimeDir = lease.runtimeDir;
  assertCacheDirectory(runtimeDir, "runtime directory");
  const manifest = path.join(runtimeDir, "agent-cache-identity.json");
  if (cacheLstat(manifest)) {
    let existing;
    try {
      existing = JSON.parse(readRegularCacheFile(manifest, "identity"));
    } catch (error) {
      if (String(error.message || "").startsWith("agent runtime cache identity")) throw error;
      throw new Error(`agent runtime cache identity is unreadable; refusing reuse: ${error.message}`);
    }
    if (!sameIdentity(existing, identity)) {
      throw new Error("agent runtime cache identity mismatch; refusing to reuse, erase, or overwrite it");
    }
    return;
  }
  const unexpectedEntries = fs.readdirSync(runtimeDir).filter(
    (entry) => entry !== CACHE_OWNER_DIR && !entry.startsWith(CACHE_WAITER_PREFIX)
  );
  if (!lease.identityCreationAllowed || unexpectedEntries.length > 0) {
    throw new Error("agent runtime cache has derived state but no identity; refusing adoption or overwrite");
  }
  try {
    writeExclusiveCacheFile(manifest, `${JSON.stringify(identity, null, 2)}\n`);
  } catch (error) {
    if (error.code !== "EEXIST") throw error;
    const existing = JSON.parse(readRegularCacheFile(manifest, "identity"));
    if (!sameIdentity(existing, identity)) {
      throw new Error("agent runtime cache identity changed during creation; refusing reuse");
    }
  }
}

function agentRuntimeCacheTarget(repo, env = process.env) {
  const canonicalRoot = fs.realpathSync(repo);
  const identity = {
    schema: CACHE_IDENTITY_SCHEMA,
    canonical_root: canonicalRoot,
    source_revision: sourceRevision(canonicalRoot),
  };
  const key = crypto.createHash("sha256").update(JSON.stringify(identity)).digest("hex");
  const cacheBase = assertPrivateCacheBase(defaultCacheBase(env));
  const runtimeDir = path.join(cacheBase, key);
  return { runtimeDir, identity };
}

function cacheBusyError(runtimeDir, waitMs) {
  const error = new Error(
    `agent runtime cache is busy at ${runtimeDir}; the current owner did not release within ` +
      `${waitMs}ms, so its lock and state were preserved`
  );
  error.code = "M1ND_AGENT_CACHE_BUSY";
  return error;
}

function writeOwnerManifest(ownerDir, token) {
  assertCacheDirectory(ownerDir, "owner directory");
  writeExclusiveCacheFile(
    path.join(ownerDir, "owner.json"),
    `${JSON.stringify({ schema: CACHE_OWNER_SCHEMA, token, pid: process.pid }, null, 2)}\n`
  );
}

function tryAcquireAgentRuntimeLease(runtimeDir, token) {
  assertCacheDirectory(runtimeDir, "runtime directory");
  const ownerDir = path.join(runtimeDir, CACHE_OWNER_DIR);
  try {
    fs.mkdirSync(ownerDir, { mode: 0o700 });
  } catch (error) {
    if (error.code === "EEXIST") {
      assertCacheDirectory(ownerDir, "owner directory");
      return null;
    }
    throw error;
  }
  try {
    assertCacheDirectory(ownerDir, "owner directory");
    writeOwnerManifest(ownerDir, token);
  } catch (error) {
    // This process created the directory and is still its sole possible
    // owner. An empty directory is safe to retract before ownership begins.
    try {
      fs.rmdirSync(ownerDir);
    } catch (_) {
      // Preserve an unexpectedly changed directory rather than deleting it.
    }
    throw error;
  }
  return { runtimeDir, ownerDir, token };
}

function waitStep() {
  return new Promise((resolve) => setTimeout(resolve, 25));
}

async function acquireAgentRuntimeLease(runtimeDir) {
  const waitMs = cacheOwnerWaitMs();
  fs.mkdirSync(path.dirname(runtimeDir), { recursive: true, mode: 0o700 });
  let identityCreationAllowed = false;
  try {
    fs.mkdirSync(runtimeDir, { mode: 0o700 });
    identityCreationAllowed = true;
  } catch (error) {
    if (error.code !== "EEXIST") throw error;
    assertCacheDirectory(runtimeDir, "runtime directory");
    identityCreationAllowed = fs.readdirSync(runtimeDir).length === 0;
  }
  assertCacheDirectory(runtimeDir, "runtime directory");
  const token = crypto.randomBytes(24).toString("hex");
  const immediate = tryAcquireAgentRuntimeLease(runtimeDir, token);
  if (immediate) return { ...immediate, identityCreationAllowed };

  const waiter = path.join(runtimeDir, `${CACHE_WAITER_PREFIX}${process.pid}-${token}.json`);
  writeExclusiveCacheFile(
    waiter,
    `${JSON.stringify({ schema: CACHE_OWNER_SCHEMA, token, pid: process.pid }, null, 2)}\n`
  );
  const deadline = Date.now() + waitMs;
  try {
    while (Date.now() < deadline) {
      const lease = tryAcquireAgentRuntimeLease(runtimeDir, token);
      if (lease) return { ...lease, identityCreationAllowed };
      await waitStep();
    }
    throw cacheBusyError(runtimeDir, waitMs);
  } finally {
    try {
      fs.unlinkSync(waiter);
    } catch (error) {
      if (error.code !== "ENOENT") throw error;
    }
  }
}

function releaseAgentRuntimeLease(lease) {
  const manifest = path.join(lease.ownerDir, "owner.json");
  assertCacheDirectory(lease.runtimeDir, "runtime directory");
  readOwnerProof(lease);
  const entries = fs.readdirSync(lease.ownerDir);
  if (entries.length !== 1 || entries[0] !== "owner.json") {
    throw new Error("agent runtime cache owner directory changed; refusing recursive or foreign cleanup");
  }
  fs.unlinkSync(manifest);
  fs.rmdirSync(lease.ownerDir);
}

module.exports = {
  CACHE_IDENTITY_SCHEMA,
  acquireAgentRuntimeLease,
  agentRuntimeCacheTarget,
  cacheOwnerWaitMs,
  ensureAgentRuntimeIdentity,
  releaseAgentRuntimeLease,
};
