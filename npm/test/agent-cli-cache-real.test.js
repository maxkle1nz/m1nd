"use strict";

const assert = require("node:assert/strict");
const crypto = require("node:crypto");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const { spawnSync } = require("node:child_process");
const test = require("node:test");

const CLI = path.resolve(__dirname, "../bin/m1nd.js");
const BINARY = process.env.M1ND_TEST_AGENT_CACHE_BINARY || "";
const EMBED_MODEL = process.env.M1ND_TEST_EMBED_MODEL || "";

test("state-dir rebuilds dirty edits, deletes and renames and isolates branch revisions across restarts", {
  skip: !BINARY || !EMBED_MODEL,
}, async () => {
  const fixture = fs.mkdtempSync(path.join(os.tmpdir(), "m1nd-branch-graph-matrix-"));
  try {
    const repo = path.join(fixture, "repo-alpha"), state = path.join(fixture, "state");
    writeFixture(repo, "branch_graph_signal");
    for (let i = 0; i < 8; i++) fs.writeFileSync(path.join(repo, `src/stable${i}.js`), `export function stable_${i}() { return ${i}; }\n`);
    fs.writeFileSync(path.join(repo, "src/removable.js"), "export function removed_symbol() { return 1; }\n");
    fs.writeFileSync(path.join(repo, "src/rename-me.js"), "export function renamed_file_symbol() { return 2; }\n");
    const env = Object.fromEntries(Object.entries(process.env).filter(([key]) => !key.startsWith("M1ND_") && !key.startsWith("GIT_")));
    env.M1ND_EMBED_MODEL = EMBED_MODEL;
    const vcs = (...args) => {
      const result = spawnSync("git", args, { cwd: repo, env: { ...env, GIT_CONFIG_NOSYSTEM: "1", GIT_CONFIG_GLOBAL: process.platform === "win32" ? "NUL" : "/dev/null" }, encoding: "utf8" });
      assert.equal(result.status, 0, result.stderr);
      return result.stdout.trim();
    };
    vcs("init", "-b", "graph-main");
    vcs("config", "user.name", "Fixture Author");
    vcs("config", "user.email", "fixture@example.invalid");
    const commit = (message) => { vcs("add", "."); vcs("commit", "-m", message); };
    commit("initial graph");
    const probe = (directory) => runFirstMinute(repo, "branch_graph_signal", env, { args: ["--no-attach", "--state-dir", directory] }).payload;
    const snapshot = (payload) => JSON.parse(fs.readFileSync(path.join(payload.runtime.runtime_root, "graph_snapshot.json"), "utf8"));
    const memory = async (runtimeDir, write) => {
      const { McpRuntimeClient, decodeToolResponse } = require("../lib/mcp-runtime-client");
      const { acquireAgentRuntimeLease, releaseAgentRuntimeLease } = require("../lib/agent-runtime-cache");
      const lease = await acquireAgentRuntimeLease(runtimeDir);
      const client = new McpRuntimeClient({ binary: BINARY, repo, runtimeDir,
        args: ["--stdio", "--no-gui"], env: { M1ND_EMBED_MODEL: EMBED_MODEL,
          M1ND_RUNTIME_BASE: runtimeDir, M1ND_GRAPH_SOURCE: path.join(runtimeDir, "graph_snapshot.json"),
          M1ND_PLASTICITY_STATE: path.join(runtimeDir, "plasticity_state.json"), M1ND_REGISTRY_DIR: path.join(state, "registry") } });
      try {
        await client.start();
        const decoded = decodeToolResponse(await client.callTool(write ? "memorize" : "seek", write
          ? { agent_id: "branch-fixture", node_label: "BranchMemory", ingest_after: true,
            claims: [{ label: "BranchMemory", kind: "entity", confidence: "high",
              text: "branch_graph_signal is the entrypoint in src/signal.js", evidence: ["src/signal.js"] }] }
          : { agent_id: "branch-fixture", query: "BranchMemory", top_k: 8 }));
        assert.equal(decoded.isError, false, JSON.stringify(decoded));
        if (write) {
          assert.equal(decoded.payload.ingested, true);
          assert.equal(decoded.payload.light_evidence_resolved, 1);
          return decoded.payload.path;
        }
        assert.ok(decoded.payload.results.some((item) => item.node_id?.startsWith("light::") && item.label?.includes("BranchMemory")));
      } finally {
        try { await client.close(); }
        finally { if (!client.spawned || client.processClosed) releaseAgentRuntimeLease(lease); }
      }
    };
    // Query learning intentionally changes current weights; compare the SOURCE
    // projection rather than treating those learned differences as code drift.
    const sourceProjection = (graph) => ({
      nodes: graph.nodes.filter((node) => !node.external_id.startsWith("light::")).map(({ external_id, label, node_type, tags, provenance }) =>
        [external_id, label, node_type, [...tags].sort(), provenance]).sort((a, b) => a[0].localeCompare(b[0])),
      edges: graph.edges.filter((edge) => !edge.source_id.startsWith("light::") && !edge.target_id.startsWith("light::")).map(({ source_id, target_id, relation, original_weight, direction, inhibitory, causal_strength }) =>
        [source_id, target_id, relation, original_weight, direction, inhibitory, causal_strength]).sort((a, b) => JSON.stringify(a).localeCompare(JSON.stringify(b))),
    });
    let epoch = 0;
    const compareRebuild = (payload) => {
      const rebuilt = probe(path.join(fixture, `baseline-${epoch++}`));
      assert.deepEqual(sourceProjection(snapshot(payload)), sourceProjection(snapshot(rebuilt)));
      return snapshot(payload);
    };
    const initial = probe(state), initialRuntime = initial.runtime.runtime_root;
    const memoryPath = await memory(initialRuntime, true), memoryDigest = sha256(memoryPath);
    const initialHead = vcs("rev-parse", "HEAD");
    fs.writeFileSync(path.join(repo, "src/signal.js"), "export function branch_graph_signal() { return 73; }\nexport function dirty_only_symbol() { return 5; }\n");
    const dirty = probe(state);
    assert.equal(dirty.runtime.runtime_root, initialRuntime);
    assert.equal(vcs("rev-parse", "HEAD"), initialHead);
    assert.equal(dirty.runtime.graph_snapshot_version.update_status, "refreshed");
    assert.ok(compareRebuild(dirty).nodes.some((node) => node.label === "dirty_only_symbol"));
    fs.unlinkSync(path.join(repo, "src/removable.js"));
    const deleted = compareRebuild(probe(state));
    assert.ok(!deleted.nodes.some((node) => node.label === "removed_symbol"));
    fs.renameSync(path.join(repo, "src/rename-me.js"), path.join(repo, "src/renamed.js"));
    const renamed = compareRebuild(probe(state));
    assert.ok(!renamed.nodes.some((node) => node.external_id.includes("rename-me.js")));
    assert.ok(renamed.nodes.some((node) => node.external_id.includes("renamed.js")));
    await memory(initialRuntime, false);
    commit("edited deleted renamed");
    const committed = probe(state), committedRuntime = committed.runtime.runtime_root;
    assert.notEqual(committedRuntime, initialRuntime);
    assert.ok(!snapshot(committed).nodes.some((node) => node.external_id.startsWith("light::")), "a new commit must not silently inherit branch-bound memory");
    const retained = sha256(path.join(initialRuntime, "graph_snapshot.json"));
    vcs("switch", "-c", "graph-feature");
    fs.writeFileSync(path.join(repo, "src/feature.js"), "export function feature_only_symbol() { return 91; }\n");
    commit("feature branch");
    const featureHead = vcs("rev-parse", "HEAD"), feature = probe(state);
    assert.notEqual(feature.runtime.runtime_root, committedRuntime);
    assert.ok(compareRebuild(feature).nodes.some((node) => node.label === "feature_only_symbol"));
    vcs("switch", "graph-main");
    const returned = probe(state);
    assert.equal(returned.runtime.runtime_root, committedRuntime);
    assert.ok(!compareRebuild(returned).nodes.some((node) => node.label === "feature_only_symbol"));
    vcs("branch", "-D", "graph-feature");
    assert.equal(probe(state).runtime.runtime_root, committedRuntime);
    assert.ok(fs.existsSync(feature.runtime.runtime_root), "deleting a branch must not erase retained memory/cache");
    vcs("merge", "--no-ff", featureHead, "-m", "merge feature");
    const merged = probe(state);
    assert.notEqual(merged.runtime.runtime_root, committedRuntime);
    assert.ok(compareRebuild(merged).nodes.some((node) => node.label === "feature_only_symbol"));
    assert.equal(sha256(path.join(initialRuntime, "graph_snapshot.json")), retained);
    assert.equal(sha256(memoryPath), memoryDigest, "old branch memory must remain intact");
  } finally { fs.rmSync(fixture, { recursive: true, force: true }); }
});

