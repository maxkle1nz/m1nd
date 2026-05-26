"use strict";

const assert = require("assert");
const fs = require("fs");
const os = require("os");
const path = require("path");
const { spawnSync } = require("child_process");

const {
  agentCommand,
  commandLooksLikeRuntime,
  defaultRuntimePath,
  hostApply,
  hostPlan,
  hostStatus,
  installSkills,
  mcpConfig,
  packRoutingCheck,
  restart,
  runtimeBinaryName,
  selfUpdate,
} = require("../lib/cli");
const { classifyScopeBinding } = require("../lib/agent-cli");

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
const projectForConfig = path.resolve("project");
const codexProjectConfig = mcpConfig(
  "codex",
  "C:\\Users\\you\\.m1nd\\bin\\m1nd-mcp.exe",
  projectForConfig
);
assert(codexProjectConfig.includes("[mcp_servers.m1nd.env]"));
assert(codexProjectConfig.includes(`M1ND_WORKSPACE_ROOT = "${projectForConfig.replace(/\\/g, "\\\\")}"`));

const genericWindowsConfig = JSON.parse(
  mcpConfig("generic", "C:\\Users\\you\\.m1nd\\bin\\m1nd-mcp.exe")
);
assert.strictEqual(
  genericWindowsConfig.mcpServers.m1nd.command,
  "C:\\Users\\you\\.m1nd\\bin\\m1nd-mcp.exe"
);
assert.deepStrictEqual(genericWindowsConfig.mcpServers.m1nd.args, ["--stdio", "--no-gui"]);
const genericProjectConfig = JSON.parse(
  mcpConfig("generic", "C:\\Users\\you\\.m1nd\\bin\\m1nd-mcp.exe", projectForConfig)
);
assert.strictEqual(genericProjectConfig.mcpServers.m1nd.env.M1ND_WORKSPACE_ROOT, projectForConfig);

const help = spawnSync(process.execPath, [cli, "--help"], { encoding: "utf8" });
assert.strictEqual(help.status, 0, help.stderr);
assert(help.stdout.includes("m1nd installer"));
assert(help.stdout.includes("m1nd smoke"));
assert(help.stdout.includes("m1nd restart"));
assert(help.stdout.includes("m1nd update"));
assert(help.stdout.includes("m1nd update status"));
assert(help.stdout.includes("m1nd hosts status"));
assert(help.stdout.includes("m1nd hosts plan"));
assert(help.stdout.includes("m1nd hosts apply"));
assert(help.stdout.includes("m1nd agent scope"));
assert(help.stdout.includes("m1nd agent orient"));
assert(help.stdout.includes("m1nd agent auto"));
assert(help.stdout.includes("m1nd agent next"));
assert(help.stdout.includes("m1nd pack-routing-check"));

const packCheck = spawnSync(process.execPath, [cli, "pack-check", "--json"], { encoding: "utf8" });
assert.strictEqual(packCheck.status, 0, packCheck.stderr);
assert.strictEqual(JSON.parse(packCheck.stdout).schema, "m1nd-agent-pack-check-v0");

const packRouting = spawnSync(process.execPath, [cli, "pack-routing-check", "--json"], { encoding: "utf8" });
assert.strictEqual(packRouting.status, 0, packRouting.stderr);
const packRoutingJson = JSON.parse(packRouting.stdout);
assert.strictEqual(packRoutingJson.schema, "m1nd-agent-pack-routing-check-v0");
assert.strictEqual(packRoutingJson.ok, true);
assert(packRoutingJson.contract_checks.some((check) => check.id === "direct-proof-is-final-truth" && check.ok));
assert(packRoutingJson.files.some((file) => file.id === "m1nd-universal-agent-pack" && file.ok));

const brokenRoutingFile = path.join(fs.mkdtempSync(path.join(os.tmpdir(), "m1nd-routing-broken-")), "pack.md");
fs.writeFileSync(brokenRoutingFile, "session companion continuity only\n");
const brokenRouting = packRoutingCheck({
  files: [
    {
      id: "broken",
      path: brokenRoutingFile,
      checks: [
        { id: "missing-agent-next", needles: ["m1nd agent next"] },
      ],
    },
  ],
  contractChecks: [
    { id: "missing-direct-proof", needles: ["direct proof"] },
  ],
});
assert.strictEqual(brokenRouting.schema, "m1nd-agent-pack-routing-check-v0");
assert.strictEqual(brokenRouting.ok, false);
assert(brokenRouting.missing.some((entry) => entry.check === "missing-agent-next"));
assert(brokenRouting.missing.some((entry) => entry.check === "missing-direct-proof"));

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

function withEnv(overrides, fn) {
  const previous = {};
  for (const [key, value] of Object.entries(overrides)) {
    previous[key] = process.env[key];
    if (value === undefined || value === null) {
      delete process.env[key];
    } else {
      process.env[key] = String(value);
    }
  }
  try {
    return fn();
  } finally {
    for (const [key, value] of Object.entries(previous)) {
      if (value === undefined) {
        delete process.env[key];
      } else {
        process.env[key] = value;
      }
    }
  }
}

function mkTmpDir() {
  return fs.mkdtempSync(path.join(os.tmpdir(), "m1nd-cli-test-"));
}

function writeFakeBinary(file, content = "fake runtime\n") {
  fs.mkdirSync(path.dirname(file), { recursive: true });
  fs.writeFileSync(file, content);
  if (process.platform !== "win32") fs.chmodSync(file, 0o755);
}

