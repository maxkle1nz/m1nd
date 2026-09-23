"use strict";

const assert = require("node:assert/strict");
const crypto = require("node:crypto");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const { spawn } = require("node:child_process");
const test = require("node:test");
const { agentRuntimeCacheTarget } = require("../lib/agent-runtime-cache");

const CLI = path.resolve(__dirname, "../bin/m1nd.js");
const BINARY = process.env.M1ND_TEST_AGENT_CACHE_BINARY || "";
const EMBED_MODEL = process.env.M1ND_TEST_EMBED_MODEL || "";

function writeFixture(root, symbol) {
  fs.mkdirSync(path.join(root, "src"), { recursive: true });
  fs.writeFileSync(path.join(root, "package.json"), `${JSON.stringify({ name: "repo-alpha", version: "0.0.0" })}\n`);
  fs.writeFileSync(path.join(root, "src", "signal.js"), `export function ${symbol}() { return 42; }\n`);
}

function sourceSnapshot(root) {
  return ["package.json", path.join("src", "signal.js")].map((relative) => {
    const file = path.join(root, relative);
    return { relative, mode: fs.statSync(file).mode, bytes: fs.readFileSync(file).toString("base64") };
  });
}

function sha256(file) {
  return crypto.createHash("sha256").update(fs.readFileSync(file)).digest("hex");
}

function directorySnapshot(root) {
  const entries = [];
  function visit(dir) {
    for (const name of fs.readdirSync(dir).sort()) {
      const file = path.join(dir, name);
      const stat = fs.lstatSync(file);
      entries.push({
        path: path.relative(root, file),
        mode: stat.mode,
        bytes: stat.isDirectory() ? null : fs.readFileSync(file).toString("base64"),
      });
      if (stat.isDirectory()) visit(file);
    }
  }
  visit(root);
  return entries;
}

function readRoots(runtimeDir) {
  return JSON.parse(fs.readFileSync(path.join(runtimeDir, "ingest_roots.json"), "utf8"));
}

function isolatedEnv(fixture, additions = {}) {
  const env = {
    HOME: path.join(fixture, "home"),
    TMPDIR: path.join(fixture, "tmp"),
    TMP: path.join(fixture, "tmp"),
    TEMP: path.join(fixture, "tmp"),
    PATH: process.env.PATH || "/usr/bin:/bin",
    XDG_CACHE_HOME: path.join(fixture, "cache"),
    M1ND_AGENT_CACHE_DIR: path.join(fixture, "cache"),
    M1ND_REGISTRY_DIR: path.join(fixture, "registry"),
    M1ND_EMBED_MODEL: EMBED_MODEL,
    NPM_CONFIG_UPDATE_NOTIFIER: "false",
    NPM_CONFIG_OFFLINE: "true",
    ...additions,
  };
  for (const key of ["HOME", "TMPDIR", "XDG_CACHE_HOME", "M1ND_REGISTRY_DIR"]) {
    fs.mkdirSync(env[key], { recursive: true });
  }
  return env;
}

function writeHoldingBinary(wrapper, signals) {
  fs.writeFileSync(
    wrapper,
    `#!${process.execPath}\n` +
      `"use strict";\n` +
      `const fs = require("node:fs");\n` +
      `const path = require("node:path");\n` +
      `const readline = require("node:readline");\n` +
      `const { spawn } = require("node:child_process");\n` +
      `const signals = process.env.M1ND_TEST_CONCURRENCY_SIGNALS;\n` +
      `const release = path.join(signals, "release");\n` +
      `let holder = false;\n` +
      `try { fs.writeFileSync(path.join(signals, "holder-claimed"), String(process.pid), { flag: "wx" }); holder = true; } catch (error) { if (error.code !== "EEXIST") throw error; }\n` +
      `if (!holder && !fs.existsSync(release)) fs.writeFileSync(path.join(signals, "contender-launched-before-release"), String(process.pid));\n` +
      `const child = spawn(process.env.M1ND_TEST_REAL_BINARY, process.argv.slice(2), { cwd: process.cwd(), env: process.env, stdio: ["pipe", "pipe", "inherit"] });\n` +
      `process.stdin.pipe(child.stdin);\n` +
      `const lines = readline.createInterface({ input: child.stdout });\n` +
      `let held = null;\n` +
      `function flush() { if (held !== null) { process.stdout.write(held + "\\n"); held = null; } }\n` +
      `function awaitRelease() { if (fs.existsSync(release)) return flush(); const timer = setInterval(() => { if (fs.existsSync(release)) { clearInterval(timer); flush(); } }, 10); }\n` +
      `lines.on("line", (line) => { let payload = null; try { payload = JSON.parse(line); } catch (_) {} if (holder && held === null && payload && payload.id === 1) { held = line; fs.writeFileSync(path.join(signals, "holder-ready"), String(process.pid), { flag: "wx" }); awaitRelease(); } else process.stdout.write(line + "\\n"); });\n` +
      `child.on("exit", (code, signal) => { flush(); if (signal) process.kill(process.pid, signal); else process.exit(code === null ? 1 : code); });\n`,
    { mode: 0o700 }
  );
}

