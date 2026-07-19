#!/usr/bin/env node
"use strict";

// Rehearse the public self-update surface against the exact native release
// binary, entirely inside a disposable directory.  The pre-update runtime is a
// deliberately non-M1ND executable fixture: a minimal POSIX script on Unix and
// a copied operating-system utility on Windows. It can be restored and executed
// after rollback without fabricating a second M1ND candidate. In particular, we
// never relocate Node or a signed macOS system binary away from its loader path.

const crypto = require("crypto");
const fs = require("fs");
const os = require("os");
const path = require("path");
const { spawnSync } = require("child_process");

const REPO_ROOT = path.resolve(__dirname, "..");
const CLI = path.join(REPO_ROOT, "npm", "bin", "m1nd.js");
const STATE_SCHEMA = "m1nd-self-update-rollback-state-v0";
const RECEIPT_SCHEMA = "m1nd-release-verified-update-smoke-v2";

function fail(message) {
  throw new Error(message);
}

function trace(message) {
  if (process.env.M1ND_UPDATE_SMOKE_DEBUG === "1") {
    process.stderr.write(`[update-rollback-smoke] ${message}\n`);
  }
}

function parseArgs(argv) {
  const args = {};
  for (let index = 0; index < argv.length; index += 1) {
    const key = argv[index];
    if (!key.startsWith("--")) fail(`unexpected positional argument: ${key}`);
    const value = argv[index + 1];
    if (value === undefined || value.startsWith("--")) fail(`missing value for ${key}`);
    args[key.slice(2)] = value;
    index += 1;
  }
  for (const required of ["binary", "expected-version", "expected-commit", "target", "output"]) {
    if (!args[required]) fail(`missing --${required}`);
  }
  if (!/^[0-9a-f]{40}$/.test(args["expected-commit"])) {
    fail("--expected-commit must be a full lowercase 40-character SHA-1");
  }
  return args;
}

function sha256(file) {
  const digest = crypto.createHash("sha256");
  digest.update(fs.readFileSync(file));
  return digest.digest("hex");
}

function run(binary, args, options = {}) {
  const result = spawnSync(binary, args, {
    encoding: "utf8",
    env: options.env || process.env,
    timeout: options.timeout || 120000,
  });
  if (result.error || result.status !== 0) {
    fail(
      `${binary} ${args.join(" ")} failed: ${
        result.error ? result.error.message : (result.stderr || result.stdout || `status ${result.status}`).trim()
      }`
    );
  }
  return (result.stdout || "").trim();
}

function runtimeVersion(binary) {
  return run(binary, ["--version"], { timeout: 15000 });
}

function installPreUpdateExecutable(target) {
  if (process.platform === "win32") {
    const source = path.join(
      process.env.WINDIR || "C:\\Windows",
      "System32",
      "where.exe"
    );
    if (!fs.existsSync(source)) fail(`pre-update executable is missing: ${source}`);
    fs.copyFileSync(source, target);
    return;
  }
  fs.writeFileSync(
    target,
    "#!/bin/sh\nprintf '%s\\n' 'pre-update-runtime 0.0.0'\n"
  );
  fs.chmodSync(target, 0o755);
}

function executionObservation(binary) {
  const result = spawnSync(binary, ["--version"], {
    encoding: "utf8",
    timeout: 15000,
  });
  if (result.error) fail(`could not execute ${binary}: ${result.error.message}`);
  return {
    signal: result.signal || null,
    status: result.status,
    stderr: (result.stderr || "").trim(),
    stdout: (result.stdout || "").trim(),
  };
}

function runCli(args, env) {
  const output = run(process.execPath, [CLI, ...args, "--json"], { env });
  try {
    return JSON.parse(output);
  } catch (error) {
    fail(`CLI returned non-JSON output: ${error.message}: ${output}`);
  }
}

function assertNoInstallFailure(proof) {
  const forbidden = new Set([
    "runtime-install-failed",
    "runtime-version-mismatch-after-install",
    "runtime-cargo-install-failed",
  ]);
  const failures = (proof.blocked_actions || []).filter((entry) => forbidden.has(entry.id));
  if (failures.length > 0) fail(`update reported install failures: ${JSON.stringify(failures)}`);
}