function writeFakeMcpRuntime(file) {
  fs.mkdirSync(path.dirname(file), { recursive: true });
  fs.writeFileSync(
    file,
    `#!/usr/bin/env node
if (process.argv.includes("--version")) {
  console.log("m1nd-mcp 0.9.0-beta.6");
  process.exit(0);
}
const readline = require("readline");
const rl = readline.createInterface({ input: process.stdin });
function write(id, result) {
  process.stdout.write(JSON.stringify({ jsonrpc: "2.0", id, result }) + "\\n");
}
function graph() {
  return {
    node_count: Number(process.env.M1ND_FAKE_NODE_COUNT || 12),
    edge_count: Number(process.env.M1ND_FAKE_EDGE_COUNT || 21),
    finalized: true,
    graph_generation: 1,
    ingest_root_count: 1,
    workspace_root: process.env.M1ND_WORKSPACE_ROOT,
    runtime_root: process.env.M1ND_RUNTIME_BASE || null
  };
}
function tool(payload) {
  return { content: [{ type: "text", text: JSON.stringify(payload) }], isError: false };
}
rl.on("line", (line) => {
  const req = JSON.parse(line);
  if (req.method === "initialize") return write(req.id, { protocolVersion: "2025-06-18", capabilities: {} });
  if (req.method === "tools/list") return write(req.id, { tools: ["trust_selftest", "session_handshake", "ingest", "search", "seek", "activate", "audit", "glob", "surgical_context_v2"].map((name) => ({ name })) });
  if (req.method !== "tools/call") return write(req.id, {});
  const name = req.params.name;
  const args = req.params.arguments || {};
  const orientBlocked = process.env.M1ND_FAKE_ORIENT_BLOCKED === "1" || process.env.M1ND_FAKE_SEARCH_BLOCKED === "1";
  if (name === "trust_selftest") {
    if (process.env.M1ND_FAKE_TRUST === "needs_ingest") {
      return write(req.id, tool({ schema: "m1nd-trust-selftest-v0", verdict: "needs_ingest", checks: { needs_ingest: true }, graph_state: { ...graph(), node_count: 0, edge_count: 0, finalized: false, ingest_root_count: 0 } }));
    }
    return write(req.id, tool({ schema: "m1nd-trust-selftest-v0", verdict: "full_trust", checks: { needs_ingest: false }, graph_state: graph() }));
  }
  if (name === "ingest") return write(req.id, tool({ schema: "m1nd-ingest-v0", ok: true, graph_state: graph(), path: args.path }));
  if (name === "session_handshake") return write(req.id, tool({ schema: "m1nd-session-handshake-v0", trust_mode: "full_trust", graph_state: graph(), scope: args.scope }));
  if (name === "search") {
    const emptyQueries = new Set(String(process.env.M1ND_FAKE_EMPTY_SEARCH_QUERIES || "").split("|").filter(Boolean));
    if (emptyQueries.has(args.query)) {
      return write(req.id, tool({ proof_state: "blocked", results: [], total_matches: 0, graph_state: graph() }));
    }
    if (orientBlocked) {
      return write(req.id, tool({ proof_state: "blocked", results: [], total_matches: 0, graph_state: graph() }));
    }
    return write(req.id, tool({ proof_state: "proving", results: [{ file_path: process.env.M1ND_FAKE_SEARCH_FILE || "src/session.js" }], total_matches: 1, graph_state: graph() }));
  }
  if (name === "seek") {
    if (orientBlocked) {
      return write(req.id, tool({ proof_state: "blocked", results: [], total_matches: 0, graph_state: graph() }));
    }
    return write(req.id, tool({ proof_state: "proving", results: [{ file_path: process.env.M1ND_FAKE_SEEK_FILE || "src/session.js" }], total_matches: 1, graph_state: graph() }));
  }
  if (name === "glob") {
    if (orientBlocked) {
      return write(req.id, tool({ proof_state: "blocked", results: [], total_matches: 0, graph_state: graph() }));
    }
    return write(req.id, tool({ proof_state: "proving", results: [{ file_path: process.env.M1ND_FAKE_GLOB_FILE || "src/session.js" }], total_matches: 1, graph_state: graph() }));
  }
  if (name === "activate") {
    if (orientBlocked) {
      return write(req.id, tool({ proof_state: "blocked", activated_count: 0, graph_state: graph() }));
    }
    return write(req.id, tool({ proof_state: "proving", activated_count: Number(process.env.M1ND_FAKE_ACTIVATED_COUNT || 2), graph_state: graph() }));
  }
  if (name === "audit") {
    if (orientBlocked) {
      return write(req.id, tool({ proof_state: "blocked", results: [], total_matches: 0, graph_state: graph() }));
    }
    return write(req.id, tool({ proof_state: "proving", results: [{ file_path: process.env.M1ND_FAKE_AUDIT_FILE || "src/architecture.js" }], total_matches: 1, graph_state: graph() }));
  }
  if (name === "surgical_context_v2") return write(req.id, tool({ schema: "m1nd-surgical-context-v2", file_path: args.file_path, graph_state: graph(), context: process.env.M1ND_FAKE_BIG_CONTEXT === "1" ? "x".repeat(5000) : "fake context" }));
  return write(req.id, tool({ schema: "unknown-tool", name, graph_state: graph() }));
});
`
  );
  if (process.platform !== "win32") fs.chmodSync(file, 0o755);
}

function realpathOrSame(file) {
  try {
    return fs.realpathSync.native(file);
  } catch (_) {
    return file;
  }
}

const registryCurrent = JSON.stringify({
  "dist-tags": { beta: "0.9.0-beta.6", latest: "0.9.0-beta.6" },
  version: "0.9.0-beta.6",
});

const fakeEnvBase = {
  M1ND_TEST_NPM_VIEW_JSON: registryCurrent,
  M1ND_TEST_CRATE_VERSION: "0.9.0-beta.6",
  M1ND_TEST_GITHUB_RELEASE_AVAILABLE: "true",
};

withEnv(
  {
    ...fakeEnvBase,
    M1ND_TEST_RUNTIME_VERSION: "m1nd-mcp 0.9.0-beta.6",
  },
  () => {
    const current = selfUpdate({
      _: ["update", "check"],
      binary: process.execPath,
      channel: "beta",
      "no-kill": true,
    });
    assert.strictEqual(current.schema, "m1nd-self-update-v0");
    assert.strictEqual(current.install_state, "current");
    assert.strictEqual(current.requires_host_rebind, false);
    assert.deepStrictEqual(current.planned_actions, []);
    assert(current.non_claims.some((claim) => claim.includes("cached tool list")));
  }
);

withEnv(
  {
    ...fakeEnvBase,
    M1ND_TEST_RUNTIME_VERSION: "m1nd-mcp 0.9.0-beta.6",
  },
  () => {
    const status = selfUpdate({
      _: ["update", "status"],
      binary: process.execPath,
      channel: "beta",
      "no-kill": true,
    });
    assert.strictEqual(status.schema, "m1nd-self-update-v0");
    assert.strictEqual(status.command, "status");
    assert.strictEqual(status.install_state, "current");
    assert(status.status_summary);
    assert.strictEqual(status.status_summary.readiness, "ready");
    assert.strictEqual(status.status_summary.package_runtime_match, true);
    assert.strictEqual(status.status_summary.agent_pack_ok, true);
    assert.strictEqual(status.status_summary.host_rebind_proven, false);
    assert(Array.isArray(status.live_runtime_processes));
    assert(status.doctor);
    assert(status.next_actions.some((action) => action.includes("update verify")));
  }
);