function writeManifestBarrier(preload) {
  fs.writeFileSync(
    preload,
    `"use strict";\n` +
      `const fs = require("node:fs");\n` +
      `const originalOpenSync = fs.openSync;\n` +
      `const originalWriteFileSync = fs.writeFileSync;\n` +
      `function isExclusiveCreate(flags) { return typeof flags === "number" && (flags & fs.constants.O_CREAT) !== 0 && (flags & fs.constants.O_EXCL) !== 0; }\n` +
      `fs.openSync = function(file, flags, mode) {\n` +
      `  if (!(typeof file === "string" && file.startsWith(process.env.M1ND_TEST_CACHE + require("node:path").sep) && file.endsWith(require("node:path").sep + "agent-cache-identity.json") && isExclusiveCreate(flags))) return originalOpenSync.apply(this, arguments);\n` +
      `  const fd = originalOpenSync.apply(this, arguments);\n` +
      `    try {\n` +
      `      const marker = process.env.M1ND_TEST_MANIFEST_OPENED;\n` +
      `      originalWriteFileSync(marker + ".pending", file, { flag: "wx" });\n` +
      `      fs.renameSync(marker + ".pending", marker);\n` +
      `      const deadline = Date.now() + 30000;\n` +
      `      while (!fs.existsSync(process.env.M1ND_TEST_MANIFEST_RELEASE)) {\n` +
      `        if (Date.now() > deadline) throw new Error("fixture manifest release deadline");\n` +
      `        Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, 10);\n` +
      `      }\n` +
      `      return fd;\n` +
      `    } catch (error) { fs.closeSync(fd); throw error; }\n` +
      `};\n`,
    { mode: 0o600 }
  );
}

function spawnFirstMinute(repo, symbol, binary, env) {
  const child = spawn(
    process.execPath,
    [CLI, "agent", "first-minute", "--repo", repo, "--binary", binary, "--query", symbol, "--no-attach", "--json"],
    { cwd: repo, env, stdio: ["ignore", "pipe", "pipe"] }
  );
  let stdout = "";
  let stderr = "";
  child.stdout.on("data", (chunk) => { stdout += chunk; });
  child.stderr.on("data", (chunk) => { stderr += chunk; });
  const completed = new Promise((resolve) => child.once("exit", (code, signal) => resolve({ code, signal, stdout, stderr })));
  return { child, completed, output: () => ({ stdout, stderr }) };
}

async function waitForHolderReady(signals, holder) {
  try {
    const outcome = await Promise.race([
      waitForSignal(signals, ["holder-ready"], 60_000).then((signal) => ({ signal })),
      holder.completed.then((completed) => ({ completed })),
    ]);
    assert.equal(outcome.signal, "holder-ready", `holder exited before initialization: ${JSON.stringify(outcome.completed)}`);
  } catch (error) {
    throw new Error(
      `${error.message}; holder-claimed=${fs.existsSync(path.join(signals, "holder-claimed"))}; ` +
        `child exit=${holder.child.exitCode} signal=${holder.child.signalCode}; ` +
        `output=${JSON.stringify(holder.output())}`,
      { cause: error }
    );
  }
}

