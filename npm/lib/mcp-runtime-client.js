"use strict";

const fs = require("fs");
const os = require("os");
const path = require("path");
const readline = require("readline");
const { spawn } = require("child_process");

function defaultRuntimeArgs() {
  return (process.env.M1ND_MCP_ARGS || "--stdio --no-gui").trim().split(/\s+/).filter(Boolean);
}

function argsHaveOption(args, option) {
  return args.some((arg) => arg === option || arg.startsWith(`${option}=`));
}

function ensureDir(dir) {
  fs.mkdirSync(dir, { recursive: true });
}

class McpRuntimeClient {
  constructor(options) {
    this.binary = options.binary;
    this.repo = options.repo;
    this.sharedRuntime = Boolean(options.sharedRuntime);
    this.runtimeDir = options.runtimeDir || null;
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
  callToolSafely,
  decodeToolResponse,
};