withEnv(
  {
    ...fakeEnvBase,
    M1ND_TEST_RUNTIME_VERSION: "m1nd-mcp 0.8.0",
  },
  () => {
    const stale = selfUpdate({
      _: ["update", "check"],
      binary: process.execPath,
      channel: "beta",
      "no-kill": true,
    });
    assert.strictEqual(stale.install_state, "stale");
    assert(stale.planned_actions.some((planned) => planned.id === "runtime-install-github-release"));
    assert.strictEqual(stale.requires_host_rebind, true);
  }
);

withEnv(fakeEnvBase, () => {
  const missing = selfUpdate({
    _: ["update", "check"],
    binary: path.join(mkTmpDir(), "missing-m1nd-mcp"),
    channel: "beta",
    "no-kill": true,
  });
  assert.strictEqual(missing.install_state, "missing");
  assert(missing.planned_actions.some((planned) => planned.kind === "runtime"));
});

withEnv(
  {
    ...fakeEnvBase,
    M1ND_TEST_RUNTIME_VERSION: "m1nd-mcp 0.8.0",
  },
  () => {
    const dryRun = selfUpdate({
      _: ["update", "apply"],
      binary: process.execPath,
      channel: "beta",
      "no-kill": true,
    });
    assert.strictEqual(dryRun.dry_run, true);
    assert.deepStrictEqual(dryRun.applied_actions, []);
    assert(dryRun.next_actions.some((action) => action.includes("--yes")));
  }
);

withEnv(
  {
    ...fakeEnvBase,
    M1ND_TEST_RUNTIME_VERSION: "m1nd-mcp 0.8.0",
  },
  () => {
    const statePath = path.join(mkTmpDir(), "update-state.json");
    const noRuntime = withEnv(
      {
        M1ND_UPDATE_STATE_PATH: statePath,
        M1ND_UPDATE_BACKUP_DIR: path.join(path.dirname(statePath), "backups"),
      },
      () =>
        selfUpdate({
          _: ["update", "apply"],
          binary: process.execPath,
          channel: "beta",
          yes: true,
          "no-runtime": true,
          "no-npm": true,
          "no-skills": true,
          "no-kill": true,
        })
    );
    assert.strictEqual(noRuntime.applied_actions.length, 0);
    assert(noRuntime.blocked_actions.some((blocked) => blocked.id === "runtime-disabled"));
    assert.strictEqual(noRuntime.requires_host_rebind, false);
    assert.strictEqual(fs.existsSync(statePath), false);
  }
);

withEnv(
  {
    ...fakeEnvBase,
    M1ND_TEST_RUNTIME_VERSION: "m1nd-mcp 0.8.0",
  },
  () => {
    const tmp = mkTmpDir();
    const target = path.join(tmp, runtimeBinaryName());
    const release = path.join(tmp, "release-m1nd-mcp");
    const statePath = path.join(tmp, "update-state.json");
    writeFakeBinary(target, "old runtime\n");
    writeFakeBinary(release, "new runtime\n");

    const applied = withEnv(
      {
        M1ND_TEST_RELEASE_ASSET_PATH: release,
        M1ND_UPDATE_BACKUP_DIR: path.join(tmp, "backups"),
        M1ND_UPDATE_STATE_PATH: statePath,
      },
      () =>
        selfUpdate({
          _: ["update", "apply"],
          binary: target,
          channel: "beta",
          yes: true,
          "no-npm": true,
          "no-skills": true,
          "no-kill": true,
        })
    );

    assert(applied.applied_actions.some((entry) => entry.id === "runtime-install-github-release" && entry.ok));
    assert(applied.applied_actions.some((entry) => entry.id === "runtime-install-github-release" && entry.version_verified === false));
    assert.strictEqual(fs.readFileSync(target, "utf8"), "new runtime\n");
    const state = JSON.parse(fs.readFileSync(statePath, "utf8"));
    assert(fs.existsSync(state.backup_binary));
    assert.strictEqual(fs.readFileSync(state.backup_binary, "utf8"), "old runtime\n");
    assert.strictEqual(applied.requires_host_rebind, true);

    const rollback = withEnv(
      {
        ...fakeEnvBase,
        M1ND_UPDATE_BACKUP_DIR: path.join(tmp, "backups"),
        M1ND_UPDATE_STATE_PATH: statePath,
      },
      () =>
        selfUpdate({
          _: ["update", "rollback"],
          binary: target,
          channel: "beta",
        })
    );
    assert(rollback.applied_actions.some((entry) => entry.id === "runtime-rollback" && entry.ok));
    assert.strictEqual(fs.readFileSync(target, "utf8"), "old runtime\n");
  }
);

withEnv(
  {
    ...fakeEnvBase,
    M1ND_TEST_RUNTIME_VERSION: "m1nd-mcp 0.9.0-beta.6",
  },
  () => {
    const tmp = mkTmpDir();
    const missing = hostStatus({
      _: ["hosts", "status"],
      host: "claude",
      project: tmp,
      binary: process.execPath,
    });
    assert.strictEqual(missing.schema, "m1nd-host-readiness-v0");
    assert.strictEqual(missing.summary.host_rebind_proven, false);
    assert.strictEqual(missing.hosts.length, 1);
    assert.strictEqual(missing.hosts[0].host, "claude");
    assert.strictEqual(missing.hosts[0].readiness, "attention");
    assert.strictEqual(missing.hosts[0].agent_pack.installed, false);
    assert.strictEqual(missing.hosts[0].config.status, "missing");
    assert(missing.non_claims.some((claim) => claim.includes("does not mutate")));

    installSkills("claude", tmp);
    const staleSkillArtifact = path.join(
      tmp,
      ".m1nd",
      "agent-pack",
      "skills",
      "m1nd-operator",
      "graph_snapshot.json"
    );
    fs.writeFileSync(staleSkillArtifact, "{}");
    assert(fs.existsSync(staleSkillArtifact));
    installSkills("claude", tmp);
    assert(!fs.existsSync(staleSkillArtifact));
    fs.mkdirSync(path.join(tmp, ".claude"), { recursive: true });
    fs.writeFileSync(
      path.join(tmp, ".claude", "mcp.json"),
      mcpConfig("claude", process.execPath, tmp)
    );

    const ready = hostStatus({
      _: ["hosts", "status"],
      host: "claude",
      project: tmp,
      binary: process.execPath,
    });
    assert.strictEqual(ready.hosts[0].agent_pack.installed, true);
    assert.strictEqual(ready.hosts[0].config.status, "configured");
    assert.strictEqual(ready.hosts[0].config.workspace_configured, true);
    assert.strictEqual(ready.hosts[0].readiness, "ready");
    assert.strictEqual(ready.summary.overall_readiness, "ready");
    assert.strictEqual(ready.summary.host_rebind_proven, false);
    assert(!ready.hosts[0].next_actions.some((action) => action.includes("Set M1ND_WORKSPACE_ROOT")));

    const plan = hostPlan({
      _: ["hosts", "plan"],
      host: "claude",
      project: tmp,
      binary: process.execPath,
    });
    assert.strictEqual(plan.schema, "m1nd-host-rebind-plan-v0");
    assert.strictEqual(plan.read_only, true);
    assert.strictEqual(plan.plans[0].host, "claude");
    assert.strictEqual(plan.plans[0].workspace_binding.env.M1ND_WORKSPACE_ROOT, path.resolve(tmp));
    assert(plan.plans[0].configure_mcp.snippet.includes("M1ND_WORKSPACE_ROOT"));
    assert.strictEqual(plan.plans[0].host_rebind_proven, false);
    assert(plan.non_claims.some((claim) => claim.includes("does not mutate")));
  }
);