function waitForSignal(dir, names, timeoutMs = 15_000) {
  const present = () => names.find((name) => fs.existsSync(path.join(dir, name)));
  const immediate = present();
  if (immediate) return Promise.resolve(immediate);
  return new Promise((resolve, reject) => {
    const interval = setInterval(() => {
      const name = present();
      if (!name) return;
      clearTimeout(timer);
      clearInterval(interval);
      resolve(name);
    }, 10);
    const timer = setTimeout(() => {
      clearInterval(interval);
      reject(new Error(`timed out waiting for signal: ${names.join(", ")}`));
    }, timeoutMs);
  });
}

function waitForContenderArrival(signals, cache, timeoutMs = 15_000) {
  const present = () => {
    if (fs.existsSync(path.join(signals, "contender-launched-before-release"))) return "runtime-contender";
    if (!fs.existsSync(cache)) return null;
    for (const runtime of fs.readdirSync(cache)) {
      const runtimeDir = path.join(cache, runtime);
      if (!fs.statSync(runtimeDir).isDirectory()) continue;
      if (fs.readdirSync(runtimeDir).some((name) => name.startsWith(".agent-cache-waiter-v1-"))) {
        return "cache-waiter-ready";
      }
    }
    return null;
  };
  const immediate = present();
  if (immediate) return Promise.resolve(immediate);
  return new Promise((resolve, reject) => {
    const interval = setInterval(() => {
      const value = present();
      if (!value) return;
      clearTimeout(timer);
      clearInterval(interval);
      resolve(value);
    }, 10);
    const timer = setTimeout(() => {
      clearInterval(interval);
      reject(new Error("timed out waiting for the second public invocation to reach cache ownership"));
    }, timeoutMs);
  });
}

function parseSuccess(run, symbol) {
  assert.equal(run.signal, null, run.stderr);
  assert.equal(run.code, 0, `first-minute failed (${run.code}): ${run.stderr}\n${run.stdout}`);
  const payload = JSON.parse(run.stdout);
  assert.equal(payload.ok, true, JSON.stringify(payload));
  const matches = payload.results.flatMap((result) =>
    Array.isArray(result.results) ? result.results : []
  );
  const rendered = JSON.stringify(matches);
  assert.match(rendered, new RegExp(symbol));
  assert.match(rendered, /src[/\\]signal\.js/);
  return payload;
}

async function runPublic(repo, symbol, binary, env) {
  const invocation = spawnFirstMinute(repo, symbol, binary, env);
  return parseSuccess(await invocation.completed, symbol);
}

async function settleOwnedChildren(children) {
  const owned = [...children].map((entry) => {
    const child = entry.child || entry;
    const completed = entry.completed || (
      child.exitCode !== null || child.signalCode !== null
        ? Promise.resolve()
        : new Promise((resolve) => child.once("exit", resolve))
    );
    return { child, completed };
  });
  for (const entry of owned) {
    if (entry.child.exitCode === null && entry.child.signalCode === null) entry.child.kill("SIGTERM");
  }
  await Promise.allSettled(owned.map((entry) => entry.completed));
}

