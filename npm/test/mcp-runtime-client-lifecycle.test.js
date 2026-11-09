"use strict";

const assert = require("node:assert/strict");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const test = require("node:test");
const { McpRuntimeClient } = require("../lib/mcp-runtime-client");

test("forbidAttach rejects conflicting transport selectors before config writes", () => {
  const fixture = fs.mkdtempSync(path.join(os.tmpdir(), "m1nd-forbid-attach-"));
  try {
    const cases = [
      { args: ["--stdio"], attach: "auto" },
      { args: ["--stdio", "--attach", "auto"] },
      { args: ["--stdio", "--attach=https://example.invalid/mcp"] },
      { args: ["--attach", "auto", "--attach=https://example.invalid/mcp"] },
      { args: ["--stdio", "--attach="] },
    ];
    for (const [index, options] of cases.entries()) {
      const runtimeDir = path.join(fixture, `state-${index}`);
      assert.throws(() => new McpRuntimeClient({
        binary: process.execPath, repo: fixture, runtimeDir, forbidAttach: true, ...options,
      }), (error) => {
        assert.equal(error.code, "M1ND_ATTACH_FORBIDDEN");
        assert.match(error.message, /--no-attach/);
        assert.ok(!error.message.includes("example.invalid"), "errors must not echo an owner URL");
        return true;
      });
      assert.equal(fs.existsSync(runtimeDir), false);
    }
  } finally { fs.rmSync(fixture, { recursive: true, force: true }); }
});

test("forbidAttach also checks captured ambient arguments and later mutation", async () => {
  const fixture = fs.mkdtempSync(path.join(os.tmpdir(), "m1nd-forbid-ambient-"));
  const previous = process.env.M1ND_MCP_ARGS;
  try {
    process.env.M1ND_MCP_ARGS = "--stdio --attach=auto";
    assert.throws(() => new McpRuntimeClient({ binary: process.execPath, repo: fixture, forbidAttach: true }),
      (error) => error.code === "M1ND_ATTACH_FORBIDDEN");
    const runtimeDir = path.join(fixture, "private-state");
    const client = new McpRuntimeClient({ binary: process.execPath, repo: fixture,
      runtimeDir, args: ["--stdio"], forbidAttach: true });
    client.args.push("--attach", "auto");
    assert.throws(() => client.launchConfig(), (error) => error.code === "M1ND_ATTACH_FORBIDDEN");
    await assert.rejects(client.start(), (error) => error.code === "M1ND_ATTACH_FORBIDDEN");
    assert.equal(client.proc, null);
    assert.equal(client.spawned, false);
    assert.equal(fs.existsSync(runtimeDir), false);
    const propertyClient = new McpRuntimeClient({ binary: process.execPath, repo: fixture,
      runtimeDir, args: ["--stdio"], forbidAttach: true });
    propertyClient.attach = "auto";
    assert.throws(() => propertyClient.launchConfig(), (error) => error.code === "M1ND_ATTACH_FORBIDDEN");
    assert.equal(fs.existsSync(runtimeDir), false);
  } finally {
    if (previous === undefined) delete process.env.M1ND_MCP_ARGS;
    else process.env.M1ND_MCP_ARGS = previous;
    fs.rmSync(fixture, { recursive: true, force: true });
  }
});

test("forbidAttach leaves allowed transport configuration and environment intact", () => {
  const fixture = fs.mkdtempSync(path.join(os.tmpdir(), "m1nd-attach-compatible-"));
  try {
    const runtimeDir = path.join(fixture, "private-state");
    const isolated = new McpRuntimeClient({ binary: "fixture", repo: fixture, runtimeDir,
      args: ["--stdio"], forbidAttach: true, env: { M1ND_ATTACH_URL: "https://example.invalid/mcp" } });
    const config = isolated.launchConfig();
    assert.equal(config.env.M1ND_ATTACH_URL, "https://example.invalid/mcp");
    assert.ok(!config.args.some((arg) => arg === "--attach" || arg.startsWith("--attach=")));
    assert.equal(config.runtimeDir, runtimeDir);
    const attached = new McpRuntimeClient({ binary: "fixture", repo: fixture,
      args: ["--stdio"], attach: "auto" }).launchConfig();
    assert.ok(attached.args.includes("--attach"));
    assert.equal(attached.runtimeDir, null);
    const legacyArguments = new McpRuntimeClient({ binary: "fixture", repo: fixture,
      args: ["--stdio", "--attach=auto"], runtimeDir: path.join(fixture, "legacy-state") }).launchConfig();
    assert.ok(legacyArguments.args.includes("--attach=auto"), "non-opt-in callers retain existing argument behavior");
  } finally { fs.rmSync(fixture, { recursive: true, force: true }); }
});