withEnv(fakeEnvBase, () => {
  const tmp = mkTmpDir();
  const selectedRuntime = path.join(tmp, "managed", runtimeBinaryName());
  const pathRuntimeDir = path.join(tmp, "path");
  const pathRuntime = path.join(pathRuntimeDir, runtimeBinaryName());
  writeFakeBinary(selectedRuntime);
  writeFakeBinary(pathRuntime);
  installSkills("claude", tmp);
  fs.mkdirSync(path.join(tmp, ".claude"), { recursive: true });
  fs.writeFileSync(path.join(tmp, ".claude", "mcp.json"), mcpConfig("claude", selectedRuntime, tmp));

  const status = withEnv(
    {
      PATH: `${pathRuntimeDir}${path.delimiter}${process.env.PATH || ""}`,
      M1ND_TEST_RUNTIME_VERSION_BY_PATH: JSON.stringify({
        [selectedRuntime]: "m1nd-mcp 0.9.0-beta.6",
        [realpathOrSame(selectedRuntime)]: "m1nd-mcp 0.9.0-beta.6",
        [pathRuntime]: "m1nd-mcp 0.8.0",
        [realpathOrSame(pathRuntime)]: "m1nd-mcp 0.8.0",
      }),
    },
    () =>
      hostStatus({
        _: ["hosts", "status"],
        host: "claude",
        project: tmp,
        binary: selectedRuntime,
      })
  );

  assert.strictEqual(status.runtime.current, true);
  assert.strictEqual(status.runtime.path_runtime_current, false);
  assert.strictEqual(status.hosts[0].config.selected_runtime_configured_current, true);
  assert.strictEqual(status.hosts[0].path_shadow.status, "shadow_warning");
  assert.strictEqual(status.hosts[0].path_shadow.blocking, false);
  assert.strictEqual(status.hosts[0].readiness, "ready");
  assert(status.hosts[0].warnings.some((warning) => warning.includes("PATH has a stale")));
  assert(!status.hosts[0].next_actions.some((action) => action.includes("Align the m1nd-mcp binary found on PATH")));

  const plan = withEnv(
    {
      PATH: `${pathRuntimeDir}${path.delimiter}${process.env.PATH || ""}`,
      M1ND_TEST_RUNTIME_VERSION_BY_PATH: JSON.stringify({
        [selectedRuntime]: "m1nd-mcp 0.9.0-beta.6",
        [realpathOrSame(selectedRuntime)]: "m1nd-mcp 0.9.0-beta.6",
        [pathRuntime]: "m1nd-mcp 0.8.0",
        [realpathOrSame(pathRuntime)]: "m1nd-mcp 0.8.0",
      }),
    },
    () =>
      hostPlan({
        _: ["hosts", "plan"],
        host: "claude",
        project: tmp,
        binary: selectedRuntime,
      })
  );
  assert.strictEqual(plan.plans[0].runtime.path_shadow.status, "shadow_warning");
  assert.strictEqual(plan.plans[0].runtime.path_shadow.blocking, false);
});

withEnv(fakeEnvBase, () => {
  const testHome = mkTmpDir();
  const project = mkTmpDir();
  const selectedRuntime = path.join(testHome, ".m1nd", "bin", runtimeBinaryName());
  const foreignRuntime = path.join(testHome, "foreign", runtimeBinaryName());
  writeFakeBinary(selectedRuntime);
  writeFakeBinary(foreignRuntime);
  fs.mkdirSync(path.join(testHome, ".codex"), { recursive: true });
  fs.writeFileSync(
    path.join(testHome, ".codex", "config.toml"),
    `${mcpConfig("codex", selectedRuntime, project)}

[mcp_servers.dexter.env]
M1ND_MCP_BINARY = "${foreignRuntime}"
`
  );

  const status = withEnv(
    {
      M1ND_TEST_HOME: testHome,
      M1ND_TEST_RUNTIME_VERSION_BY_PATH: JSON.stringify({
        [selectedRuntime]: "m1nd-mcp 0.9.0-beta.6",
        [realpathOrSame(selectedRuntime)]: "m1nd-mcp 0.9.0-beta.6",
        [foreignRuntime]: "m1nd-mcp 0.8.0",
        [realpathOrSame(foreignRuntime)]: "m1nd-mcp 0.8.0",
      }),
    },
    () =>
      hostStatus({
        _: ["hosts", "status"],
        host: "codex",
        project,
        binary: selectedRuntime,
      })
  );

  assert.strictEqual(status.hosts[0].config.selected_runtime_configured_current, true);
  assert(!status.hosts[0].config.runtime_bindings.some((binding) => binding.path === foreignRuntime));
  assert.strictEqual(status.hosts[0].readiness, "attention");
});