test("persisted graph version reports bytes and partial status without inventing freshness", () => {
  const { graphSnapshotVersion } = require("../lib/agent-graph-receipt");
  const fixture = fs.mkdtempSync(path.join(os.tmpdir(), "m1nd-graph-version-"));
  try {
    assert.equal(graphSnapshotVersion(fixture).status, "unavailable");
    const file = path.join(fixture, "graph_snapshot.json");
    fs.writeFileSync(file, '{"version":4,"nodes":[],"edges":[]}\n');
    const cold = graphSnapshotVersion(fixture);
    assert.equal(cold.sha256, sha256(file));
    assert.equal(cold.update_status, "not_observed");
    assert.equal(cold.semantic_completeness, "not_proven");
    const updated = graphSnapshotVersion(fixture, { attempted: true, ok: true, code_snapshot: { update_mode: "full_rebuild" } });
    assert.equal(updated.update_status, "refreshed");
    assert.equal(updated.code_snapshot.update_mode, "full_rebuild");
    const refused = graphSnapshotVersion(fixture, { attempted: true, ok: false, refused: "refresh_would_shrink_graph" });
    assert.equal(refused.update_status, "refused");
    assert.equal(refused.sha256, cold.sha256);
    fs.writeFileSync(file, '{"version":4,"nodes":[{}],"edges":[]}\n');
    assert.notEqual(graphSnapshotVersion(fixture).sha256, cold.sha256);
  } finally { fs.rmSync(fixture, { recursive: true, force: true }); }
});

