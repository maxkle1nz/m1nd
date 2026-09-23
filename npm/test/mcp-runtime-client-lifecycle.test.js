"use strict";

const assert = require("node:assert/strict");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const test = require("node:test");
const { McpRuntimeClient } = require("../lib/mcp-runtime-client");

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