withEnv(
  {
    ...fakeEnvBase,
    M1ND_TEST_RUNTIME_VERSION: "m1nd-mcp 0.9.0-beta.6",
  },
  () => {
    const tmp = mkTmpDir();
    const dryRun = hostApply({
      _: ["hosts", "apply"],
      host: "claude",
      project: tmp,
      binary: process.execPath,
    });
    assert.strictEqual(dryRun.schema, "m1nd-host-apply-v0");
    assert.strictEqual(dryRun.dry_run, true);
    assert.strictEqual(dryRun.applied_actions.length, 0);
    assert.strictEqual(fs.existsSync(path.join(tmp, ".claude", "mcp.json")), false);
    assert.strictEqual(fs.existsSync(path.join(tmp, ".m1nd", "agent-pack")), false);
    assert.strictEqual(dryRun.requires_host_rebind, true);
    assert.strictEqual(dryRun.host_rebind_proven, false);
    assert(dryRun.non_claims.some((claim) => claim.includes("cached MCP tool list")));

    const applied = hostApply({
      _: ["hosts", "apply"],
      host: "claude",
      project: tmp,
      binary: process.execPath,
      yes: true,
    });
    assert.strictEqual(applied.dry_run, false);
    assert(applied.applied_actions.some((entry) => entry.id === "install-agent-pack" && entry.ok));
    assert(applied.applied_actions.some((entry) => entry.id === "write-mcp-config" && entry.ok));
    assert(fs.existsSync(path.join(tmp, ".m1nd", "agent-pack", "CLAUDE.md")));
    const claudeConfig = JSON.parse(fs.readFileSync(path.join(tmp, ".claude", "mcp.json"), "utf8"));
    assert.strictEqual(claudeConfig.mcpServers.m1nd.command, process.execPath);
    assert.strictEqual(claudeConfig.mcpServers.m1nd.env.M1ND_WORKSPACE_ROOT, path.resolve(tmp));
    assert(applied.changed_files.includes(path.join(tmp, ".claude", "mcp.json")));
    assert(applied.next_actions.some((entry) => entry.includes("Restart or rebind")));

    const idempotent = hostApply({
      _: ["hosts", "apply"],
      host: "claude",
      project: tmp,
      binary: process.execPath,
      yes: true,
    });
    assert(idempotent.applied_actions.some((entry) => entry.id === "write-mcp-config" && entry.changed === false));
  }
);

withEnv(
  {
    ...fakeEnvBase,
    M1ND_TEST_RUNTIME_VERSION: "m1nd-mcp 0.9.0-beta.6",
  },
  () => {
    const tmp = mkTmpDir();
    const generic = hostApply({
      _: ["hosts", "apply"],
      host: "generic",
      project: tmp,
      binary: process.execPath,
      yes: true,
    });
    assert(fs.existsSync(path.join(tmp, ".m1nd", "agent-pack", "m1nd-agent-rules.md")));
    assert(generic.blocked_actions.some((entry) => entry.id === "config-manual"));
    assert.strictEqual(generic.host_rebind_proven, false);

    const disabled = hostApply({
      _: ["hosts", "apply"],
      host: "claude",
      project: mkTmpDir(),
      binary: process.execPath,
      yes: true,
      "no-skills": true,
      "no-config": true,
    });
    assert.strictEqual(disabled.applied_actions.length, 0);
    assert(disabled.blocked_actions.some((entry) => entry.id === "agent-pack-disabled"));
    assert(disabled.blocked_actions.some((entry) => entry.id === "config-disabled"));
  }
);

withEnv(
  {
    ...fakeEnvBase,
    M1ND_TEST_RUNTIME_VERSION: "m1nd-mcp 0.9.0-beta.6",
  },
  () => {
    const testHome = mkTmpDir();
    const project = mkTmpDir();
    const applied = withEnv(
      {
        M1ND_TEST_HOME: testHome,
      },
      () =>
        hostApply({
          _: ["hosts", "apply"],
          host: "codex",
          project,
          binary: process.execPath,
          yes: true,
        })
    );
    const configPath = path.join(testHome, ".codex", "config.toml");
    assert(fs.existsSync(path.join(testHome, ".codex", "skills", "m1nd-first", "SKILL.md")));
    assert(fs.existsSync(configPath));
    const config = fs.readFileSync(configPath, "utf8");
    assert(config.includes("[mcp_servers.m1nd]"));
    assert(config.includes("[mcp_servers.m1nd.env]"));
    assert(config.includes(`M1ND_WORKSPACE_ROOT = "${path.resolve(project).replace(/\\/g, "\\\\")}"`));
    assert(applied.changed_files.includes(configPath));
  }
);

const updateCheck = spawnSync(process.execPath, [cli, "update", "check", "--json", "--binary", process.execPath], {
  encoding: "utf8",
  env: {
    ...process.env,
    ...fakeEnvBase,
    M1ND_TEST_RUNTIME_VERSION: "m1nd-mcp 0.9.0-beta.6",
  },
});
assert.strictEqual(updateCheck.status, 0, updateCheck.stderr);
assert.strictEqual(JSON.parse(updateCheck.stdout).schema, "m1nd-self-update-v0");

const updateStatus = spawnSync(process.execPath, [cli, "update", "status", "--json", "--binary", process.execPath], {
  encoding: "utf8",
  env: {
    ...process.env,
    ...fakeEnvBase,
    M1ND_TEST_RUNTIME_VERSION: "m1nd-mcp 0.9.0-beta.6",
  },
});
assert.strictEqual(updateStatus.status, 0, updateStatus.stderr);
const updateStatusJson = JSON.parse(updateStatus.stdout);
assert.strictEqual(updateStatusJson.schema, "m1nd-self-update-v0");
assert.strictEqual(updateStatusJson.command, "status");
assert(updateStatusJson.status_summary);

const hostStatusCliProject = mkTmpDir();
installSkills("generic", hostStatusCliProject);
const hostsStatus = spawnSync(
  process.execPath,
  [cli, "hosts", "status", "--json", "--host", "generic", "--project", hostStatusCliProject, "--binary", process.execPath],
  {
    encoding: "utf8",
    env: {
      ...process.env,
      ...fakeEnvBase,
      M1ND_TEST_RUNTIME_VERSION: "m1nd-mcp 0.9.0-beta.6",
    },
  }
);
assert.strictEqual(hostsStatus.status, 0, hostsStatus.stderr);
const hostsStatusJson = JSON.parse(hostsStatus.stdout);
assert.strictEqual(hostsStatusJson.schema, "m1nd-host-readiness-v0");
assert.strictEqual(hostsStatusJson.hosts[0].host, "generic");
assert.strictEqual(hostsStatusJson.hosts[0].config.status, "manual");
assert.strictEqual(hostsStatusJson.hosts[0].readiness, "attention");

