"use strict";

const assert = require("assert");

const {
  defaultRuntimePath,
  mcpConfig,
  runtimeBinaryName,
} = require("../lib/cli");

assert.strictEqual(runtimeBinaryName("win32"), "m1nd-mcp.exe");
assert.strictEqual(runtimeBinaryName("darwin"), "m1nd-mcp");
assert.strictEqual(runtimeBinaryName("linux"), "m1nd-mcp");

assert.strictEqual(
  defaultRuntimePath("win32", "C:\\Users\\you"),
  "C:\\Users\\you\\.m1nd\\bin\\m1nd-mcp.exe"
);

const codexWindowsConfig = mcpConfig(
  "codex",
  "C:\\Users\\you\\.m1nd\\bin\\m1nd-mcp.exe"
);
assert(codexWindowsConfig.includes('command = "C:\\\\Users\\\\you\\\\.m1nd\\\\bin\\\\m1nd-mcp.exe"'));
assert(codexWindowsConfig.includes('args = ["--stdio", "--no-gui"]'));

const genericWindowsConfig = JSON.parse(
  mcpConfig("generic", "C:\\Users\\you\\.m1nd\\bin\\m1nd-mcp.exe")
);
assert.strictEqual(
  genericWindowsConfig.mcpServers.m1nd.command,
  "C:\\Users\\you\\.m1nd\\bin\\m1nd-mcp.exe"
);
assert.deepStrictEqual(genericWindowsConfig.mcpServers.m1nd.args, ["--stdio", "--no-gui"]);

console.log("npm cli tests ok");
