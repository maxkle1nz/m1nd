"use strict";

const fs = require("fs");
const os = require("os");
const path = require("path");
const readline = require("readline");
const { spawn, spawnSync } = require("child_process");

const OWNER_DISCOVERY_SCHEMA = "m1nd-owner-discovery-v0";

function defaultRuntimeArgs() {
  return (process.env.M1ND_MCP_ARGS || "--stdio --no-gui").trim().split(/\s+/).filter(Boolean);
}

function argsHaveOption(args, option) {
  return args.some((arg) => arg === option || arg.startsWith(`${option}=`));
}

function ensureDir(dir) {
  fs.mkdirSync(dir, { recursive: true });
}

/// Ask the runtime whether a live serve owner already holds this repo.
///
/// The question is NOT re-implemented here: `m1nd-mcp --discover-owner` is a
/// one-shot, read-only projection of the same `discover_serve_owner` the
/// `--attach auto` bridge uses (its two questions: an owner for this client's
/// runtime root, else an owner whose declared ingest roots cover this repo).
/// This wrapper only decides what to do with the answer.
///
/// Never throws, and never a crash surface: a runtime that predates the flag
/// refuses it like any unknown argument, which is reported honestly as
/// `supported: false` so the historical isolated boot stays available.
function discoverServeOwner(options) {
  const binary = options.binary;
  const repo = options.repo;
  if (!binary || !fs.existsSync(binary)) {
    return {
      schema: OWNER_DISCOVERY_SCHEMA,
      supported: false,
      found: false,
      reason: `no m1nd-mcp runtime at ${binary || "unknown"}; owner discovery was not asked`,
    };
  }
  let probe;
  try {
    probe = spawnSync(binary, ["--discover-owner"], {
      cwd: repo,
      input: "",
      encoding: "utf8",
      timeout: 15000,
      env: { ...process.env, ...(options.env || {}), M1ND_WORKSPACE_ROOT: repo },
    });
  } catch (error) {
    return {
      schema: OWNER_DISCOVERY_SCHEMA,
      supported: false,
      found: false,
      reason: `owner discovery probe failed: ${error.message || String(error)}`,
    };
  }
  let payload = null;
  try {
    payload = JSON.parse(String(probe.stdout || ""));
  } catch (_) {
    payload = null;
  }
  if (!payload || payload.schema !== OWNER_DISCOVERY_SCHEMA) {
    return {
      schema: OWNER_DISCOVERY_SCHEMA,
      supported: false,
      found: false,
      reason:
        "this m1nd-mcp runtime does not answer --discover-owner (older build), so no live serve owner could be looked up",
    };
  }
  return { ...payload, supported: true, found: Boolean(payload.found) };
}

class McpRuntimeClient {
  constructor(options) {
    this.binary = options.binary;
    this.repo = options.repo;
    this.sharedRuntime = Boolean(options.sharedRuntime);
    this.runtimeDir = options.runtimeDir || null;
    // `auto` (or an explicit owner URL) turns this client into the thin
    // stdio↔HTTP bridge instead of a private runtime: no isolated runtime dir
    // is minted, no graph is loaded, no lease is taken.
    this.attach = options.attach || null;
    this.extraEnv = options.env || {};
    this.cwd = options.cwd || null;
    this.args = options.args || defaultRuntimeArgs();
    this.proc = null;
    this.readline = null;
    this.pending = new Map();
    this.nextId = 1;
    this.stderr = "";
  }

  launchConfig() {
    if (this.attach) {
      const args = [...this.args];
      if (!argsHaveOption(args, "--attach")) args.push("--attach", this.attach);
      return {
        args,
        cwd: this.cwd || this.repo,
        env: { ...process.env, ...this.extraEnv, M1ND_WORKSPACE_ROOT: this.repo },
        runtimeDir: null,
      };
    }
    const args = [...this.args];
    let runtimeDir = this.runtimeDir;
    if (!this.sharedRuntime && !runtimeDir) {
      runtimeDir = fs.mkdtempSync(path.join(os.tmpdir(), "m1nd-agent-"));
    }
    if (runtimeDir) {
      ensureDir(runtimeDir);
      if (!argsHaveOption(args, "--runtime-dir")) {
        args.push("--runtime-dir", runtimeDir);
      }
    }
    const env = {
      ...process.env,
      ...this.extraEnv,
      M1ND_WORKSPACE_ROOT: this.repo,
    };
    let cwd = this.cwd || this.repo;
    if (!this.sharedRuntime && runtimeDir) {
      cwd = runtimeDir;
      if (!env.M1ND_RUNTIME_BASE) env.M1ND_RUNTIME_BASE = runtimeDir;
      if (!env.M1ND_GRAPH_SOURCE && !argsHaveOption(args, "--graph")) {
        env.M1ND_GRAPH_SOURCE = path.join(runtimeDir, "graph_snapshot.json");
      }
      if (!env.M1ND_PLASTICITY_STATE && !argsHaveOption(args, "--plasticity")) {
        env.M1ND_PLASTICITY_STATE = path.join(runtimeDir, "plasticity_state.json");
      }
    }
    return { args, cwd, env, runtimeDir };
  }