const hostsPlan = spawnSync(
  process.execPath,
  [cli, "hosts", "plan", "--json", "--host", "generic", "--project", hostStatusCliProject, "--binary", process.execPath],
  {
    encoding: "utf8",
    env: {
      ...process.env,
      ...fakeEnvBase,
      M1ND_TEST_RUNTIME_VERSION: "m1nd-mcp 0.9.0-beta.6",
    },
  }
);
assert.strictEqual(hostsPlan.status, 0, hostsPlan.stderr);
const hostsPlanJson = JSON.parse(hostsPlan.stdout);
assert.strictEqual(hostsPlanJson.schema, "m1nd-host-rebind-plan-v0");
assert.strictEqual(hostsPlanJson.plans[0].configure_mcp.status, "manual");
assert(hostsPlanJson.plans[0].configure_mcp.snippet.includes("M1ND_WORKSPACE_ROOT"));

const hostApplyCliProject = mkTmpDir();
const hostApplyCli = spawnSync(
  process.execPath,
  [cli, "hosts", "apply", "--json", "--host", "antigravity", "--project", hostApplyCliProject, "--binary", process.execPath, "--yes"],
  {
    encoding: "utf8",
    env: {
      ...process.env,
      ...fakeEnvBase,
      M1ND_TEST_RUNTIME_VERSION: "m1nd-mcp 0.9.0-beta.6",
    },
  }
);
assert.strictEqual(hostApplyCli.status, 0, hostApplyCli.stderr);
const hostApplyCliJson = JSON.parse(hostApplyCli.stdout);
assert.strictEqual(hostApplyCliJson.schema, "m1nd-host-apply-v0");
assert.strictEqual(hostApplyCliJson.dry_run, false);
assert.strictEqual(hostApplyCliJson.requires_host_rebind, true);
assert(fs.existsSync(path.join(hostApplyCliProject, "mcp_config.json")));
assert(fs.existsSync(path.join(hostApplyCliProject, ".m1nd", "agent-pack", "AGENTS.md")));

const scopeRepo = mkTmpDir();
const nestedScope = path.join(scopeRepo, "src");
fs.mkdirSync(nestedScope);
const fileScope = path.join(scopeRepo, "PRD.md");
fs.writeFileSync(fileScope, "# prd\n");
assert.strictEqual(classifyScopeBinding(scopeRepo, scopeRepo).binding_kind, "full_repo_binding");
assert.strictEqual(classifyScopeBinding(scopeRepo, nestedScope).binding_kind, "nested_workspace_binding");
assert.strictEqual(classifyScopeBinding(scopeRepo, fileScope).binding_kind, "file_level_binding");
assert.strictEqual(classifyScopeBinding(scopeRepo, mkTmpDir()).binding_kind, "wrong_workspace_binding");
assert.strictEqual(classifyScopeBinding(scopeRepo, null).binding_kind, "ambiguous_scope");

const fakeMcp = path.join(mkTmpDir(), runtimeBinaryName());
writeFakeMcpRuntime(fakeMcp);
const agentEnv = {
  ...process.env,
  ...fakeEnvBase,
  M1ND_TEST_RUNTIME_VERSION: "m1nd-mcp 0.9.0-beta.6",
};
const agentScopeRepo = mkTmpDir();
const agentScope = spawnSync(
  process.execPath,
  [cli, "agent", "scope", "--repo", agentScopeRepo, "--binary", fakeMcp, "--json"],
  {
    encoding: "utf8",
    env: {
      ...agentEnv,
      M1ND_WORKSPACE_ROOT: mkTmpDir(),
    },
  }
);
assert.strictEqual(agentScope.status, 0, agentScope.stderr);
const agentScopeJson = JSON.parse(agentScope.stdout);
assert.strictEqual(agentScopeJson.schema, "m1nd-agent-cli-v0");
assert.strictEqual(agentScopeJson.command, "scope");
assert.strictEqual(agentScopeJson.scope_alignment.binding_kind, "full_repo_binding");
assert.strictEqual(agentScopeJson.scope_alignment.ambient_binding_kind, "wrong_workspace_binding");

const agentTrust = spawnSync(
  process.execPath,
  [cli, "agent", "trust", "--repo", agentScopeRepo, "--binary", fakeMcp, "--ensure-ingest", "--json"],
  {
    encoding: "utf8",
    env: {
      ...agentEnv,
      M1ND_FAKE_TRUST: "needs_ingest",
    },
  }
);
assert.strictEqual(agentTrust.status, 0, agentTrust.stderr);
const agentTrustJson = JSON.parse(agentTrust.stdout);
assert.strictEqual(agentTrustJson.schema, "m1nd-agent-cli-v0");
assert.strictEqual(agentTrustJson.command, "trust");
assert(agentTrustJson.calls.some((entry) => entry.tool === "ingest"));
assert.strictEqual(agentTrustJson.trust.verdict, "full_trust");

const agentOrientRepo = mkTmpDir();
const agentOrient = spawnSync(
  process.execPath,
  [cli, "agent", "orient", "--repo", agentOrientRepo, "--binary", fakeMcp, "--query", "session boundary", "--mode", "short", "--json"],
  { encoding: "utf8", env: agentEnv }
);
assert.strictEqual(agentOrient.status, 0, agentOrient.stderr);
const agentOrientJson = JSON.parse(agentOrient.stdout);
assert.strictEqual(agentOrientJson.schema, "m1nd-agent-cli-v0");
assert.strictEqual(agentOrientJson.command, "orient");
assert.strictEqual(agentOrientJson.switch_to_direct_proof, true);
assert(agentOrientJson.calls.some((entry) => entry.tool === "seek"));
assert.strictEqual(agentOrientJson.action.schema, "m1nd-agent-action-envelope-v0");
assert.strictEqual(agentOrientJson.action.route.kind, "direct_proof");
assert.strictEqual(fs.existsSync(path.join(agentOrientRepo, "graph_snapshot.json")), false);
assert.strictEqual(fs.existsSync(path.join(agentOrientRepo, "plasticity_state.json")), false);

const agentBlocked = spawnSync(
  process.execPath,
  [cli, "agent", "orient", "--repo", agentOrientRepo, "--binary", fakeMcp, "--query", "session boundary", "--mode", "short", "--json"],
  {
    encoding: "utf8",
    env: {
      ...agentEnv,
      M1ND_FAKE_SEARCH_BLOCKED: "1",
    },
  }
);
assert.strictEqual(agentBlocked.status, 0, agentBlocked.stderr);
const agentBlockedJson = JSON.parse(agentBlocked.stdout);
assert.strictEqual(agentBlockedJson.m1nd_usage_mode, "recovery_overhead");
assert(agentBlockedJson.next_actions.some((entry) => entry.includes("recover")));