function main() {
  const args = parseArgs(process.argv.slice(2));
  const candidate = path.resolve(args.binary);
  const output = path.resolve(args.output);
  if (!fs.existsSync(candidate) || !fs.statSync(candidate).isFile()) {
    fail(`candidate binary does not exist or is not a regular file: ${candidate}`);
  }
  const releaseDir = path.dirname(candidate);
  for (const required of ["CANDIDATE.json", "CANDIDATE.json.sigstore.json"]) {
    const file = path.join(releaseDir, required);
    if (!fs.existsSync(file) || !fs.statSync(file).isFile()) {
      fail(`verified candidate control file is missing: ${file}`);
    }
  }

  const temporary = fs.mkdtempSync(path.join(os.tmpdir(), "m1nd-update-rollback-smoke-"));
  try {
    const targetName = process.platform === "win32" ? "m1nd-mcp.exe" : "m1nd-mcp";
    const target = path.join(temporary, "managed", targetName);
    const statePath = path.join(temporary, "state", "update-state.json");
    const backupDir = path.join(temporary, "backups");
    fs.mkdirSync(path.dirname(target), { recursive: true });
    installPreUpdateExecutable(target);

    const beforeSha = sha256(target);
    const beforeExecution = executionObservation(target);
    const candidateSha = sha256(candidate);
    if (candidateSha === beforeSha) fail("candidate unexpectedly equals the pre-update executable");

    const registry = JSON.stringify({
      "dist-tags": { latest: args["expected-version"] },
      version: args["expected-version"],
    });
    const env = {
      ...process.env,
      M1ND_TEST_NPM_VIEW_JSON: registry,
      M1ND_TEST_CRATE_VERSION: args["expected-version"],
      M1ND_TEST_RELEASE_DIR: releaseDir,
      M1ND_UPDATE_BACKUP_DIR: backupDir,
      M1ND_UPDATE_STATE_PATH: statePath,
      M1ND_TEST_HOME: path.join(temporary, "home"),
    };

    trace("applying exact raw candidate through the public CLI");
    const applied = runCli(
      [
        "update",
        "apply",
        "--binary",
        target,
        "--channel",
        "latest",
        "--yes",
        "--no-npm",
        "--no-skills",
        "--no-kill",
      ],
      env
    );
    trace("public CLI apply returned");
    assertNoInstallFailure(applied);
    const installAction = (applied.applied_actions || []).find(
      (entry) => entry.id === "runtime-install-github-release"
    );
    if (!installAction || !installAction.ok) fail("exact release asset was not applied successfully");
    if (applied.requires_host_rebind !== true) {
      fail("successful runtime replacement did not require host rebind");
    }
    if (!installAction.candidate_verification) fail("update did not return candidate verification evidence");
    if (installAction.candidate_verification.verifier_source !== "trusted-fixed-path") {
      fail("hosted release smoke did not use the installed cosign verifier");
    }
    if (installAction.candidate_verification.transport_source !== "local-test-directory") {
      fail("hosted release smoke did not disclose its CI-local release transport");
    }
    if (
      !applied.test_overrides ||
      applied.test_overrides.active !== true ||
      applied.test_overrides.release_transport !== "local-test-directory" ||
      applied.test_overrides.verifier_source !== "trusted-fixed-path"
    ) {
      fail("update proof did not disclose the active CI-local transport seam");
    }

    const afterSha = sha256(target);
    const afterVersion = runtimeVersion(target);
    if (afterSha !== candidateSha) fail("managed target bytes differ from the exact candidate after update");
    if (!afterVersion.includes(args["expected-version"])) fail("updated runtime version mismatch");
    if (!afterVersion.includes(args["expected-commit"].slice(0, 7))) {
      fail("updated runtime commit mismatch");
    }

    const state = JSON.parse(fs.readFileSync(statePath, "utf8"));
    if (state.schema !== STATE_SCHEMA) fail(`unexpected rollback state schema: ${state.schema}`);
    if (state.phase !== "installed") fail(`unexpected update journal phase: ${state.phase}`);
    if (!state.backup_binary || !fs.existsSync(state.backup_binary)) fail("rollback backup is missing");
    if (sha256(state.backup_binary) !== beforeSha) fail("rollback backup bytes differ from pre-update runtime");
    if (state.backup_sha256 !== beforeSha || state.before_sha256 !== beforeSha) {
      fail("update journal did not bind the pre-update and backup digests");
    }
    if (state.candidate_sha256 !== candidateSha || state.after_sha256 !== candidateSha) {
      fail("update journal did not bind the candidate and installed digests");
    }
    if (fs.readdirSync(path.dirname(statePath)).some((name) => name.endsWith(".tmp"))) {
      fail("an atomic journal temporary file remained after update");
    }

    trace("rolling back through the public CLI");
    const rolledBack = runCli(
      ["update", "rollback", "--binary", target, "--channel", "latest"],
      env
    );
    trace("public CLI rollback returned");
    const rollbackAction = (rolledBack.applied_actions || []).find(
      (entry) => entry.id === "runtime-rollback"
    );
    if (!rollbackAction || !rollbackAction.ok) fail("runtime rollback did not complete");
    if (rolledBack.requires_host_rebind !== true) {
      fail("successful runtime restoration did not require host rebind");
    }

    const restoredSha = sha256(target);
    const restoredExecution = executionObservation(target);
    if (restoredSha !== beforeSha) fail("rollback did not restore exact pre-update bytes");
    if (JSON.stringify(restoredExecution) !== JSON.stringify(beforeExecution)) {
      fail("rollback did not restore executable behavior");
    }
    if (sha256(candidate) !== candidateSha) fail("candidate source bytes changed during rehearsal");
    const rolledBackState = JSON.parse(fs.readFileSync(statePath, "utf8"));
    if (
      rolledBackState.phase !== "rolled_back" ||
      rolledBackState.restored_sha256 !== beforeSha
    ) {
      fail("rollback journal did not atomically record the restored bytes");
    }
    if (fs.readdirSync(path.dirname(statePath)).some((name) => name.endsWith(".tmp"))) {
      fail("an atomic journal temporary file remained after rollback");
    }

    const journalAfterRollback = fs.readFileSync(statePath);
    const idempotent = runCli(
      ["update", "rollback", "--binary", target, "--channel", "latest"],
      env
    );
    if (!(idempotent.applied_actions || []).some((entry) => entry.id === "runtime-rollback" && entry.idempotent)) {
      fail("second rollback was not an idempotent no-op");
    }
    if (idempotent.requires_host_rebind !== false) {
      fail("idempotent rollback incorrectly required host rebind");
    }
    if (!fs.readFileSync(statePath).equals(journalAfterRollback)) {
      fail("idempotent rollback rewrote the journal");
    }

    const interruptedJournal = { ...rolledBackState, phase: "installed" };
    delete interruptedJournal.rolled_back_at;
    delete interruptedJournal.restored_sha256;
    delete interruptedJournal.restored_version;
    delete interruptedJournal.recovery;
    fs.writeFileSync(statePath, `${JSON.stringify(interruptedJournal, null, 2)}\n`);
    const targetBeforeCrashRecovery = fs.readFileSync(target);
    const crashRecovered = runCli(
      ["update", "rollback", "--binary", target, "--channel", "latest"],
      env
    );
    if (!(crashRecovered.applied_actions || []).some(
      (entry) => entry.recovery === "installed-target-already-before"
    )) {
      fail("rollback crash recovery did not close an installed journal whose target was already restored");
    }
    if (crashRecovered.requires_host_rebind !== false) {
      fail("journal-only rollback crash recovery incorrectly required host rebind");
    }
    if (!fs.readFileSync(target).equals(targetBeforeCrashRecovery)) {
      fail("rollback crash recovery rewrote an already-restored target");
    }
    const journalAfterCrashRecovery = fs.readFileSync(statePath);

    const restoredBytes = fs.readFileSync(target);
    const driftBytes = Buffer.from("post-update target drift fixture\n");
    fs.writeFileSync(target, driftBytes);
    if (process.platform !== "win32") fs.chmodSync(target, 0o755);
    const driftRefusal = runCli(
      ["update", "rollback", "--binary", target, "--channel", "latest"],
      env
    );
    if (!(driftRefusal.blocked_actions || []).some((entry) => entry.id === "rollback-target-digest-mismatch")) {
      fail("stale rollback did not refuse current-target digest drift");
    }
    if (!fs.readFileSync(target).equals(driftBytes)) {
      fail("stale rollback overwrote the drifted target before refusal");
    }
    if (driftRefusal.requires_host_rebind !== false) {
      fail("stale rollback refusal incorrectly required host rebind");
    }
    if (!fs.readFileSync(statePath).equals(journalAfterCrashRecovery)) {
      fail("stale rollback mutated the journal before refusal");
    }
    fs.writeFileSync(target, restoredBytes);
    if (process.platform !== "win32") fs.chmodSync(target, 0o755);

    const receipt = {
      schema: RECEIPT_SCHEMA,
      target: args.target,
      candidate_id: installAction.candidate_verification.candidate_id,
      candidate_manifest_sha256: installAction.candidate_verification.manifest_sha256,
      candidate_verification: installAction.candidate_verification,
      test_overrides: applied.test_overrides,
      candidate: {
        sha256: candidateSha,
        size_bytes: fs.statSync(candidate).size,
        version_output: afterVersion,
      },
      pre_update: {
        execution: beforeExecution,
        sha256: beforeSha,
      },
      rollback: {
        restored_execution: restoredExecution,
        restored_sha256: restoredSha,
      },
      proofs: {
        atomic_state_journal: true,
        exact_candidate_installed: true,
        expected_commit_verified: true,
        expected_version_verified: true,
        executable_after_update: true,
        backup_digest_matched_pre_update: true,
        exact_pre_update_bytes_restored: true,
        executable_after_rollback: true,
        live_installation_untouched: true,
        idempotent_rollback: true,
        rollback_crash_recovery: true,
        no_effects_on_drift_refusal: true,
        state_bound_backup_digest: true,
        state_bound_candidate_digest: true,
        target_digest_fence: true,
        verified_candidate_identity: true,
        verified_candidate_signature: true,
      },
      verdict: "PASS",
    };
    fs.mkdirSync(path.dirname(output), { recursive: true });
    fs.writeFileSync(output, `${JSON.stringify(receipt, null, 2)}\n`);
    trace(`wrote PASS receipt to ${output}`);
  } finally {
    fs.rmSync(temporary, { recursive: true, force: true });
  }
}

try {
  main();
} catch (error) {
  process.stderr.write(`update/rollback smoke refused: ${error.message}\n`);
  process.exitCode = 1;
}
