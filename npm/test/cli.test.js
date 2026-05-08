"use strict";

const assert = require("assert");
const path = require("path");
const { spawnSync } = require("child_process");

const {
  commandLooksLikeRuntime,
  defaultRuntimePath,
  mcpConfig,
  restart,
  runtimeBinaryName,
} = require("../lib/cli");

const cli = path.resolve(__dirname, "../bin/m1nd.js");

assert.strictEqual(runtimeBinaryName("win32"), "m1nd-mcp.exe");
assert.strictEqual(runtimeBinaryName("darwin"), "m1nd-mcp");
assert.strictEqual(runtimeBinaryName("linux"), "m1nd-mcp");
assert.strictEqual(commandLooksLikeRuntime("/Users/you/.m1nd/bin/m1nd-mcp --stdio"), true);
assert.strictEqual(commandLooksLikeRuntime("(m1nd-mcp)"), true);
assert.strictEqual(commandLooksLikeRuntime("node codex prompt mentions m1nd-mcp"), false);

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

const help = spawnSync(process.execPath, [cli, "--help"], { encoding: "utf8" });
assert.strictEqual(help.status, 0, help.stderr);
assert(help.stdout.includes("m1nd installer"));
assert(help.stdout.includes("m1nd smoke"));
assert(help.stdout.includes("m1nd restart"));

const packCheck = spawnSync(process.execPath, [cli, "pack-check", "--json"], { encoding: "utf8" });
assert.strictEqual(packCheck.status, 0, packCheck.stderr);
assert.strictEqual(JSON.parse(packCheck.stdout).schema, "m1nd-agent-pack-check-v0");

const restartPlan = restart({
  source: path.resolve(__dirname, "..", ".."),
  binary: path.resolve(__dirname, "missing-m1nd-mcp"),
  "no-build": true,
  "no-install": true,
  "no-kill": true,
});
assert.strictEqual(restartPlan.schema, "m1nd-npm-restart-v0");
assert.strictEqual(restartPlan.dry_run, true);
assert.strictEqual(restartPlan.actions.built, false);
assert.strictEqual(restartPlan.actions.installed, false);
assert(restartPlan.next_actions.some((action) => action.includes("Restart or rebind")));

console.log("npm cli tests ok");