  async start() {
    if (!this.binary || !fs.existsSync(this.binary)) {
      throw new Error(`m1nd-mcp runtime not found at ${this.binary || "unknown"}`);
    }
    const config = this.launchConfig();
    this.runtimeDir = config.runtimeDir;
    this.proc = spawn(this.binary, config.args, {
      cwd: config.cwd,
      env: config.env,
      stdio: ["pipe", "pipe", "pipe"],
    });
    this.proc.stderr.on("data", (chunk) => {
      this.stderr += chunk.toString();
      if (this.stderr.length > 12000) this.stderr = this.stderr.slice(-12000);
    });
    this.proc.on("exit", () => {
      for (const pending of this.pending.values()) {
        pending.reject(new Error(`m1nd-mcp process exited; stderr=${this.stderr.trim()}`));
      }
      this.pending.clear();
    });
    this.readline = readline.createInterface({ input: this.proc.stdout });
    this.readline.on("line", (line) => this.handleLine(line));
    await this.request("initialize", {});
    return this;
  }

  handleLine(line) {
    let payload;
    try {
      payload = JSON.parse(line);
    } catch (error) {
      const first = this.pending.values().next().value;
      if (first) first.reject(new Error(`invalid MCP JSON response: ${error.message}`));
      return;
    }
    const pending = this.pending.get(payload.id);
    if (!pending) return;
    this.pending.delete(payload.id);
    if (payload.error) {
      pending.reject(new Error(JSON.stringify(payload.error)));
    } else {
      pending.resolve(payload);
    }
  }

  request(method, params) {
    if (!this.proc || !this.proc.stdin || this.proc.stdin.destroyed) {
      return Promise.reject(new Error("m1nd-mcp process is not running"));
    }
    const id = this.nextId;
    this.nextId += 1;
    const payload = { jsonrpc: "2.0", id, method, params };
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        this.pending.delete(id);
        reject(new Error(`MCP request timed out for ${method}; stderr=${this.stderr.trim()}`));
      }, 30000);
      this.pending.set(id, {
        resolve: (value) => {
          clearTimeout(timer);
          resolve(value);
        },
        reject: (error) => {
          clearTimeout(timer);
          reject(error);
        },
      });
      this.proc.stdin.write(`${JSON.stringify(payload)}\n`);
    });
  }

  async tools() {
    return this.request("tools/list", {});
  }

  async callTool(name, args) {
    return this.request("tools/call", { name, arguments: args });
  }

  close() {
    if (this.readline) this.readline.close();
    if (this.proc && this.proc.exitCode === null && this.proc.signalCode === null) {
      this.proc.kill("SIGTERM");
    }
  }
}

function parseEmbeddedJson(text) {
  try {
    return JSON.parse(text);
  } catch (_) {
    return text;
  }
}

function decodeToolResponse(response) {
  const result = response.result || {};
  const content = Array.isArray(result.content) ? result.content : [];
  if (content.length > 0 && content[0].type === "text") {
    return {
      isError: Boolean(result.isError),
      payload: parseEmbeddedJson(content[0].text || ""),
    };
  }
  return {
    isError: Boolean(response.error || result.isError),
    payload: response,
  };
}

async function callToolSafely(client, name, args) {
  try {
    return decodeToolResponse(await client.callTool(name, args));
  } catch (error) {
    return {
      isError: true,
      payload: {
        error: error instanceof Error ? error.message : String(error),
      },
    };
  }
}

module.exports = {
  McpRuntimeClient,
  OWNER_DISCOVERY_SCHEMA,
  callToolSafely,
  decodeToolResponse,
  discoverServeOwner,
};