const agentRecover = spawnSync(
  process.execPath,
  [cli, "agent", "recover", "--repo", agentOrientRepo, "--binary", fakeMcp, "--from", "Transport closed", "--json"],
  { encoding: "utf8", env: agentEnv }
);
assert.strictEqual(agentRecover.status, 0, agentRecover.stderr);
const agentRecoverJson = JSON.parse(agentRecover.stdout);
assert.strictEqual(agentRecoverJson.command, "recover");
assert.strictEqual(agentRecoverJson.recovery_type, "transport_closed");
assert(agentRecoverJson.recovery_plan.some((step) => String(step.command).includes("agent doctor")));
assert(agentRecoverJson.recovery_plan.some((step) => String(step.command).includes(`--binary ${fakeMcp}`)));

const agentAuto = spawnSync(
  process.execPath,
  [cli, "agent", "auto", "--repo", agentOrientRepo, "--query", "session boundary", "--binary", fakeMcp, "--json"],
  { encoding: "utf8", env: agentEnv }
);
assert.strictEqual(agentAuto.status, 0, agentAuto.stderr);
const agentAutoJson = JSON.parse(agentAuto.stdout);
assert.strictEqual(agentAutoJson.command, "auto");
assert.strictEqual(agentAutoJson.action.schema, "m1nd-agent-action-envelope-v0");
assert.strictEqual(agentAutoJson.action.route.kind, "orient");
assert.strictEqual(agentAutoJson.action.route.tool, "seek");
assert.strictEqual(agentAutoJson.action.trigger.kind, "natural_language");
assert(agentAutoJson.action.action.command.includes(`--binary ${fakeMcp}`));
assert(agentAutoJson.next_actions[0].includes(`--binary ${fakeMcp}`));

const agentAutoSymbol = spawnSync(
  process.execPath,
  [cli, "agent", "auto", "--repo", agentOrientRepo, "--query", "chooseOrientationTool", "--binary", fakeMcp, "--json"],
  { encoding: "utf8", env: agentEnv }
);
assert.strictEqual(agentAutoSymbol.status, 0, agentAutoSymbol.stderr);
const agentAutoSymbolJson = JSON.parse(agentAutoSymbol.stdout);
assert.strictEqual(agentAutoSymbolJson.action.route.tool, "search");
assert.strictEqual(agentAutoSymbolJson.action.trigger.kind, "exact_identifier");

const agentAutoPackage = spawnSync(
  process.execPath,
  [cli, "agent", "auto", "--repo", agentOrientRepo, "--query", "@maxkle1nz/m1nd", "--binary", fakeMcp, "--json"],
  { encoding: "utf8", env: agentEnv }
);
assert.strictEqual(agentAutoPackage.status, 0, agentAutoPackage.stderr);
const agentAutoPackageJson = JSON.parse(agentAutoPackage.stdout);
assert.strictEqual(agentAutoPackageJson.action.route.kind, "orient");
assert.strictEqual(agentAutoPackageJson.action.route.tool, "seek");

const agentAutoUrl = spawnSync(
  process.execPath,
  [cli, "agent", "auto", "--repo", agentOrientRepo, "--query", "https://github.com/maxkle1nz/m1nd", "--binary", fakeMcp, "--json"],
  { encoding: "utf8", env: agentEnv }
);
assert.strictEqual(agentAutoUrl.status, 0, agentAutoUrl.stderr);
const agentAutoUrlJson = JSON.parse(agentAutoUrl.stdout);
assert.strictEqual(agentAutoUrlJson.action.route.kind, "orient");
assert.strictEqual(agentAutoUrlJson.action.route.tool, "seek");

const agentAutoOverride = spawnSync(
  process.execPath,
  [cli, "agent", "auto", "--repo", agentOrientRepo, "--query", "session boundary", "--tool", "search", "--binary", fakeMcp, "--json"],
  { encoding: "utf8", env: agentEnv }
);
assert.strictEqual(agentAutoOverride.status, 0, agentAutoOverride.stderr);
const agentAutoOverrideJson = JSON.parse(agentAutoOverride.stdout);
assert.strictEqual(agentAutoOverrideJson.action.route.tool, "search");
assert.strictEqual(agentAutoOverrideJson.action.trigger.kind, "explicit_tool_override");

const agentAutoTransportClosed = spawnSync(
  process.execPath,
  [cli, "agent", "next", "--repo", agentOrientRepo, "--from", "Transport closed", "--binary", fakeMcp, "--json"],
  { encoding: "utf8", env: agentEnv }
);
assert.strictEqual(agentAutoTransportClosed.status, 0, agentAutoTransportClosed.stderr);
const agentAutoTransportClosedJson = JSON.parse(agentAutoTransportClosed.stdout);
assert.strictEqual(agentAutoTransportClosedJson.command, "next");
assert.strictEqual(agentAutoTransportClosedJson.resolved_command, "auto");
assert.strictEqual(agentAutoTransportClosedJson.action.route.kind, "recover");
assert.strictEqual(agentAutoTransportClosedJson.action.route.recovery_type, "transport_closed");
assert(agentAutoTransportClosedJson.action.action.command.includes(`--binary ${fakeMcp}`));
assert(agentAutoTransportClosedJson.next_actions[0].includes(`--binary ${fakeMcp}`));

const agentNextContextFile = path.join(agentOrientRepo, "agent-cli.js");
fs.writeFileSync(agentNextContextFile, "// route me\n");
const agentNextContext = spawnSync(
  process.execPath,
  [cli, "agent", "next", "--repo", agentOrientRepo, "--query", "agent-cli.js", "--binary", fakeMcp, "--json"],
  { encoding: "utf8", env: agentEnv }
);
assert.strictEqual(agentNextContext.status, 0, agentNextContext.stderr);
const agentNextContextJson = JSON.parse(agentNextContext.stdout);
assert.strictEqual(agentNextContextJson.command, "next");
assert.strictEqual(agentNextContextJson.resolved_command, "auto");
assert.strictEqual(agentNextContextJson.action.route.kind, "context");
assert(agentNextContextJson.action.action.command.includes(`--binary ${fakeMcp}`));
assert(agentNextContextJson.next_actions[0].includes(`--binary ${fakeMcp}`));