test("cold initialize has a longer bounded deadline than ordinary requests", async () => {
  const client = new McpRuntimeClient({ binary: "fixture", repo: "repo-alpha" });
  client.proc = { stdin: { destroyed: false, write() {} } };
  const deadlines = [];
  const originalSetTimeout = global.setTimeout;
  global.setTimeout = (callback, delay, ...args) => {
    deadlines.push(delay);
    return originalSetTimeout(callback, delay, ...args);
  };
  try {
    const initialize = client.request("initialize", {});
    client.handleLine(JSON.stringify({ id: 1, result: {} }));
    await initialize;
    const tools = client.request("tools/list", {});
    client.handleLine(JSON.stringify({ id: 2, result: {} }));
    await tools;
  } finally {
    global.setTimeout = originalSetTimeout;
  }
  assert.ok(deadlines[0] >= 90_000 && deadlines[0] <= 180_000,
    `initialize must allow cold startup but remain bounded, got ${deadlines[0]}`);
  assert.equal(deadlines[1], 30_000, "ordinary requests must keep their existing bound");
});

test("slow cooperative checkpoint finishes without SIGKILL", {
  skip: process.platform === "win32" && "POSIX shebang fixture cannot be spawned by Windows CreateProcess",
  timeout: 20_000,
}, async () => {
  const fixture = fs.mkdtempSync(path.join(os.tmpdir(), "m1nd-slow-checkpoint-"));
  const repo = path.join(fixture, "repo-alpha");
  const binary = path.join(fixture, "owner.js");
  const checkpoint = path.join(fixture, "checkpoint-ack");
  fs.mkdirSync(repo);
  fs.writeFileSync(binary, `#!${process.execPath}\n` +
    `"use strict";\n` +
    `const readline = require("node:readline");\n` +
    `const fs = require("node:fs");\n` +
    `const rl = readline.createInterface({ input: process.stdin });\n` +
    `rl.on("line", line => { const req = JSON.parse(line); process.stdout.write(JSON.stringify({jsonrpc:"2.0",id:req.id,result:{}}) + "\\n"); });\n` +
    `process.on("SIGTERM", () => setTimeout(() => { fs.writeFileSync(${JSON.stringify(checkpoint)}, "ACK\\n"); process.exit(0); }, 5300));\n` +
    `process.stdin.on("end", () => { setInterval(() => {}, 1000); });\n`, { mode: 0o700 });
  const client = new McpRuntimeClient({ binary, repo, cwd: repo });
  try {
    await client.start();
    await client.closeAndWait();
    assert.equal(client.processClosed, true, "child streams must actually close");
    assert.equal(client.closeStatus.code, 0);
    assert.equal(client.closeStatus.signal, null);
    assert.equal(fs.readFileSync(checkpoint, "utf8"), "ACK\n");
  } finally {
    if (client.proc && !client.processClosed) {
      client.proc.kill("SIGKILL"); // Test-owned child only, after the assertion.
      await client.waitForClose(3_000);
    }
    fs.rmSync(fixture, { recursive: true, force: true });
  }
});

test("unconfirmed close fails bounded without killing the writer", async () => {
  const client = new McpRuntimeClient({ binary: "fixture", repo: "repo-alpha" });
  const signals = [];
  client.proc = {
    stdin: { destroyed: false, end() {} },
    stdout: { unref() {} }, stderr: { unref() {} },
    exitCode: null, signalCode: null,
    kill(signal) { signals.push(signal); return true; },
    unref() {},
  };
  client.waitForClose = async () => false;
  await assert.rejects(client.closeAndWait(), /child streams did not close/);
  assert.deepEqual(signals, ["SIGTERM"], "a timeout must not SIGKILL an unconfirmed owner");
  assert.equal(client.processClosed, false, "lease release still requires the close event");
});

test("interruption rejects pending requests and prevents future runtime work", async () => {
  const interrupted = Object.assign(new Error("m1nd agent interrupted by SIGTERM"), {
    code: "M1ND_AGENT_INTERRUPTED",
    signal: "SIGTERM",
    exitCode: 143,
  });
  const cancellation = { error: null };
  const signals = [];
  const client = new McpRuntimeClient({ binary: "fixture", repo: "repo-alpha", cancellation });
  client.proc = {
    stdin: { destroyed: false, write() {} },
    exitCode: null,
    signalCode: null,
    kill(signal) { signals.push(signal); return true; },
  };

  const pending = client.request("initialize", {});
  cancellation.error = interrupted;
  client.cancel(interrupted, "SIGTERM");

  await assert.rejects(pending, (error) => error === interrupted);
  await assert.rejects(client.request("tools/list", {}), (error) => error === interrupted);
  await assert.rejects(client.start(), (error) => error === interrupted);
  assert.deepEqual(signals, ["SIGTERM"]);
  assert.equal(client.processClosed, false, "forwarding a signal is not evidence that the child closed");
});