test(
  "cold cache identity publication is covered by the owner lease",
  { skip: !BINARY || !EMBED_MODEL, timeout: 120_000 },
  async () => {
    const fixture = fs.mkdtempSync(path.join(os.tmpdir(), "m1nd-agent-cache-cold-creation-"));
    const children = new Set();
    const release = path.join(fixture, "release-manifest");
    try {
      const repo = path.join(fixture, "repo-alpha");
      const opened = path.join(fixture, "manifest-opened");
      const preload = path.join(fixture, "hold-manifest.cjs");
      const symbol = "cold_cache_race_signal_71ab";
      writeFixture(repo, symbol);
      const sourceBefore = sourceSnapshot(repo);
      writeManifestBarrier(preload);
      const env = isolatedEnv(fixture);
      const firstEnv = {
        ...env,
        NODE_OPTIONS: `--require=${preload}`,
        M1ND_TEST_CACHE: fs.realpathSync(env.M1ND_AGENT_CACHE_DIR),
        M1ND_TEST_MANIFEST_OPENED: opened,
        M1ND_TEST_MANIFEST_RELEASE: release,
      };

      const first = spawnFirstMinute(repo, symbol, BINARY, firstEnv);
      children.add(first);
      await waitForSignal(fixture, ["manifest-opened"]);
      const manifest = fs.readFileSync(opened, "utf8");
      assert.equal(fs.statSync(manifest).size, 0, "fixture did not hold the real manifest after exclusive open");

      const second = spawnFirstMinute(repo, symbol, BINARY, env);
      children.add(second);
      const arrival = await Promise.race([
        waitForContenderArrival(fixture, env.M1ND_AGENT_CACHE_DIR),
        second.completed.then((run) => ({ completed: run })),
      ]);
      assert.equal(arrival, "cache-waiter-ready", `second invocation escaped ownership wait: ${JSON.stringify(arrival)}`);
      assert.equal(second.child.exitCode, null, "second invocation stopped before the holder released publication");

      fs.writeFileSync(release, "release\n", { flag: "wx" });
      const [firstRun, secondRun] = await Promise.all([first.completed, second.completed]);
      const firstPayload = parseSuccess(firstRun, symbol);
      const secondPayload = parseSuccess(secondRun, symbol);
      assert.equal(secondPayload.runtime.runtime_root, firstPayload.runtime.runtime_root);
      assert.deepEqual(readRoots(firstPayload.runtime.runtime_root), [fs.realpathSync(repo)]);
      assert.deepEqual(sourceSnapshot(repo), sourceBefore);
      assert.deepEqual(
        JSON.parse(fs.readFileSync(path.join(firstPayload.runtime.runtime_root, "agent-cache-identity.json"), "utf8")),
        JSON.parse(fs.readFileSync(manifest, "utf8"))
      );
    } finally {
      if (!fs.existsSync(release)) fs.writeFileSync(release, "release\n");
      await settleOwnedChildren(children);
      fs.rmSync(fixture, { recursive: true, force: true });
    }
  }
);

test(
  "cold cache coordination refuses populated, invalid, and foreign identity state without repair",
  { skip: !BINARY || !EMBED_MODEL, timeout: 60_000 },
  async () => {
    const fixture = fs.mkdtempSync(path.join(os.tmpdir(), "m1nd-agent-cache-cold-negatives-"));
    const children = new Set();
    try {
      for (const kind of ["populated", "invalid", "foreign"]) {
        const base = path.join(fixture, kind);
        const repo = path.join(base, "repo-alpha");
        const symbol = `cold_negative_${kind}_signal`;
        writeFixture(repo, symbol);
        const env = isolatedEnv(base);
        const target = agentRuntimeCacheTarget(repo, env);
        fs.mkdirSync(target.runtimeDir, { recursive: true });
        if (kind === "populated") {
          fs.writeFileSync(path.join(target.runtimeDir, "derived-state.bin"), "foreign-state\n");
        } else if (kind === "invalid") {
          fs.writeFileSync(path.join(target.runtimeDir, "agent-cache-identity.json"), "{\n");
        } else {
          fs.writeFileSync(
            path.join(target.runtimeDir, "agent-cache-identity.json"),
            `${JSON.stringify({ ...target.identity, canonical_root: path.join(base, "foreign-root") }, null, 2)}\n`
          );
        }
        const before = directorySnapshot(target.runtimeDir);
        const invocation = spawnFirstMinute(repo, symbol, BINARY, env);
        children.add(invocation);
        const run = await invocation.completed;
        assert.equal(run.signal, null, run.stderr);
        assert.notEqual(run.code, 0, `${kind} cache state was adopted`);
        if (kind === "populated") assert.match(run.stderr, /derived state but no identity/i);
        else if (kind === "invalid") assert.match(run.stderr, /identity is unreadable/i);
        else assert.match(run.stderr, /identity mismatch/i);
        assert.deepEqual(directorySnapshot(target.runtimeDir), before, `${kind} cache state was repaired or replaced`);
      }
    } finally {
      await settleOwnedChildren(children);
      fs.rmSync(fixture, { recursive: true, force: true });
    }
  }
);