const agentAutoBlocked = spawnSync(
  process.execPath,
  [cli, "agent", "auto", "--repo", agentOrientRepo, "--from", "stdin", "--binary", fakeMcp, "--json"],
  {
    encoding: "utf8",
    input: agentBlocked.stdout,
    env: agentEnv,
  }
);
assert.strictEqual(agentAutoBlocked.status, 0, agentAutoBlocked.stderr);
const agentAutoBlockedJson = JSON.parse(agentAutoBlocked.stdout);
assert.strictEqual(agentAutoBlockedJson.action.route.kind, "recover");
assert.strictEqual(agentAutoBlockedJson.action.route.recovery_type, "blocked_retrieval");

const agentAutoWrongWorkspace = spawnSync(
  process.execPath,
  [cli, "agent", "auto", "--repo", agentOrientRepo, "--from", "stdin", "--binary", fakeMcp, "--json"],
  {
    encoding: "utf8",
    input: JSON.stringify({
      proof_state: "blocked",
      context_guard: { wrong_workspace_binding: true },
      recovery: { binding_issue: "wrong_workspace_binding" },
    }),
    env: agentEnv,
  }
);
assert.strictEqual(agentAutoWrongWorkspace.status, 0, agentAutoWrongWorkspace.stderr);
const agentAutoWrongWorkspaceJson = JSON.parse(agentAutoWrongWorkspace.stdout);
assert.strictEqual(agentAutoWrongWorkspaceJson.action.route.kind, "recover");
assert.strictEqual(agentAutoWrongWorkspaceJson.action.route.recovery_type, "wrong_workspace_binding");

const agentActivate = spawnSync(
  process.execPath,
  [cli, "agent", "orient", "--repo", agentOrientRepo, "--binary", fakeMcp, "--query", "review the session orchestration and dependency flow", "--mode", "deep", "--tool", "activate", "--json"],
  { encoding: "utf8", env: agentEnv }
);
assert.strictEqual(agentActivate.status, 0, agentActivate.stderr);
const agentActivateJson = JSON.parse(agentActivate.stdout);
assert(agentActivateJson.calls.some((entry) => entry.tool === "activate"));
assert.strictEqual(agentActivateJson.m1nd_usage_mode, "short_audit_orientation");
assert.strictEqual(agentActivateJson.switch_to_direct_proof, false);

const agentContext = spawnSync(
  process.execPath,
  [cli, "agent", "context", "--repo", agentOrientRepo, "--binary", fakeMcp, "--query", "session boundary", "--tokens", "800", "--json"],
  { encoding: "utf8", env: agentEnv }
);
assert.strictEqual(agentContext.status, 0, agentContext.stderr);
const agentContextJson = JSON.parse(agentContext.stdout);
assert.strictEqual(agentContextJson.command, "context");
assert(agentContextJson.selected_file.endsWith(path.join("src", "session.js")));
assert(agentContextJson.calls.some((entry) => entry.tool === "surgical_context_v2"));

const directContextFile = path.join(agentOrientRepo, "src", "session.js");
fs.mkdirSync(path.dirname(directContextFile), { recursive: true });
fs.writeFileSync(directContextFile, "// direct context anchor\n");
const agentContextPathPhrase = spawnSync(
  process.execPath,
  [cli, "agent", "context", "--repo", agentOrientRepo, "--binary", fakeMcp, "--query", "src/session.js session boundary", "--tokens", "800", "--json"],
  { encoding: "utf8", env: agentEnv }
);
assert.strictEqual(agentContextPathPhrase.status, 0, agentContextPathPhrase.stderr);
const agentContextPathPhraseJson = JSON.parse(agentContextPathPhrase.stdout);
assert.strictEqual(agentContextPathPhraseJson.selected_file, directContextFile);
assert(!agentContextPathPhraseJson.calls.some((entry) => entry.tool === "search"));

const agentContextIdentifierFallback = spawnSync(
  process.execPath,
  [cli, "agent", "context", "--repo", agentOrientRepo, "--binary", fakeMcp, "--query", "packRoutingCheck agent pack routing", "--tokens", "800", "--json"],
  {
    encoding: "utf8",
    env: {
      ...agentEnv,
      M1ND_FAKE_EMPTY_SEARCH_QUERIES: "packRoutingCheck agent pack routing",
      M1ND_FAKE_SEARCH_FILE: "npm/lib/cli.js",
    },
  }
);
assert.strictEqual(agentContextIdentifierFallback.status, 0, agentContextIdentifierFallback.stderr);
const agentContextIdentifierFallbackJson = JSON.parse(agentContextIdentifierFallback.stdout);
assert(agentContextIdentifierFallbackJson.selected_file.endsWith(path.join("npm", "lib", "cli.js")));
assert(agentContextIdentifierFallbackJson.calls.filter((entry) => entry.tool === "search").length >= 2);

const agentContextBudget = spawnSync(
  process.execPath,
  [cli, "agent", "context", "--repo", agentOrientRepo, "--binary", fakeMcp, "--query", "session boundary", "--tokens", "10", "--json"],
  {
    encoding: "utf8",
    env: {
      ...agentEnv,
      M1ND_FAKE_BIG_CONTEXT: "1",
    },
  }
);
assert.strictEqual(agentContextBudget.status, 0, agentContextBudget.stderr);
const agentContextBudgetJson = JSON.parse(agentContextBudget.stdout);
assert(agentContextBudgetJson.results[0].context.includes("truncated by m1nd agent context"));
assert(agentContextBudgetJson.results[0].context.length < 1200);

const agentContextEscape = spawnSync(
  process.execPath,
  [cli, "agent", "context", "--repo", agentOrientRepo, "--binary", fakeMcp, "--query", "session boundary", "--json"],
  {
    encoding: "utf8",
    env: {
      ...agentEnv,
      M1ND_FAKE_SEARCH_FILE: "../escape.js",
    },
  }
);
assert.notStrictEqual(agentContextEscape.status, 0);
assert(agentContextEscape.stderr.includes("path escapes repo"));

const agentDoctor = spawnSync(
  process.execPath,
  [cli, "agent", "doctor", "--repo", agentOrientRepo, "--binary", fakeMcp, "--json"],
  { encoding: "utf8", env: agentEnv }
);
assert.strictEqual(agentDoctor.status, 0, agentDoctor.stderr);
const agentDoctorJson = JSON.parse(agentDoctor.stdout);
assert.strictEqual(agentDoctorJson.command, "doctor");
assert(agentDoctorJson.package_doctor);
assert(agentDoctorJson.hosts);
assert(agentDoctorJson.update);

console.log("npm cli tests ok");
