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