test("persisted graph version refuses symlink snapshots", { skip: process.platform === "win32" }, () => {
  const { graphSnapshotVersion } = require("../lib/agent-graph-receipt");
  const fixture = fs.mkdtempSync(path.join(os.tmpdir(), "m1nd-graph-version-link-"));
  try {
    fs.writeFileSync(path.join(fixture, "outside.json"), "{}");
    fs.symlinkSync(path.join(fixture, "outside.json"), path.join(fixture, "graph_snapshot.json"));
    const receipt = graphSnapshotVersion(fixture);
    assert.equal(receipt.status, "unavailable");
    assert.equal(receipt.reason, "unsafe_snapshot_file");
    assert.equal(receipt.sha256, undefined);
  } finally { fs.rmSync(fixture, { recursive: true, force: true }); }
});

test("state-dir suggested commands reuse storage through a POSIX shell", {
  skip: !BINARY || process.platform === "win32"
    ? "requires explicit native binary and POSIX shell recipe" : false,
}, () => {
  const fixture = fs.mkdtempSync(path.join(os.tmpdir(), "m1nd-state-command-chain-"));
  try {
    const repo = path.join(fixture, "repo-alpha");
    const state = path.join(fixture, "state 'quoted' $(echo not-executed)");
    writeFixture(repo, "state_command_signal");
    const before = sourceSnapshot(repo);
    const env = Object.fromEntries(Object.entries(process.env).filter(([k]) => !k.startsWith("M1ND_")));
    if (EMBED_MODEL) env.M1ND_EMBED_MODEL = EMBED_MODEL;
    const scope = spawnSync(process.execPath, [CLI, "agent", "scope", "--repo", repo, "--binary", BINARY,
      "--no-attach", "--state-dir", state, "--json"], { env, encoding: "utf8", timeout: 10000 });
    assert.equal(scope.status, 0, scope.stderr);
    assert.equal(fs.existsSync(state), false);
    const quote = (text) => `'${text.replace(/'/g, `'\\''`)}'`;
    const execute = (command) => {
      assert.ok(command.startsWith("m1nd agent "), command);
      const script = `${quote(process.execPath)} ${quote(CLI)} ${command.slice("m1nd ".length)}`;
      const result = spawnSync("sh", ["-c", script], { env, encoding: "utf8", timeout: 60000 });
      assert.equal(result.status, 0, result.stderr);
      return JSON.parse(result.stdout);
    };
    const trust = execute(JSON.parse(scope.stdout).action.action.command);
    const orient = execute(trust.action.action.command);
    assert.equal(trust.runtime.runtime_root, orient.runtime.runtime_root);
    assert.equal(orient.runtime.state_selection.state_root, fs.realpathSync(state));
    assert.equal(path.dirname(orient.runtime.runtime_root), path.join(fs.realpathSync(state), "agent-runtimes"));
    const empty = spawnSync(process.execPath, [CLI, "agent", "first-minute", "--repo", repo, "--binary", BINARY,
      "--no-attach", "--state-dir", state, "--tool", "search", "--query", "zzzz_absent_state_probe_987654321", "--json"],
    { env, encoding: "utf8", timeout: 60000 });
    assert.equal(empty.status, 0, empty.stderr);
    const recovery = JSON.parse(empty.stdout);
    assert.match(recovery.recommended_next_command, /m1nd agent recover /);
    const recipe = recovery.next_actions.find((text) => text.includes("pipe this JSON"));
    assert.ok(recipe.includes(recovery.recommended_next_command), recipe);
    assert.deepEqual(sourceSnapshot(repo), before);
    assert.equal(fs.existsSync(path.join(orient.runtime.runtime_root, ".agent-cache-owner-v1")), false);
  } finally { fs.rmSync(fixture, { recursive: true, force: true }); }
});

test("explicit state-dir contains native cache and registry without ambient fallback", {
  skip: !BINARY && "set M1ND_TEST_AGENT_CACHE_BINARY to an explicit native binary",
}, () => {
  const fixture = fs.mkdtempSync(path.join(os.tmpdir(), "m1nd-explicit-state-real-"));
  try {
    const repo = path.join(fixture, "repo-alpha");
    writeFixture(repo, "state_directory_signal");
    const before = sourceSnapshot(repo);
    const state = path.join(fixture, "state"), unusedCache = path.join(fixture, "unused-cache");
    const unusedRegistry = path.join(fixture, "unused-registry");
    const env = Object.fromEntries(Object.entries(process.env).filter(([k]) => !k.startsWith("M1ND_")));
    Object.assign(env, { M1ND_AGENT_CACHE_DIR: unusedCache, M1ND_REGISTRY_DIR: unusedRegistry });
    if (EMBED_MODEL) env.M1ND_EMBED_MODEL = EMBED_MODEL;
    const options = { args: ["--no-attach", "--state-dir", state] };
    const cold = runFirstMinute(repo, "state_directory_signal", env, options);
    const warm = runFirstMinute(repo, "state_directory_signal", env, options);
    assert.equal(cold.payload.runtime.runtime_root, warm.payload.runtime.runtime_root);
    const selection = cold.payload.runtime.state_selection;
    assert.equal(selection.state_root, fs.realpathSync(state));
    assert.equal(path.dirname(cold.payload.runtime.runtime_root), selection.cache_base);
    assert.equal(fs.existsSync(path.join(cold.payload.runtime.runtime_root, "graph_snapshot.json")), true);
    assert.equal(fs.statSync(selection.registry_root).isDirectory(), true);
    assert.equal(fs.existsSync(unusedCache), false);
    assert.equal(fs.existsSync(unusedRegistry), false);
    assert.equal(fs.existsSync(path.join(cold.payload.runtime.runtime_root, ".agent-cache-owner-v1")), false);
    assert.deepEqual(sourceSnapshot(repo), before);
  } finally { fs.rmSync(fixture, { recursive: true, force: true }); }
});

function sha256(file) {
  return crypto.createHash("sha256").update(fs.readFileSync(file)).digest("hex");
}

function writeFixture(root, symbol) {
  fs.mkdirSync(path.join(root, "src"), { recursive: true });
  fs.writeFileSync(
    path.join(root, "package.json"),
    `${JSON.stringify({ name: path.basename(root), version: "0.0.0" }, null, 2)}\n`
  );
  fs.writeFileSync(path.join(root, "src", "signal.js"), `export function ${symbol}() { return 42; }\n`);
}

function sourceSnapshot(root) {
  return ["package.json", path.join("src", "signal.js")].map((relative) => {
    const file = path.join(root, relative);
    return { relative, mode: fs.statSync(file).mode, bytes: fs.readFileSync(file).toString("base64") };
  });
}

function treeDigest(root) {
  const hash = crypto.createHash("sha256");
  function visit(dir) {
    for (const name of fs.readdirSync(dir).sort()) {
      const file = path.join(dir, name);
      const relative = path.relative(root, file);
      const stat = fs.lstatSync(file);
      hash.update(relative).update("\0").update(String(stat.mode)).update("\0");
      if (stat.isDirectory()) visit(file);
      else hash.update(fs.readFileSync(file));
    }
  }
  visit(root);
  return hash.digest("hex");
}

function runFirstMinute(repo, symbol, env, options = {}) {
  const run = spawnSync(
    process.execPath,
    [
      CLI,
      "agent",
      "first-minute",
      "--repo",
      repo,
      "--binary",
      BINARY,
      "--query",
      symbol,
      ...(options.args || []),
      "--json",
    ],
    { encoding: "utf8", timeout: 60_000, env, cwd: options.cwd }
  );
  assert.equal(run.error, undefined, run.error && run.error.message);
  assert.equal(run.signal, null, `command timed out or was signalled: ${run.stderr}`);
  assert.equal(run.status, 0, `first-minute failed (${run.status}): ${run.stderr}\n${run.stdout}`);
  const payload = JSON.parse(run.stdout);
  assert.equal(payload.ok, true, JSON.stringify(payload));
  const matches = payload.results.flatMap((result) =>
    Array.isArray(result.results) ? result.results : []
  );
  const rendered = JSON.stringify(matches);
  assert.match(rendered, new RegExp(symbol));
  assert.match(rendered, /src[/\\]signal\.js/);
  return { payload, matches, stderr: run.stderr, status: run.status };
}

function runFirstMinuteFailure(repo, symbol, env) {
  const run = spawnSync(
    process.execPath,
    [CLI, "agent", "first-minute", "--repo", repo, "--binary", BINARY, "--query", symbol, "--json"],
    { encoding: "utf8", timeout: 15_000, env }
  );
  assert.equal(run.signal, null, `command timed out or was signalled: ${run.stderr}`);
  assert.notEqual(run.status, 0, `ambiguous Git failure was accepted:\n${run.stdout}`);
  assert.match(run.stderr, /git.*(failed|unavailable|timed out|identity)/i);
  return run;
}

function isolatedEnv(fixture, additions = {}) {
  const home = path.join(fixture, "home");
  const temp = path.join(fixture, "tmp");
  const registry = path.join(fixture, "registry");
  const cache = path.join(fixture, "cache");
  for (const dir of [home, temp, registry, cache]) fs.mkdirSync(dir, { recursive: true });
  return {
    HOME: home,
    TMPDIR: temp,
    TMP: temp,
    TEMP: temp,
    PATH: process.env.PATH || "/usr/bin:/bin",
    XDG_CACHE_HOME: cache,
    M1ND_AGENT_CACHE_DIR: cache,
    M1ND_REGISTRY_DIR: registry,
    M1ND_EMBED_MODEL: EMBED_MODEL,
    NPM_CONFIG_UPDATE_NOTIFIER: "false",
    NPM_CONFIG_OFFLINE: "true",
    ...additions,
  };
}

function git(repo, args) {
  const run = spawnSync("git", args, {
    cwd: repo,
    encoding: "utf8",
    env: {
      HOME: repo,
      PATH: process.env.PATH || "/usr/bin:/bin",
      GIT_CONFIG_NOSYSTEM: "1",
      GIT_CONFIG_GLOBAL: "/dev/null",
      LC_ALL: "C",
    },
  });
  assert.equal(run.status, 0, `git ${args.join(" ")} failed: ${run.stderr}`);
  return run.stdout.trim();
}

function writeFakeGit(binDir, realGit) {
  fs.mkdirSync(binDir, { recursive: true });
  const script = path.join(binDir, "git");
  fs.writeFileSync(
    script,
    `#!${process.execPath}\n` +
      `const { spawnSync } = require("node:child_process");\n` +
      `const args = process.argv.slice(2);\n` +
      `const mode = process.env.FAKE_GIT_MODE;\n` +
      `if (mode === "timeout") setTimeout(() => {}, 30000);\n` +
      `else if (mode === "generic") { process.stderr.write("simulated git metadata failure\\n"); process.exit(2); }\n` +
      `else if (mode === "branch" && args.includes("symbolic-ref")) { process.stderr.write("simulated branch query failure\\n"); process.exit(2); }\n` +
      `else { const r = spawnSync(${JSON.stringify(realGit)}, args, { stdio: "inherit", env: process.env }); process.exit(r.status === null ? 2 : r.status); }\n`,
    { mode: 0o700 }
  );
}

function readRoots(runtimeDir) {
  return JSON.parse(fs.readFileSync(path.join(runtimeDir, "ingest_roots.json"), "utf8"));
}

function snapshotText(runtimeDir) {
  return fs.readFileSync(path.join(runtimeDir, "graph_snapshot.json"), "utf8");
}

test(
  "public first-minute refreshes a reused cache before retrieval without changing Git identity",
  { skip: !BINARY || !EMBED_MODEL, timeout: 180_000 },
  () => {
    const fixture = fs.mkdtempSync(path.join(os.tmpdir(), "m1nd-agent-cache-freshness-real-"));
    try {
      for (const kind of ["non-git", "git-dirty"]) {
        const sandbox = path.join(fixture, kind);
        const repo = path.join(sandbox, "repo");
        const oldSymbol = `${kind.replace(/-/g, "_")}_old_signal_a91f`;
        const newSymbol = `${kind.replace(/-/g, "_")}_new_signal_b72e`;
        writeFixture(repo, oldSymbol);
        if (kind === "git-dirty") {
          git(repo, ["init"]);
          git(repo, ["add", "."]);
          git(repo, ["-c", "user.name=Test User", "-c", "user.email=test@example.invalid", "commit", "-m", "fixture"]);
        }
        const branchBefore = kind === "git-dirty" ? git(repo, ["branch", "--show-current"]) : null;
        const headBefore = kind === "git-dirty" ? git(repo, ["rev-parse", "HEAD"]) : null;
        const env = isolatedEnv(sandbox);

        const first = runFirstMinute(repo, oldSymbol, env);
        const runtime = first.payload.runtime.runtime_root;
        const canonicalRoot = fs.realpathSync(repo);
        assert.deepEqual(readRoots(runtime), [canonicalRoot]);
        assert.match(snapshotText(runtime), new RegExp(oldSymbol));

        fs.writeFileSync(
          path.join(repo, "src", "signal.js"),
          `export function ${newSymbol}() { return 42; }\n`
        );
        const second = runFirstMinute(repo, newSymbol, env);
        assert.equal(second.payload.runtime.runtime_root, runtime, `${kind} selected a new runtime`);
        const snapshot = snapshotText(runtime);
        assert.match(snapshot, new RegExp(newSymbol), `${kind} reused snapshot did not absorb the live edit`);
        assert.doesNotMatch(snapshot, new RegExp(oldSymbol), `${kind} reused snapshot retained the replaced symbol`);
        assert.deepEqual(readRoots(runtime), [canonicalRoot]);

        const refresh = second.payload.calls.find((call) => call.tool === "ingest");
        assert.ok(refresh, `${kind} did not expose its refresh call`);
        assert.equal(refresh.isError, false, JSON.stringify(refresh));
        assert.equal(refresh.ok, true, JSON.stringify(refresh));
        assert.equal(refresh.refused, undefined, JSON.stringify(refresh));
        assert.equal(refresh.action, "graph.ingest.refresh_declared_root", JSON.stringify(refresh));

        if (kind === "git-dirty") {
          assert.equal(git(repo, ["branch", "--show-current"]), branchBefore);
          assert.equal(git(repo, ["rev-parse", "HEAD"]), headBefore);
        }
      }
    } finally {
      fs.rmSync(fixture, { recursive: true, force: true });
    }
  }
);

test(
  "public first-minute reuses one root-bound cache across processes and isolates distinct roots",
  { skip: !BINARY || !EMBED_MODEL },
  () => {
    const fixture = fs.mkdtempSync(path.join(os.tmpdir(), "m1nd-agent-cache-real-"));
    try {
      const repoAlpha = path.join(fixture, "repo-alpha");
      const repoBeta = path.join(fixture, "repo-beta");
      const alphaSymbol = "alpha_cache_signal_5f91";
      const betaSymbol = "beta_cache_signal_9c42";
      writeFixture(repoAlpha, alphaSymbol);
      writeFixture(repoBeta, betaSymbol);
      const alphaBefore = sourceSnapshot(repoAlpha);
      const betaBefore = sourceSnapshot(repoBeta);

      const env = isolatedEnv(fixture);
      const cache = fs.realpathSync(env.M1ND_AGENT_CACHE_DIR);

      const alphaFirst = runFirstMinute(repoAlpha, alphaSymbol, env);
      const alphaRuntime = alphaFirst.payload.runtime.runtime_root;
      assert.equal(typeof alphaRuntime, "string");
      const snapshot = path.join(alphaRuntime, "graph_snapshot.json");
      const firstSnapshotHash = sha256(snapshot);
      const firstSnapshotBirth = fs.statSync(snapshot).birthtimeMs;
      assert.deepEqual(readRoots(alphaRuntime), [fs.realpathSync(repoAlpha)]);
      assert.deepEqual(
        JSON.parse(fs.readFileSync(path.join(alphaRuntime, "agent-cache-identity.json"), "utf8")),
        {
          schema: "m1nd-agent-runtime-cache-v1",
          canonical_root: fs.realpathSync(repoAlpha),
          source_revision: { kind: "non_git", head: null, branch: null },
        }
      );

      const alphaSecond = runFirstMinute(repoAlpha, alphaSymbol, env);
      assert.equal(
        alphaSecond.payload.runtime.runtime_root,
        alphaRuntime,
        `separate public invocations used distinct runtimes: ${alphaRuntime} vs ${alphaSecond.payload.runtime.runtime_root}`
      );
      assert.ok(alphaRuntime.startsWith(cache + path.sep), alphaRuntime);
      assert.equal(sha256(snapshot), firstSnapshotHash);
      assert.ok(fs.statSync(snapshot).birthtimeMs >= firstSnapshotBirth);
      assert.deepEqual(readRoots(alphaRuntime), [fs.realpathSync(repoAlpha)]);

      const beta = runFirstMinute(repoBeta, betaSymbol, env);
      const betaRuntime = beta.payload.runtime.runtime_root;
      assert.notEqual(betaRuntime, alphaRuntime);
      assert.deepEqual(readRoots(betaRuntime), [fs.realpathSync(repoBeta)]);
      assert.deepEqual(readRoots(alphaRuntime), [fs.realpathSync(repoAlpha)]);
      assert.doesNotMatch(JSON.stringify(beta.matches), new RegExp(alphaSymbol));
      assert.doesNotMatch(JSON.stringify(alphaSecond.matches), new RegExp(betaSymbol));

      for (const option of ["--runtime-dir ../relative-runtime", "--runtime-dir=../relative-runtime"]) {
        const explicit = runFirstMinute(repoAlpha, alphaSymbol, {
          ...env,
          M1ND_MCP_ARGS: `--stdio --no-gui ${option}`,
        });
        const expected = path.join(fixture, "relative-runtime");
        assert.equal(fs.realpathSync(explicit.payload.runtime.runtime_root), fs.realpathSync(expected));
        assert.deepEqual(readRoots(expected), [fs.realpathSync(repoAlpha)]);
      }

      for (const [index, option] of ["--runtime-dir env-runtime", "--runtime-dir=env-runtime"].entries()) {
        const invocation = path.join(fixture, `precedence-${index}`);
        fs.mkdirSync(invocation);
        const explicit = runFirstMinute(
          repoAlpha,
          alphaSymbol,
          { ...env, M1ND_MCP_ARGS: `--stdio --no-gui ${option}` },
          { cwd: invocation, args: ["--runtime-dir", "cli-runtime"] }
        );
        const cliRuntime = path.join(invocation, "cli-runtime");
        const losingRuntime = path.join(repoAlpha, "env-runtime");
        assert.equal(fs.realpathSync(explicit.payload.runtime.runtime_root), fs.realpathSync(cliRuntime));
        assert.deepEqual(readRoots(cliRuntime), [fs.realpathSync(repoAlpha)]);
        assert.equal(fs.existsSync(losingRuntime), false, "losing ambient runtime received state");
      }

      assert.deepEqual(fs.readdirSync(cache).sort(), [path.basename(alphaRuntime), path.basename(betaRuntime)].sort());

      assert.deepEqual(sourceSnapshot(repoAlpha), alphaBefore);
      assert.deepEqual(sourceSnapshot(repoBeta), betaBefore);
      assert.equal(fs.existsSync(path.join(repoAlpha, ".m1nd")), false);
      assert.equal(fs.existsSync(path.join(repoBeta, ".m1nd")), false);
    } finally {
      fs.rmSync(fixture, { recursive: true, force: true });
    }
  }
);

test(
  "public first-minute refuses corrupt Git metadata without creating cache state",
  { skip: !BINARY || !EMBED_MODEL },
  () => {
    const fixture = fs.mkdtempSync(path.join(os.tmpdir(), "m1nd-agent-cache-corrupt-git-"));
    try {
      for (const kind of ["file", "directory", "ancestor", "environment"]) {
        const base = path.join(fixture, kind);
        const repo = path.join(base, "source");
        const symbol = "corrupt_git_identity_signal_31b7";
        writeFixture(repo, symbol);
        const env = isolatedEnv(base);
        if (kind === "directory") fs.mkdirSync(path.join(repo, ".git"));
        else if (kind === "environment") env.GIT_DIR = path.join(base, "missing-git-directory");
        else fs.writeFileSync(
          path.join(kind === "ancestor" ? base : repo, ".git"),
          "gitdir: ../missing-git-directory\n"
        );
        const before = treeDigest(env.M1ND_AGENT_CACHE_DIR);
        const run = spawnSync(process.execPath,
          [CLI, "agent", "first-minute", "--repo", repo, "--binary", BINARY, "--query", symbol, "--json"],
          { encoding: "utf8", timeout: 15_000, env });
        assert.equal(run.error, undefined);
        assert.equal(run.signal, null);
        assert.equal(treeDigest(env.M1ND_AGENT_CACHE_DIR), before, `${kind} metadata failure wrote cache state`);
        assert.notEqual(run.status, 0, "corrupt metadata was accepted");
        assert.match(run.stderr, /git.*(failed|unavailable|timed out|identity)/i);
      }
    } finally {
      fs.rmSync(fixture, { recursive: true, force: true });
    }
  }
);

test(
  "public first-minute refuses ambiguous Git failures without touching an existing cache",
  { skip: !BINARY || !EMBED_MODEL, timeout: 90_000 },
  () => {
    const fixture = fs.mkdtempSync(path.join(os.tmpdir(), "m1nd-agent-cache-git-"));
    try {
      const repo = path.join(fixture, "repo");
      const symbol = "git_identity_signal_74c1";
      writeFixture(repo, symbol);
      git(repo, ["init"]);
      git(repo, ["add", "."]);
      git(repo, ["-c", "user.name=Test User", "-c", "user.email=test@example.invalid", "commit", "-m", "fixture"]);
      const env = isolatedEnv(fixture);
      const first = runFirstMinute(repo, symbol, env);
      const runtime = first.payload.runtime.runtime_root;
      const before = treeDigest(runtime);
      const realGit = spawnSync("which", ["git"], { encoding: "utf8" }).stdout.trim();
      const fakeBin = path.join(fixture, "fake-bin");
      writeFakeGit(fakeBin, realGit);

      for (const mode of ["generic", "branch", "timeout"]) {
        runFirstMinuteFailure(repo, symbol, { ...env, PATH: fakeBin, FAKE_GIT_MODE: mode });
        assert.equal(treeDigest(runtime), before, `${mode} failure changed the existing cache`);
      }
      runFirstMinuteFailure(repo, symbol, { ...env, PATH: path.join(fixture, "missing-bin") });
      assert.equal(treeDigest(runtime), before, "unavailable Git changed the existing cache");

      git(repo, ["checkout", "--detach"]);
      const detached = runFirstMinute(repo, symbol, env);
      const detachedIdentity = JSON.parse(
        fs.readFileSync(path.join(detached.payload.runtime.runtime_root, "agent-cache-identity.json"), "utf8")
      );
      assert.equal(detachedIdentity.source_revision.kind, "git");
      assert.equal(detachedIdentity.source_revision.branch, "(detached)");

      const unborn = path.join(fixture, "unborn");
      writeFixture(unborn, "unborn_identity_signal_93d2");
      git(unborn, ["init"]);
      const unbornRun = runFirstMinute(unborn, "unborn_identity_signal_93d2", env);
      const unbornIdentity = JSON.parse(
        fs.readFileSync(path.join(unbornRun.payload.runtime.runtime_root, "agent-cache-identity.json"), "utf8")
      );
      assert.equal(unbornIdentity.source_revision.kind, "git_unborn");
      assert.equal(typeof unbornIdentity.source_revision.branch, "string");

      const nonGit = path.join(fixture, "non-git");
      writeFixture(nonGit, "non_git_identity_signal_2b68");
      const nonGitRun = runFirstMinute(nonGit, "non_git_identity_signal_2b68", env);
      const nonGitIdentity = JSON.parse(
        fs.readFileSync(path.join(nonGitRun.payload.runtime.runtime_root, "agent-cache-identity.json"), "utf8")
      );
      assert.deepEqual(nonGitIdentity.source_revision, { kind: "non_git", head: null, branch: null });
    } finally {
      fs.rmSync(fixture, { recursive: true, force: true });
    }
  }
);