test(
  "simultaneous public first-minute calls serialize one cache owner and both return real matches",
  { skip: !BINARY || !EMBED_MODEL, timeout: 180_000 },
  async () => {
    const fixture = fs.mkdtempSync(path.join(os.tmpdir(), "m1nd-agent-cache-concurrency-"));
    const children = new Set();
    try {
      const repo = path.join(fixture, "repo-alpha");
      const signals = path.join(fixture, "signals");
      const wrapper = path.join(fixture, "holding-m1nd-mcp.js");
      const symbol = "concurrent_cache_signal_94a1";
      fs.mkdirSync(signals);
      writeFixture(repo, symbol);
      const sourceBefore = sourceSnapshot(repo);
      writeHoldingBinary(wrapper, signals);
      const env = isolatedEnv(fixture, {
        M1ND_TEST_CONCURRENCY_SIGNALS: signals,
        M1ND_TEST_REAL_BINARY: BINARY,
      });

      const first = spawnFirstMinute(repo, symbol, wrapper, env);
      children.add(first.child);
      await waitForHolderReady(signals, first);

      const second = spawnFirstMinute(repo, symbol, wrapper, env);
      children.add(second.child);
      const arrival = await waitForContenderArrival(signals, env.M1ND_AGENT_CACHE_DIR);
      fs.writeFileSync(path.join(signals, "release"), "release\n", { flag: "wx" });

      const [firstRun, secondRun] = await Promise.all([first.completed, second.completed]);
      const firstPayload = parseSuccess(firstRun, symbol);
      const secondPayload = parseSuccess(secondRun, symbol);
      assert.equal(arrival, "cache-waiter-ready", "the second invocation launched a competing runtime owner");
      assert.equal(secondPayload.runtime.runtime_root, firstPayload.runtime.runtime_root);
      assert.equal(fs.existsSync(path.join(signals, "contender-launched-before-release")), false);

      const runtimeDir = firstPayload.runtime.runtime_root;
      const snapshot = path.join(runtimeDir, "graph_snapshot.json");
      assert.deepEqual(readRoots(runtimeDir), [fs.realpathSync(repo)]);
      assert.ok(fs.statSync(snapshot).size > 2, "first cache creation persisted an empty snapshot");
      const snapshotHash = sha256(snapshot);

      const restarted = await runPublic(repo, symbol, wrapper, env);
      assert.equal(restarted.runtime.runtime_root, runtimeDir, "warm restart did not reuse the first cache");
      assert.equal(sha256(snapshot), snapshotHash, "warm restart rewrote the reusable snapshot");
      assert.deepEqual(readRoots(runtimeDir), [fs.realpathSync(repo)]);
      assert.deepEqual(sourceSnapshot(repo), sourceBefore);
    } finally {
      await settleOwnedChildren(children);
      fs.rmSync(fixture, { recursive: true, force: true });
    }
  }
);

test(
  "distinct workspaces do not share the cache owner lease",
  { skip: !BINARY || !EMBED_MODEL, timeout: 180_000 },
  async () => {
    const fixture = fs.mkdtempSync(path.join(os.tmpdir(), "m1nd-agent-cache-distinct-"));
    const children = new Set();
    try {
      const repoAlpha = path.join(fixture, "repo-alpha");
      const repoBeta = path.join(fixture, "repo-beta");
      const signals = path.join(fixture, "signals");
      const wrapper = path.join(fixture, "holding-m1nd-mcp.js");
      const alphaSymbol = "distinct_alpha_signal_a817";
      const betaSymbol = "distinct_beta_signal_b418";
      fs.mkdirSync(signals);
      writeFixture(repoAlpha, alphaSymbol);
      writeFixture(repoBeta, betaSymbol);
      writeHoldingBinary(wrapper, signals);
      const env = isolatedEnv(fixture, {
        M1ND_TEST_CONCURRENCY_SIGNALS: signals,
        M1ND_TEST_REAL_BINARY: BINARY,
      });

      const alpha = spawnFirstMinute(repoAlpha, alphaSymbol, wrapper, env);
      children.add(alpha.child);
      await waitForHolderReady(signals, alpha);

      const beta = spawnFirstMinute(repoBeta, betaSymbol, wrapper, env);
      children.add(beta.child);
      await waitForSignal(signals, ["contender-launched-before-release"]);
      const betaPayload = parseSuccess(await beta.completed, betaSymbol);
      assert.equal(alpha.child.exitCode, null, "the first workspace stopped before its explicit release");

      fs.writeFileSync(path.join(signals, "release"), "release\n", { flag: "wx" });
      const alphaPayload = parseSuccess(await alpha.completed, alphaSymbol);
      assert.notEqual(alphaPayload.runtime.runtime_root, betaPayload.runtime.runtime_root);
      assert.deepEqual(readRoots(alphaPayload.runtime.runtime_root), [fs.realpathSync(repoAlpha)]);
      assert.deepEqual(readRoots(betaPayload.runtime.runtime_root), [fs.realpathSync(repoBeta)]);
    } finally {
      await settleOwnedChildren(children);
      fs.rmSync(fixture, { recursive: true, force: true });
    }
  }
);

