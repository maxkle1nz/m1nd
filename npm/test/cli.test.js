"use strict";

const assert = require("assert");
const fs = require("fs");
const os = require("os");
const path = require("path");
const { spawnSync } = require("child_process");

const {
  commandLooksLikeRuntime,
  defaultRuntimePath,
  hostStatus,
  installSkills,
  mcpConfig,
  restart,
  runtimeBinaryName,
  selfUpdate,
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
assert(help.stdout.includes("m1nd update"));
assert(help.stdout.includes("m1nd update status"));
assert(help.stdout.includes("m1nd hosts status"));

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

const registryCurrent = JSON.stringify({
  "dist-tags": { beta: "0.9.0-beta.2", latest: "0.9.0-beta.2" },
  version: "0.9.0-beta.2",
});

const fakeEnvBase = {
  M1ND_TEST_NPM_VIEW_JSON: registryCurrent,
  M1ND_TEST_CRATE_VERSION: "0.9.0-beta.2",
  M1ND_TEST_GITHUB_RELEASE_AVAILABLE: "true",
};

withEnv(
  {
    ...fakeEnvBase,
    M1ND_TEST_RUNTIME_VERSION: "m1nd-mcp 0.9.0-beta.2",
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
    M1ND_TEST_RUNTIME_VERSION: "m1nd-mcp 0.9.0-beta.2",
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
    M1ND_TEST_RUNTIME_VERSION: "m1nd-mcp 0.9.0-beta.2",
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
    fs.mkdirSync(path.join(tmp, ".claude"), { recursive: true });
    fs.writeFileSync(
      path.join(tmp, ".claude", "mcp.json"),
      JSON.stringify({ mcpServers: { m1nd: { command: "m1nd-mcp", args: ["--stdio", "--no-gui"] } } })
    );

    const ready = hostStatus({
      _: ["hosts", "status"],
      host: "claude",
      project: tmp,
      binary: process.execPath,
    });
    assert.strictEqual(ready.hosts[0].agent_pack.installed, true);
    assert.strictEqual(ready.hosts[0].config.status, "configured");
    assert.strictEqual(ready.hosts[0].readiness, "ready");
    assert.strictEqual(ready.summary.overall_readiness, "ready");
    assert.strictEqual(ready.summary.host_rebind_proven, false);
  }
);

const updateCheck = spawnSync(process.execPath, [cli, "update", "check", "--json", "--binary", process.execPath], {
  encoding: "utf8",
  env: {
    ...process.env,
    ...fakeEnvBase,
    M1ND_TEST_RUNTIME_VERSION: "m1nd-mcp 0.9.0-beta.2",
  },
});
assert.strictEqual(updateCheck.status, 0, updateCheck.stderr);
assert.strictEqual(JSON.parse(updateCheck.stdout).schema, "m1nd-self-update-v0");

const updateStatus = spawnSync(process.execPath, [cli, "update", "status", "--json", "--binary", process.execPath], {
  encoding: "utf8",
  env: {
    ...process.env,
    ...fakeEnvBase,
    M1ND_TEST_RUNTIME_VERSION: "m1nd-mcp 0.9.0-beta.2",
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
      M1ND_TEST_RUNTIME_VERSION: "m1nd-mcp 0.9.0-beta.2",
    },
  }
);
assert.strictEqual(hostsStatus.status, 0, hostsStatus.stderr);
const hostsStatusJson = JSON.parse(hostsStatus.stdout);
assert.strictEqual(hostsStatusJson.schema, "m1nd-host-readiness-v0");
assert.strictEqual(hostsStatusJson.hosts[0].host, "generic");
assert.strictEqual(hostsStatusJson.hosts[0].config.status, "manual");
assert.strictEqual(hostsStatusJson.hosts[0].readiness, "ready");

console.log("npm cli tests ok");
