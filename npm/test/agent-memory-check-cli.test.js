"use strict";

const assert = require("node:assert/strict");
const crypto = require("node:crypto");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const { spawnSync } = require("node:child_process");
const test = require("node:test");
const { planAgentRuntimeCacheTarget } = require("../lib/agent-runtime-cache");
const CLI = path.resolve(__dirname, "../bin/m1nd.js");

function digest(root) {
  const hash = crypto.createHash("sha256");
  const visit = (directory) => {
    for (const name of fs.readdirSync(directory).sort()) {
      const file = path.join(directory, name), stat = fs.lstatSync(file);
      hash.update(path.relative(root, file));
      if (stat.isDirectory()) visit(file);
      else hash.update(fs.readFileSync(file));
    }
  };
  visit(root);
  return hash.digest("hex");
}

test("public memory-check never boots a native runtime or creates missing state", () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "m1nd-memory-check-cli-"));
  try {
    const repo = path.join(root, "repo-alpha"), state = path.join(root, "state");
    fs.mkdirSync(repo);
    const env = Object.fromEntries(Object.entries(process.env).filter(([key]) => !key.startsWith("M1ND_") && !key.startsWith("GIT_")));
    const run = spawnSync(process.execPath, [CLI, "agent", "memory-check", "--repo", repo,
      "--no-attach", "--state-dir", state, "--binary", path.join(root, "must-not-run"), "--json"], { env, encoding: "utf8" });
    assert.equal(run.status, 0, run.stderr);
    const result = JSON.parse(run.stdout);
    assert.equal(result.sharing_authorized, false);
    assert.equal(result.mutations, false);
    assert.equal(fs.existsSync(state), false);
  } finally { fs.rmSync(root, { recursive: true, force: true }); }
});

test("public memory-check reports legacy provenance without changing source or cache", {
  skip: !fs.constants.O_NOFOLLOW && "Memory review requires no-follow file reads",
}, () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "m1nd-memory-check-existing-"));
  try {
    const repo = path.join(root, "repo-alpha"), state = path.join(root, "state");
    fs.mkdirSync(path.join(repo, "src"), { recursive: true });
    fs.writeFileSync(path.join(repo, "src/signal.js"), "export const signal = 1;\n");
    const planned = planAgentRuntimeCacheTarget(repo, { M1ND_AGENT_CACHE_DIR: path.join(state, "agent-runtimes") });
    fs.mkdirSync(path.join(planned.runtimeDir, "agent-memory"), { recursive: true, mode: 0o700 });
    fs.writeFileSync(path.join(planned.runtimeDir, "agent-cache-identity.json"), JSON.stringify(planned.identity));
    fs.writeFileSync(path.join(planned.runtimeDir, "agent-memory", "memory.light.md"),
      "---\nProtocol: L1GHT/1.0\nNode: Memory\nCreated: 1\n---\n[⍂ entity: Signal]\n[𝔻 evidence: src/signal.js]\n");
    const before = digest(root);
    const env = Object.fromEntries(Object.entries(process.env).filter(([key]) => !key.startsWith("M1ND_") && !key.startsWith("GIT_")));
    const run = spawnSync(process.execPath, [CLI, "agent", "memory-check", "--repo", repo,
      "--no-attach", "--state-dir", state, "--binary", path.join(root, "must-not-run"), "--json"], { env, encoding: "utf8" });
    assert.equal(run.status, 0, run.stderr);
    const result = JSON.parse(run.stdout);
    assert.equal(result.authored_provenance, "absent");
    assert.equal(result.documents[0].status, "origin_not_attested");
    assert.equal(result.sharing_authorized, false);
    assert.equal(result.semantic_validity, "not_certified");
    assert.equal(digest(root), before);
  } finally { fs.rmSync(root, { recursive: true, force: true }); }
});