test(
  "a bounded loser preserves another owner and interruption cannot terminate the winner",
  { skip: !BINARY || !EMBED_MODEL, timeout: 180_000 },
  async () => {
    const fixture = fs.mkdtempSync(path.join(os.tmpdir(), "m1nd-agent-cache-busy-"));
    const children = new Set();
    try {
      const repo = path.join(fixture, "repo-alpha");
      const signals = path.join(fixture, "signals");
      const wrapper = path.join(fixture, "holding-m1nd-mcp.js");
      const symbol = "bounded_busy_signal_c519";
      fs.mkdirSync(signals);
      writeFixture(repo, symbol);
      writeHoldingBinary(wrapper, signals);
      const env = isolatedEnv(fixture, {
        M1ND_TEST_CONCURRENCY_SIGNALS: signals,
        M1ND_TEST_REAL_BINARY: BINARY,
        M1ND_AGENT_CACHE_OWNER_WAIT_MS: "1000",
      });

      const winner = spawnFirstMinute(repo, symbol, wrapper, env);
      children.add(winner.child);
      await waitForHolderReady(signals, winner);
      const runtimeDir = path.join(env.M1ND_AGENT_CACHE_DIR, fs.readdirSync(env.M1ND_AGENT_CACHE_DIR)[0]);
      const ownerManifest = path.join(runtimeDir, ".agent-cache-owner-v1", "owner.json");
      const ownerBefore = sha256(ownerManifest);

      const boundedLoser = spawnFirstMinute(repo, symbol, wrapper, env);
      children.add(boundedLoser.child);
      const refused = await boundedLoser.completed;
      assert.equal(refused.signal, null, refused.stderr);
      assert.notEqual(refused.code, 0, "a live owner was silently appropriated");
      assert.match(refused.stderr, /cache is busy.*lock and state were preserved/i);
      assert.equal(sha256(ownerManifest), ownerBefore, "the refused contender modified the owner's proof");
      assert.equal(winner.child.exitCode, null, "the refused contender terminated the winner");

      const interruptedLoser = spawnFirstMinute(repo, symbol, wrapper, env);
      children.add(interruptedLoser.child);
      await waitForContenderArrival(signals, env.M1ND_AGENT_CACHE_DIR);
      interruptedLoser.child.kill("SIGTERM");
      const interrupted = await interruptedLoser.completed;
      assert.equal(interrupted.signal, "SIGTERM");
      assert.equal(winner.child.exitCode, null, "interrupting the loser terminated the winner");
      assert.equal(sha256(ownerManifest), ownerBefore, "interrupting the loser modified the owner's proof");

      fs.writeFileSync(path.join(signals, "release"), "release\n", { flag: "wx" });
      parseSuccess(await winner.completed, symbol);
      assert.equal(fs.existsSync(path.dirname(ownerManifest)), false, "the proven owner did not release its lease");
    } finally {
      await settleOwnedChildren(children);
      fs.rmSync(fixture, { recursive: true, force: true });
    }
  }
);
