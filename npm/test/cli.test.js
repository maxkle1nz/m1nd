"use strict";

const assert = require("assert");
const crypto = require("crypto");
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
  hostRecipe,
  githubReleaseAssetName,
  githubReleaseAvailability,
  canonicalJsonV1,
  parseIntegerJson,
  domainSeparatedDigest,
  validateCanonicalCompatibility,
  validateCanonicalGateReceipt,
  verifyCanonicalReleaseVectors,
  osGateOk,
  installSkills,
  mcpConfig,
  packRoutingCheck,
  restart,
  runtimeBinaryName,
  selfUpdate: productionSelfUpdate,
  createSelfUpdateTestHarness,
  parseLaunchctlLabel,
  parseLaunchctlProgramPath,
  launchdLabelManagesTarget,
  shouldKickstartAfterInstall,
} = require("../lib/cli");

const runSelfUpdateTest = createSelfUpdateTestHarness();
function selfUpdate(args) {
  return runSelfUpdateTest(args, {
    releaseDirectory: process.env.M1ND_TEST_RELEASE_DIR || null,
    cosignPath: process.env.M1ND_TEST_COSIGN_PATH || null,
  });
}
const { classifyScopeBinding } = require("../lib/agent-cli");
const northShim = require("../bin/m1nd-north-shim");

const cli = path.resolve(__dirname, "../bin/m1nd.js");
const canonicalVectors = path.resolve(
  __dirname,
  "../../tests/fixtures/M1ND10-CANONICAL-VECTORS.json"
);

assert.deepStrictEqual(verifyCanonicalReleaseVectors(canonicalVectors), {
  ok: true,
  status: "STRUCTURALLY_VALID_NOT_CRYPTOGRAPHICALLY_VERIFIED",
});
// custody_floor must be a member of the closed ratified set; a smuggled value
// (e.g. "software") is refused at the structural gate, mirroring the Rust/Python
// closed-set validators. Mutating in place avoids re-serializing the BigInt u64s.
{
  const custodyVectors = parseIntegerJson(fs.readFileSync(canonicalVectors, "utf8"), "custody vectors");
  const receipt = custodyVectors.evidence_set.gate_receipts[0];
  assert.strictEqual(receipt.core.custody_floor, "secure-enclave-single-host-v1");
  assert.doesNotThrow(() => validateCanonicalGateReceipt(receipt));
  receipt.core.custody_floor = "software";
  assert.throws(
    () => validateCanonicalGateReceipt(receipt),
    /custody_floor .* outside the ratified/
  );
}
assert.strictEqual(
  canonicalJsonV1(parseIntegerJson('{"z":"coração","built_at":9007199254740993,"a":"α"}')),
  '{"a":"α","built_at":9007199254740993,"z":"coração"}'
);
for (const refusedNumber of ['{"n":1.0}', '{"n":1e30}']) {
  assert.throws(() => parseIntegerJson(refusedNumber), /non-integer JSON number refused/);
}
for (const ambiguousObject of ['{"a":1,"a":2}', '{"outer":{"a":1,"a":2}}']) {
  assert.throws(() => parseIntegerJson(ambiguousObject), /duplicate object key refused/);
}
assert.throws(
  () =>
    validateCanonicalCompatibility({
      schema: "m1nd-release-compatibility-manifest-v1",
      version: "1.4.0",
      commit: "a".repeat(40),
      source_ref: "refs/tags/v1.4.0",
      targets: [
        {
          target: "linux-x86_64",
          asset: "m1nd-mcp-wrong",
          sha256: "a".repeat(64),
          size_bytes: 1,
        },
      ],
    }),
  /asset .* does not match/
);

// Version fixtures track the real package version so the self-update tests keep
// asserting current==package and stale<package as package.json advances.
const CURRENT_VERSION = JSON.parse(
  fs.readFileSync(path.resolve(__dirname, "../../package.json"), "utf8")
).version;
function versionBelow(version) {
  const match = String(version).match(/^(\d+)\.(\d+)\.(\d+)/);
  if (!match) return "0.0.0";
  const major = Number(match[1]);
  const minor = Number(match[2]);
  const patch = Number(match[3]);
  if (patch > 0) return `${major}.${minor}.${patch - 1}`;
  if (minor > 0) return `${major}.${minor - 1}.0`;
  if (major > 0) return `${major - 1}.0.0`;
  return "0.0.0";
}
const STALE_VERSION = versionBelow(CURRENT_VERSION);

assert.strictEqual(runtimeBinaryName("win32"), "m1nd-mcp.exe");
assert.strictEqual(runtimeBinaryName("darwin"), "m1nd-mcp");
assert.strictEqual(runtimeBinaryName("linux"), "m1nd-mcp");
assert.strictEqual(githubReleaseAssetName("linux", "x64"), "m1nd-mcp-linux-x86_64");
assert.strictEqual(githubReleaseAssetName("darwin", "x64"), "m1nd-mcp-macos-x86_64");
assert.strictEqual(githubReleaseAssetName("darwin", "arm64"), "m1nd-mcp-macos-aarch64");
assert.strictEqual(
  githubReleaseAssetName("win32", "x64"),
  "m1nd-mcp-windows-x86_64.exe"
);
assert.strictEqual(githubReleaseAssetName("win32", "arm64"), null);
{
  // Windows is phase-2: the release ships no Windows binary, so runtime
  // acquisition on win32 must refuse with a clear message instead of a later
  // ENOENT on a m1nd-mcp.exe that was never downloaded. Clear the availability
  // override so the guard, not a test fixture, decides — and prove it returns
  // before any network probe.
  const savedAvailable = process.env.M1ND_TEST_GITHUB_RELEASE_AVAILABLE;
  delete process.env.M1ND_TEST_GITHUB_RELEASE_AVAILABLE;
  const windowsAvailability = githubReleaseAvailability("1.5.0", "win32", "x64");
  assert.strictEqual(windowsAvailability.ok, false);
  assert.strictEqual(windowsAvailability.available, false);
  assert.strictEqual(windowsAvailability.source, "windows-phase-2");
  assert.strictEqual(
    windowsAvailability.error,
    "m1nd 1.5.0 does not ship a Windows binary; Windows support is phase-2"
  );
  if (savedAvailable === undefined) {
    delete process.env.M1ND_TEST_GITHUB_RELEASE_AVAILABLE;
  } else {
    process.env.M1ND_TEST_GITHUB_RELEASE_AVAILABLE = savedAvailable;
  }
}
assert.strictEqual(
  commandLooksLikeRuntime("/Us" + "ers/alice/.m1nd/bin/" + runtimeBinaryName() + " --stdio"),
  true
);
assert.strictEqual(commandLooksLikeRuntime("(" + runtimeBinaryName() + ")"), true);
assert.strictEqual(commandLooksLikeRuntime("node codex prompt mentions m1nd-mcp"), false);

assert.strictEqual(
  defaultRuntimePath("win32", "C:\\Users\\<name>"),
  "C:\\Users\\<name>\\.m1nd\\bin\\m1nd-mcp.exe"
);

const codexWindowsConfig = mcpConfig(
  "codex",
  "C:\\Users\\<name>\\.m1nd\\bin\\m1nd-mcp.exe"
);
assert(codexWindowsConfig.includes('command = "C:\\\\Users\\\\<name>\\\\.m1nd\\\\bin\\\\m1nd-mcp.exe"'));
assert(codexWindowsConfig.includes('args = ["--stdio", "--no-gui"]'));
const projectForConfig = path.resolve("project");
const codexProjectConfig = mcpConfig(
  "codex",
  "C:\\Users\\<name>\\.m1nd\\bin\\m1nd-mcp.exe",
  projectForConfig
);
assert(codexProjectConfig.includes("[mcp_servers.m1nd.env]"));
assert(codexProjectConfig.includes(`M1ND_WORKSPACE_ROOT = "${projectForConfig.replace(/\\/g, "\\\\")}"`));

const genericWindowsConfig = JSON.parse(
  mcpConfig("generic", "C:\\Users\\<name>\\.m1nd\\bin\\m1nd-mcp.exe")
);
assert.strictEqual(
  genericWindowsConfig.mcpServers.m1nd.command,
  "C:\\Users\\<name>\\.m1nd\\bin\\m1nd-mcp.exe"
);
assert.deepStrictEqual(genericWindowsConfig.mcpServers.m1nd.args, ["--stdio", "--no-gui"]);
const genericProjectConfig = JSON.parse(
  mcpConfig("generic", "C:\\Users\\<name>\\.m1nd\\bin\\m1nd-mcp.exe", projectForConfig)
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
assert(help.stdout.includes("m1nd agent first-minute"));
assert(help.stdout.includes("m1nd agent auto"));
assert(help.stdout.includes("m1nd agent next"));
assert(help.stdout.includes("m1nd pack-routing-check"));
assert(help.stdout.includes("RETROBUILDER capability_suggestions"));

// Cold-start bug 1: `m1nd --version` (a stranger's most common first command) must
// print the package version and exit 0 — not "missing value for --version". The bare
// `version` subcommand and the conventional `-V` short flag behave identically.
for (const versionArgs of [["--version"], ["-V"], ["version"]]) {
  const versionRun = spawnSync(process.execPath, [cli, ...versionArgs], { encoding: "utf8" });
  assert.strictEqual(versionRun.status, 0, `${versionArgs.join(" ")} exit: ${versionRun.stderr}`);
  assert.strictEqual(
    versionRun.stdout.trim(),
    CURRENT_VERSION,
    `${versionArgs.join(" ")} should print the package version`
  );
}

const packCheck = spawnSync(process.execPath, [cli, "pack-check", "--json"], { encoding: "utf8" });
assert.strictEqual(packCheck.status, 0, packCheck.stderr);
assert.strictEqual(JSON.parse(packCheck.stdout).schema, "m1nd-agent-pack-check-v0");

const packRouting = spawnSync(process.execPath, [cli, "pack-routing-check", "--json"], { encoding: "utf8" });
assert.strictEqual(packRouting.status, 0, packRouting.stderr);
const packRoutingJson = JSON.parse(packRouting.stdout);
assert.strictEqual(packRoutingJson.schema, "m1nd-agent-pack-routing-check-v0");
assert.strictEqual(packRoutingJson.ok, true);
assert(packRoutingJson.contract_checks.some((check) => check.id === "direct-proof-is-final-truth" && check.ok));
assert(packRoutingJson.files.some((file) => file.id === "m1nd-guardian" && file.ok));
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

function sha256Text(content) {
  return crypto.createHash("sha256").update(content).digest("hex");
}

function canonicalJson(value) {
  function order(candidate) {
    if (Array.isArray(candidate)) return candidate.map(order);
    if (candidate && typeof candidate === "object") {
      return Object.keys(candidate)
        .sort()
        .reduce((result, key) => {
          result[key] = order(candidate[key]);
          return result;
        }, {});
    }
    return candidate;
  }
  return `${JSON.stringify(order(value))}\n`;
}

function currentReleaseTarget() {
  if (process.platform === "darwin" && process.arch === "arm64") return "macos-aarch64";
  if (process.platform === "darwin" && process.arch === "x64") return "macos-x86_64";
  if (process.platform === "linux" && process.arch === "x64") return "linux-x86_64";
  if (process.platform === "win32" && process.arch === "x64") return "windows-x86_64";
  throw new Error(`unmapped test platform ${process.platform}-${process.arch}`);
}

function writeVersionBinary(file, version, marker = "fixture") {
  writeFakeBinary(
    file,
    `#!/bin/sh\nprintf '%s\\n' 'm1nd-mcp ${version} (${marker})'\n`
  );
}

function writeFakeCosign(file) {
  writeFakeBinary(
    file,
    `#!/usr/bin/env node
"use strict";
const crypto = require("crypto");
const fs = require("fs");
const args = process.argv.slice(2);
function value(name) {
  const index = args.indexOf(name);
  if (index < 0 || index + 1 >= args.length) process.exit(21);
  return args[index + 1];
}
if (args[0] !== "verify-blob") process.exit(22);
const subject = args[args.length - 1];
const bundle = JSON.parse(fs.readFileSync(value("--bundle"), "utf8"));
const digest = crypto.createHash("sha256").update(fs.readFileSync(subject)).digest("hex");
if (bundle.subject_sha256 !== digest) process.exit(23);
if (bundle.certificate_identity !== value("--certificate-identity")) process.exit(24);
if (bundle.certificate_oidc_issuer !== value("--certificate-oidc-issuer")) process.exit(25);
process.stdout.write("Verified OK\\n");
`
  );
}

function writeVerifiedReleaseFixture(root, rawSource, options = {}) {
  const releaseDir = path.join(root, "release");
  fs.mkdirSync(releaseDir, { recursive: true });
  const version = options.version || CURRENT_VERSION;
  const target = options.target || currentReleaseTarget();
  const asset = githubReleaseAssetName();
  const raw = path.join(releaseDir, asset);
  fs.copyFileSync(rawSource, raw);
  if (process.platform !== "win32") fs.chmodSync(raw, 0o755);
  const rawSha256 = sha256Text(fs.readFileSync(raw));
  const rawSize = fs.statSync(raw).size;
  const artifacts = [
    {
      kind: "runtime_binary",
      name: asset,
      sha256: rawSha256,
      size_bytes: rawSize,
      target,
    },
  ];
  const runtimeBindings = [
    {
      archive: `m1nd-mcp-${target}.tar.gz`,
      archive_member: process.platform === "win32" ? "m1nd-mcp.exe" : "m1nd-mcp",
      artifact_smoke_receipt: `GATE-ARTIFACT-SMOKE-${target}.json`,
      raw_binary: asset,
      runtime_sha256: rawSha256,
      size_bytes: rawSize,
      target,
    },
  ];
  const seed = {
    artifacts,
    commit: options.commit || "a".repeat(40),
    runtime_bindings: runtimeBindings,
    source_ref: options.sourceRef || `refs/tags/v${version}`,
    version,
  };
  const manifest = {
    schema: options.schema || "m1nd-release-candidate-v1",
    candidate_id: `sha256:${sha256Text(canonicalJson(seed))}`,
    ...seed,
    build_policy: {
      builds_per_target: 1,
      archive_raw_digest_match: true,
      promotion: "exact_declared_bytes_only",
      raw_asset_install: true,
      targets: options.policyTargets || [target],
    },
  };
  if (options.mutateManifest) options.mutateManifest(manifest);
  const manifestPath = path.join(releaseDir, "CANDIDATE.json");
  fs.writeFileSync(manifestPath, canonicalJson(manifest));
  const identity =
    options.identity ||
    `https://github.com/maxkle1nz/m1nd/.github/workflows/release.yml@refs/tags/v${version}`;
  const issuer = options.issuer || "https://token.actions.githubusercontent.com";
  fs.writeFileSync(
    path.join(releaseDir, "CANDIDATE.json.sigstore.json"),
    `${JSON.stringify({
      certificate_identity: identity,
      certificate_oidc_issuer: issuer,
      subject_sha256: sha256Text(fs.readFileSync(manifestPath)),
    })}\n`
  );
  const cosign = path.join(root, "fake-cosign");
  writeFakeCosign(cosign);
  return { asset, cosign, manifest, manifestPath, raw, releaseDir, target };
}

function writeCanonicalVerifiedReleaseFixture(root, rawSource, options = {}) {
  const releaseDir = path.join(root, "canonical-release");
  fs.mkdirSync(releaseDir, { recursive: true });
  const version = options.version || CURRENT_VERSION;
  const target = options.target || currentReleaseTarget();
  const asset = githubReleaseAssetName();
  const raw = path.join(releaseDir, asset);
  fs.copyFileSync(rawSource, raw);
  if (process.platform !== "win32") fs.chmodSync(raw, 0o755);
  const rawSha256 = sha256Text(fs.readFileSync(raw));
  const rawSize = fs.statSync(raw).size;
  const commit = options.commit || "b".repeat(40);
  const compatibility = {
    schema: "m1nd-release-compatibility-manifest-v1",
    version,
    commit,
    source_ref: `refs/tags/v${version}`,
    targets: [{ target, asset, sha256: rawSha256, size_bytes: rawSize }],
  };
  const compatibilityPath = path.join(releaseDir, "RELEASE-COMPATIBILITY.json");
  fs.writeFileSync(compatibilityPath, canonicalJsonV1(compatibility));
  const compatibilityDigest = sha256Text(fs.readFileSync(compatibilityPath));
  const rollbackDigest = sha256Text("fixture-canonical-rollback");
  const digest = (label) => sha256Text(`fixture:${label}`);
  const core = {
    repo_commits: { m1nd: commit },
    artifact_digests: {
      release_compatibility_manifest_v1: compatibilityDigest,
      release_rollback_plan_v1: rollbackDigest,
      [`release_asset:${asset}`]: rawSha256,
    },
    schema_policy_versions: { action_catalog: "v1" },
    tool_catalog_digest: digest("tool-catalog"),
    safety_kernel_digest: digest("safety-kernel"),
    previous_governance_runtime_digest: digest("previous-runtime"),
    constitution_epoch_digest: digest("constitution"),
    autonomy_epoch_grants_digest: digest("autonomy-grants"),
    independence_quorum_policy_digest: digest("quorum"),
    intended_active_mode: "FULL_AUTONOMY",
    compatibility_manifest_digest: compatibilityDigest,
    rollback_plan_digest: rollbackDigest,
    harness_fixture_threat_digests: { threat_matrix: digest("threat") },
    build_environment_digest: digest("environment"),
    built_at: 9007199254740993n,
  };
  const manifest = {
    schema: "m1nd-release-candidate-manifest-v1",
    core,
    candidate_digest: domainSeparatedDigest("m1nd-release-candidate-manifest-v1", core),
    provenance_signature: "NOT_CRYPTOGRAPHIC:fixture-candidate",
  };
  const manifestPath = path.join(releaseDir, "CANDIDATE.json");
  fs.writeFileSync(manifestPath, canonicalJsonV1(manifest));
  const identity =
    options.identity ||
    `https://github.com/maxkle1nz/m1nd/.github/workflows/release.yml@refs/tags/v${version}`;
  const issuer = options.issuer || "https://token.actions.githubusercontent.com";
  fs.writeFileSync(
    path.join(releaseDir, "CANDIDATE.json.sigstore.json"),
    `${JSON.stringify({
      certificate_identity: identity,
      certificate_oidc_issuer: issuer,
      subject_sha256: sha256Text(fs.readFileSync(manifestPath)),
    })}\n`
  );
  const cosign = path.join(root, "fake-cosign-canonical");
  writeFakeCosign(cosign);
  return {
    asset,
    candidateDigest: manifest.candidate_digest,
    compatibilityPath,
    cosign,
    manifest,
    manifestPath,
    raw,
    releaseDir,
    target,
  };
}

function rewriteSignedFixtureManifest(fixture, mutate) {
  const manifest = JSON.parse(fs.readFileSync(fixture.manifestPath, "utf8"));
  mutate(manifest);
  fs.writeFileSync(fixture.manifestPath, canonicalJson(manifest));
  const bundlePath = path.join(fixture.releaseDir, "CANDIDATE.json.sigstore.json");
  const bundle = JSON.parse(fs.readFileSync(bundlePath, "utf8"));
  bundle.subject_sha256 = sha256Text(fs.readFileSync(fixture.manifestPath));
  fs.writeFileSync(bundlePath, `${JSON.stringify(bundle)}\n`);
}

function writeFakeMcpRuntime(file) {
  fs.mkdirSync(path.dirname(file), { recursive: true });
  fs.writeFileSync(
    file,
    `#!/usr/bin/env node
if (process.argv.includes("--version")) {
  console.log("m1nd-mcp ${CURRENT_VERSION}");
  process.exit(0);
}
const readline = require("readline");
const fs = require("fs");
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
  if (req.method === "tools/list") {
    const names = ["trust_selftest", "session_handshake", "ingest", "search", "seek", "activate", "audit", "glob", "surgical_context_v2"];
    if (process.env.M1ND_FAKE_LEGACY_NO_TRUST_SELFTEST === "1") names.splice(names.indexOf("trust_selftest"), 1);
    return write(req.id, { tools: names.map((name) => ({ name })) });
  }
  if (req.method !== "tools/call") return write(req.id, {});
  const name = req.params.name;
  if (process.env.M1ND_FAKE_CALL_LOG) fs.appendFileSync(process.env.M1ND_FAKE_CALL_LOG, name + "\\n");
  const args = req.params.arguments || {};
  const orientBlocked = process.env.M1ND_FAKE_ORIENT_BLOCKED === "1" || process.env.M1ND_FAKE_SEARCH_BLOCKED === "1";
  if (name === "trust_selftest") {
    if (process.env.M1ND_FAKE_TRUST === "needs_ingest") {
      return write(req.id, tool({ schema: "m1nd-trust-selftest-v0", verdict: "needs_ingest", checks: { needs_ingest: true }, graph_state: { ...graph(), node_count: 0, edge_count: 0, finalized: false, ingest_root_count: 0 } }));
    }
    return write(req.id, tool({ schema: "m1nd-trust-selftest-v0", verdict: "full_trust", checks: { needs_ingest: false }, graph_state: graph() }));
  }
  if (name === "ingest") return write(req.id, tool({ schema: "m1nd-ingest-v0", ok: true, graph_state: graph(), path: args.path }));
  if (name === "session_handshake") {
    if (process.env.M1ND_FAKE_TRUST === "needs_ingest") {
      return write(req.id, tool({ schema: "m1nd-session-handshake-v0", trust_mode: "needs_ingest", graph_state: { ...graph(), node_count: 0, edge_count: 0, finalized: false, ingest_root_count: 0 }, scope: args.scope }));
    }
    return write(req.id, tool({ schema: "m1nd-session-handshake-v0", trust_mode: "full_trust", graph_state: graph(), scope: args.scope }));
  }
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
  "dist-tags": { beta: CURRENT_VERSION, latest: CURRENT_VERSION },
  version: CURRENT_VERSION,
});

const fakeEnvBase = {
  M1ND_TEST_NPM_VIEW_JSON: registryCurrent,
  M1ND_TEST_CRATE_VERSION: CURRENT_VERSION,
  M1ND_TEST_GITHUB_RELEASE_AVAILABLE: "true",
};

// Security regression: ambient test transport/verifier variables are never a
// production authority.  Refusal happens before fixture reads, verifier
// execution, target replacement, or rollback-journal creation.
{
  const tmp = mkTmpDir();
  const target = path.join(tmp, runtimeBinaryName());
  const source = path.join(tmp, "candidate-runtime");
  const marker = path.join(tmp, "ambient-fake-cosign-executed");
  const statePath = path.join(tmp, "update-state.json");
  writeVersionBinary(target, STALE_VERSION, "old");
  writeVersionBinary(source, CURRENT_VERSION, "new");
  const fixture = writeVerifiedReleaseFixture(tmp, source);
  writeFakeBinary(
    fixture.cosign,
    `#!/usr/bin/env node\nrequire("fs").writeFileSync(${JSON.stringify(marker)}, "executed");\n`
  );
  const before = fs.readFileSync(target);
  assert.throws(
    () =>
      withEnv(
        {
          ...fakeEnvBase,
          M1ND_TEST_RELEASE_DIR: fixture.releaseDir,
          M1ND_TEST_COSIGN_PATH: fixture.cosign,
          M1ND_TEST_RUNTIME_VERSION: `m1nd-mcp ${STALE_VERSION}`,
          M1ND_UPDATE_STATE_PATH: statePath,
        },
        () =>
          productionSelfUpdate({
            _: ["update", "apply"],
            binary: target,
            channel: "latest",
            yes: true,
            "no-npm": true,
            "no-skills": true,
            "no-kill": true,
          })
      ),
    /unsafe self-update test overrides/
  );
  assert.deepStrictEqual(fs.readFileSync(target), before);
  assert.strictEqual(fs.existsSync(marker), false);
  assert.strictEqual(fs.existsSync(statePath), false);
}

// A repository-controlled PATH must never become updater authority. Planning
// with every historically used updater executable shadowed performs no child
// execution and refuses the unsigned runtime fallback.
if (process.platform !== "win32") {
  const tmp = mkTmpDir();
  const marker = path.join(tmp, "hostile-path-tool-executed");
  for (const name of ["npm", "cargo", "curl", "cosign", "node"]) {
    writeFakeBinary(
      path.join(tmp, name),
      `#!/bin/sh\nprintf '%s' executed > ${JSON.stringify(marker)}\nexit 0\n`
    );
  }
  const proof = withEnv(
    {
      ...fakeEnvBase,
      PATH: tmp,
      M1ND_TEST_GITHUB_RELEASE_AVAILABLE: "false",
      M1ND_TEST_RUNTIME_VERSION: undefined,
    },
    () =>
      productionSelfUpdate({
        _: ["update", "check"],
        binary: path.join(tmp, "missing-runtime"),
        channel: "latest",
        "no-npm": true,
        "no-skills": true,
        "no-kill": true,
      })
  );
  assert.strictEqual(fs.existsSync(marker), false);
  assert(proof.blocked_actions.some((entry) => entry.id === "runtime-release-unavailable"));
  assert(!proof.planned_actions.some((entry) => entry.id === "runtime-install-cargo"));
}

function refusedVerifiedReleaseCase(configure) {
  const tmp = mkTmpDir();
  const target = path.join(tmp, runtimeBinaryName());
  const source = path.join(tmp, "candidate-runtime");
  const statePath = path.join(tmp, "update-state.json");
  const backupDir = path.join(tmp, "backups");
  writeVersionBinary(target, STALE_VERSION, "old");
  writeVersionBinary(source, CURRENT_VERSION, "new");
  const fixture = writeVerifiedReleaseFixture(tmp, source);
  const env = {
    ...fakeEnvBase,
    M1ND_TEST_RELEASE_DIR: fixture.releaseDir,
    M1ND_TEST_COSIGN_PATH: fixture.cosign,
    M1ND_TEST_RUNTIME_VERSION: undefined,
    M1ND_UPDATE_BACKUP_DIR: backupDir,
    M1ND_UPDATE_STATE_PATH: statePath,
  };
  configure({ env, fixture, source, statePath, target, tmp });
  const before = fs.readFileSync(target);
  const proof = withEnv(env, () =>
    selfUpdate({
      _: ["update", "apply"],
      binary: target,
      channel: "latest",
      yes: true,
      "no-npm": true,
      "no-skills": true,
      "no-kill": true,
    })
  );
  assert(proof.blocked_actions.some((entry) => entry.id === "runtime-install-failed"));
  assert.deepStrictEqual(fs.readFileSync(target), before);
  assert.strictEqual(fs.existsSync(statePath), false);
  assert.strictEqual(fs.existsSync(backupDir), false);
  assert.strictEqual(proof.test_overrides.active, true);
  assert.strictEqual(proof.requires_host_rebind, false);
  return proof;
}

function rollbackJournalFixture(phase, targetBytes, overrides = {}) {
  const tmp = mkTmpDir();
  const target = path.join(tmp, runtimeBinaryName());
  const backup = path.join(tmp, "backups", "runtime-before");
  const statePath = path.join(tmp, "update-state.json");
  const beforeBytes = Buffer.from("before-runtime\n");
  const candidateBytes = Buffer.from("candidate-runtime\n");
  writeFakeBinary(target, targetBytes);
  writeFakeBinary(backup, beforeBytes);
  const state = {
    schema: "m1nd-self-update-rollback-state-v0",
    created_at: new Date().toISOString(),
    phase,
    install_kind: "verified-github-release",
    rollback_available: true,
    target_binary: target,
    backup_binary: backup,
    backup_sha256: sha256Text(beforeBytes),
    before_version: `m1nd-mcp ${STALE_VERSION}`,
    before_sha256: sha256Text(beforeBytes),
    candidate_sha256: sha256Text(candidateBytes),
    after_version: `m1nd-mcp ${CURRENT_VERSION}`,
    after_sha256: phase === "prepared" ? null : sha256Text(candidateBytes),
    ...overrides,
  };
  fs.writeFileSync(statePath, `${JSON.stringify(state, null, 2)}\n`);
  const env = {
    ...fakeEnvBase,
    M1ND_TEST_RUNTIME_VERSION: `m1nd-mcp ${CURRENT_VERSION}`,
    M1ND_UPDATE_STATE_PATH: statePath,
  };
  const rollback = () =>
    withEnv(env, () => selfUpdate({ _: ["update", "rollback"], binary: target, channel: "latest" }));
  return { backup, beforeBytes, candidateBytes, env, rollback, state, statePath, target, tmp };
}

// The fake cosign fixture is a shebang script; Windows cannot execute it, so
// the apply/rollback harness scenarios below run on POSIX CI only. Real
// Windows updater verification belongs to hosted G8 with genuine cosign.
if (process.platform !== "win32") {

withEnv(
  {
    ...fakeEnvBase,
    M1ND_TEST_RUNTIME_VERSION: `m1nd-mcp ${CURRENT_VERSION}`,
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

refusedVerifiedReleaseCase(({ fixture }) => {
  fs.appendFileSync(fixture.raw, "tampered raw bytes\n");
});

refusedVerifiedReleaseCase(({ fixture }) => {
  fs.appendFileSync(fixture.manifestPath, " \n");
});

refusedVerifiedReleaseCase(({ fixture }) => {
  const bundlePath = path.join(fixture.releaseDir, "CANDIDATE.json.sigstore.json");
  const bundle = JSON.parse(fs.readFileSync(bundlePath, "utf8"));
  bundle.certificate_identity = "https://github.com/example/other/.github/workflows/release.yml@refs/tags/v1.4.0";
  fs.writeFileSync(bundlePath, `${JSON.stringify(bundle)}\n`);
});

refusedVerifiedReleaseCase(({ fixture }) => {
  const bundlePath = path.join(fixture.releaseDir, "CANDIDATE.json.sigstore.json");
  const bundle = JSON.parse(fs.readFileSync(bundlePath, "utf8"));
  bundle.certificate_oidc_issuer = "https://issuer.example.invalid";
  fs.writeFileSync(bundlePath, `${JSON.stringify(bundle)}\n`);
});

refusedVerifiedReleaseCase(({ fixture }) => {
  fs.rmSync(path.join(fixture.releaseDir, "CANDIDATE.json.sigstore.json"));
});

refusedVerifiedReleaseCase(({ fixture }) => {
  fs.rmSync(fixture.manifestPath);
});

refusedVerifiedReleaseCase(({ fixture }) => {
  fs.truncateSync(fixture.manifestPath, 16 * 1024 * 1024 + 1);
});

if (process.platform !== "win32") {
  refusedVerifiedReleaseCase(({ fixture, tmp }) => {
    const outside = path.join(tmp, "outside-release-runtime");
    fs.copyFileSync(fixture.raw, outside);
    fs.rmSync(fixture.raw);
    fs.symlinkSync(outside, fixture.raw);
  });
}

refusedVerifiedReleaseCase(({ env, tmp }) => {
  env.M1ND_TEST_COSIGN_PATH = path.join(tmp, "missing-cosign");
});

refusedVerifiedReleaseCase(({ fixture }) => {
  rewriteSignedFixtureManifest(fixture, (manifest) => {
    manifest.version = STALE_VERSION;
    manifest.source_ref = `refs/tags/v${STALE_VERSION}`;
  });
});

refusedVerifiedReleaseCase(({ fixture }) => {
  rewriteSignedFixtureManifest(fixture, (manifest) => {
    manifest.source_ref = "refs/heads/main";
  });
});

refusedVerifiedReleaseCase(({ fixture }) => {
  rewriteSignedFixtureManifest(fixture, (manifest) => {
    manifest.schema = "m1nd-release-candidate-future";
  });
});

refusedVerifiedReleaseCase(({ fixture }) => {
  rewriteSignedFixtureManifest(fixture, (manifest) => {
    const binding = manifest.runtime_bindings.find((entry) => entry.target === fixture.target);
    binding.raw_binary = "m1nd-mcp-wrong-asset";
  });
});

refusedVerifiedReleaseCase(({ fixture }) => {
  rewriteSignedFixtureManifest(fixture, (manifest) => {
    manifest.build_policy.targets = ["wrong-platform-x86_64"];
  });
});

for (const phase of ["prepared", "installed", "rolled_back"]) {
  const drift = rollbackJournalFixture(phase, Buffer.from("drifted-runtime\n"));
  const targetBefore = fs.readFileSync(drift.target);
  const journalBefore = fs.readFileSync(drift.statePath);
  const backupBefore = fs.readFileSync(drift.backup);
  const refused = drift.rollback();
  assert(refused.blocked_actions.some((entry) => entry.id === "rollback-target-digest-mismatch"));
  assert.strictEqual(refused.requires_host_rebind, false);
  assert.deepStrictEqual(fs.readFileSync(drift.target), targetBefore);
  assert.deepStrictEqual(fs.readFileSync(drift.statePath), journalBefore);
  assert.deepStrictEqual(fs.readFileSync(drift.backup), backupBefore);
}

{
  const unknown = rollbackJournalFixture("future_phase", Buffer.from("candidate-runtime\n"));
  const targetBefore = fs.readFileSync(unknown.target);
  const journalBefore = fs.readFileSync(unknown.statePath);
  const refused = unknown.rollback();
  assert(refused.blocked_actions.some((entry) => entry.id === "rollback-state-phase-invalid"));
  assert.strictEqual(refused.requires_host_rebind, false);
  assert.deepStrictEqual(fs.readFileSync(unknown.target), targetBefore);
  assert.deepStrictEqual(fs.readFileSync(unknown.statePath), journalBefore);
}

{
  const legacy = rollbackJournalFixture("installed", Buffer.from("candidate-runtime\n"), {
    phase: undefined,
  });
  const targetBefore = fs.readFileSync(legacy.target);
  const journalBefore = fs.readFileSync(legacy.statePath);
  const backupBefore = fs.readFileSync(legacy.backup);
  const refused = legacy.rollback();
  assert(refused.blocked_actions.some((entry) => entry.id === "rollback-state-phase-invalid"));
  assert.strictEqual(refused.requires_host_rebind, false);
  assert.deepStrictEqual(fs.readFileSync(legacy.target), targetBefore);
  assert.deepStrictEqual(fs.readFileSync(legacy.statePath), journalBefore);
  assert.deepStrictEqual(fs.readFileSync(legacy.backup), backupBefore);
}

{
  const prepared = rollbackJournalFixture("prepared", Buffer.from("before-runtime\n"));
  const targetBefore = fs.readFileSync(prepared.target);
  const backupBefore = fs.readFileSync(prepared.backup);
  const recovered = prepared.rollback();
  assert(recovered.applied_actions.some((entry) => entry.recovery === "prepared-target-still-before"));
  assert.strictEqual(recovered.requires_host_rebind, false);
  assert.deepStrictEqual(fs.readFileSync(prepared.target), targetBefore);
  assert.deepStrictEqual(fs.readFileSync(prepared.backup), backupBefore);
  assert.strictEqual(JSON.parse(fs.readFileSync(prepared.statePath)).phase, "rolled_back");
}

{
  const prepared = rollbackJournalFixture("prepared", Buffer.from("candidate-runtime\n"));
  const recovered = prepared.rollback();
  assert(recovered.applied_actions.some((entry) => entry.id === "runtime-rollback" && entry.ok));
  assert.strictEqual(recovered.requires_host_rebind, true);
  assert.deepStrictEqual(fs.readFileSync(prepared.target), prepared.beforeBytes);
  const state = JSON.parse(fs.readFileSync(prepared.statePath));
  assert.strictEqual(state.phase, "rolled_back");
  assert.strictEqual(state.recovery, "prepared-target-was-candidate");
}

{
  const installed = rollbackJournalFixture("installed", Buffer.from("before-runtime\n"));
  const targetBefore = fs.readFileSync(installed.target);
  const backupBefore = fs.readFileSync(installed.backup);
  const recovered = installed.rollback();
  assert(recovered.applied_actions.some((entry) => entry.recovery === "installed-target-already-before"));
  assert.strictEqual(recovered.requires_host_rebind, false);
  assert.deepStrictEqual(fs.readFileSync(installed.target), targetBefore);
  assert.deepStrictEqual(fs.readFileSync(installed.backup), backupBefore);
  const state = JSON.parse(fs.readFileSync(installed.statePath));
  assert.strictEqual(state.phase, "rolled_back");
  assert.strictEqual(state.recovery, "installed-target-already-before");
}

{
  const firstInstall = rollbackJournalFixture("installed", Buffer.from("candidate-runtime\n"), {
    backup_binary: null,
    backup_sha256: null,
    before_sha256: null,
  });
  fs.rmSync(firstInstall.target);
  const recovered = firstInstall.rollback();
  assert(recovered.applied_actions.some((entry) => entry.recovery === "installed-target-already-before"));
  assert.strictEqual(recovered.requires_host_rebind, false);
  assert.strictEqual(fs.existsSync(firstInstall.target), false);
  assert.strictEqual(JSON.parse(fs.readFileSync(firstInstall.statePath)).phase, "rolled_back");
}

{
  const firstInstall = rollbackJournalFixture("installed", Buffer.from("candidate-runtime\n"), {
    backup_binary: null,
    backup_sha256: null,
    before_sha256: null,
  });
  const restored = firstInstall.rollback();
  assert(restored.applied_actions.some((entry) => entry.id === "runtime-rollback" && entry.ok));
  assert.strictEqual(restored.requires_host_rebind, true);
  assert.strictEqual(fs.existsSync(firstInstall.target), false);
  assert.strictEqual(JSON.parse(fs.readFileSync(firstInstall.statePath)).restored_sha256, null);
  const second = firstInstall.rollback();
  assert(second.applied_actions.some((entry) => entry.idempotent));
  assert.strictEqual(second.requires_host_rebind, false);
  assert.strictEqual(fs.existsSync(firstInstall.target), false);
}

{
  const cargo = rollbackJournalFixture("installed", Buffer.from("cargo-runtime\n"), {
    install_kind: "cargo-fallback-unverified",
    rollback_available: false,
    backup_binary: null,
    backup_sha256: null,
    candidate_sha256: null,
    after_sha256: sha256Text(Buffer.from("cargo-runtime\n")),
  });
  const targetBefore = fs.readFileSync(cargo.target);
  const journalBefore = fs.readFileSync(cargo.statePath);
  const refused = cargo.rollback();
  assert(refused.blocked_actions.some((entry) => entry.id === "rollback-unavailable-cargo-fallback"));
  assert.strictEqual(refused.requires_host_rebind, false);
  assert.deepStrictEqual(fs.readFileSync(cargo.target), targetBefore);
  assert.deepStrictEqual(fs.readFileSync(cargo.statePath), journalBefore);
}

withEnv(
  {
    ...fakeEnvBase,
    M1ND_TEST_RUNTIME_VERSION: `m1nd-mcp ${STALE_VERSION}`,
  },
  () => {
    const tmp = mkTmpDir();
    const target = path.join(tmp, runtimeBinaryName());
    const release = path.join(tmp, "release-m1nd-mcp");
    const statePath = path.join(tmp, "update-state.json");
    writeVersionBinary(target, STALE_VERSION, "old");
    writeVersionBinary(release, CURRENT_VERSION, "new");
    const verifiedRelease = writeVerifiedReleaseFixture(tmp, release);

    const env = {
      M1ND_TEST_RELEASE_DIR: verifiedRelease.releaseDir,
      M1ND_TEST_COSIGN_PATH: verifiedRelease.cosign,
      M1ND_TEST_RUNTIME_VERSION: undefined,
      M1ND_UPDATE_BACKUP_DIR: path.join(tmp, "backups"),
      M1ND_UPDATE_STATE_PATH: statePath,
    };
    withEnv(env, () =>
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
    const state = JSON.parse(fs.readFileSync(statePath, "utf8"));
    fs.writeFileSync(state.backup_binary, "tampered backup\n");

    const refused = withEnv(env, () =>
      selfUpdate({
        _: ["update", "rollback"],
        binary: target,
        channel: "beta",
      })
    );
    assert(
      refused.blocked_actions.some(
        (entry) => entry.id === "rollback-backup-digest-mismatch"
      )
    );
    assert.strictEqual(sha256Text(fs.readFileSync(target)), sha256Text(fs.readFileSync(release)));
  }
);

withEnv(
  {
    ...fakeEnvBase,
    M1ND_TEST_RUNTIME_VERSION: `m1nd-mcp ${CURRENT_VERSION}`,
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
    M1ND_TEST_RUNTIME_VERSION: `m1nd-mcp ${STALE_VERSION}`,
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
    M1ND_TEST_RUNTIME_VERSION: `m1nd-mcp ${STALE_VERSION}`,
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
    M1ND_TEST_RUNTIME_VERSION: `m1nd-mcp ${STALE_VERSION}`,
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
    M1ND_TEST_RUNTIME_VERSION: `m1nd-mcp ${STALE_VERSION}`,
  },
  () => {
    const tmp = mkTmpDir();
    const target = path.join(tmp, runtimeBinaryName());
    const release = path.join(tmp, "release-m1nd-mcp");
    const statePath = path.join(tmp, "update-state.json");
    writeVersionBinary(target, STALE_VERSION, "old");
    writeVersionBinary(release, CURRENT_VERSION, "new");
    const oldRuntime = fs.readFileSync(target);
    const newRuntime = fs.readFileSync(release);
    const verifiedRelease = writeVerifiedReleaseFixture(tmp, release);

    const applied = withEnv(
      {
        M1ND_TEST_RELEASE_DIR: verifiedRelease.releaseDir,
        M1ND_TEST_COSIGN_PATH: verifiedRelease.cosign,
        M1ND_TEST_RUNTIME_VERSION: undefined,
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
    assert(applied.applied_actions.some((entry) => entry.id === "runtime-install-github-release" && entry.version_verified));
    assert.strictEqual(sha256Text(fs.readFileSync(target)), sha256Text(newRuntime));
    assert.strictEqual(applied.test_overrides.active, true);
    assert.strictEqual(applied.test_overrides.release_transport, "local-test-directory");
    assert.strictEqual(applied.test_overrides.verifier_source, "explicit-test-executable");
    assert(applied.non_claims.some((claim) => claim.includes("not a live GitHub/Sigstore receipt")));
    const verifiedAction = applied.applied_actions.find((entry) => entry.id === "runtime-install-github-release");
    assert.strictEqual(verifiedAction.candidate_verification.transport_source, "local-test-directory");
    assert.strictEqual(verifiedAction.candidate_verification.verifier_source, "explicit-test-executable");
    const state = JSON.parse(fs.readFileSync(statePath, "utf8"));
    assert.strictEqual(state.phase, "installed");
    assert.strictEqual(state.install_kind, "verified-github-release");
    assert(fs.existsSync(state.backup_binary));
    assert.strictEqual(sha256Text(fs.readFileSync(state.backup_binary)), sha256Text(oldRuntime));
    assert.strictEqual(state.before_sha256, sha256Text(oldRuntime));
    assert.strictEqual(state.backup_sha256, sha256Text(oldRuntime));
    assert.strictEqual(state.candidate_sha256, sha256Text(newRuntime));
    assert.strictEqual(state.after_sha256, sha256Text(newRuntime));
    assert.strictEqual(
      fs.readdirSync(path.dirname(statePath)).some((name) => name.endsWith(".tmp")),
      false
    );
    assert.strictEqual(applied.requires_host_rebind, true);

    const rollback = withEnv(
      {
        ...fakeEnvBase,
        M1ND_TEST_RELEASE_DIR: verifiedRelease.releaseDir,
        M1ND_TEST_COSIGN_PATH: verifiedRelease.cosign,
        M1ND_TEST_RUNTIME_VERSION: undefined,
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
    assert.strictEqual(rollback.requires_host_rebind, true);
    assert.strictEqual(sha256Text(fs.readFileSync(target)), sha256Text(oldRuntime));
    const rolledBackState = JSON.parse(fs.readFileSync(statePath, "utf8"));
    assert.strictEqual(rolledBackState.phase, "rolled_back");
    assert.strictEqual(rolledBackState.restored_sha256, sha256Text(oldRuntime));

    const journalBeforeSecondRollback = fs.readFileSync(statePath);
    const secondRollback = withEnv(
      {
        ...fakeEnvBase,
        M1ND_TEST_RELEASE_DIR: verifiedRelease.releaseDir,
        M1ND_TEST_COSIGN_PATH: verifiedRelease.cosign,
        M1ND_TEST_RUNTIME_VERSION: undefined,
        M1ND_UPDATE_BACKUP_DIR: path.join(tmp, "backups"),
        M1ND_UPDATE_STATE_PATH: statePath,
      },
      () => selfUpdate({ _: ["update", "rollback"], binary: target, channel: "beta" })
    );
    assert(secondRollback.applied_actions.some((entry) => entry.id === "runtime-rollback" && entry.idempotent));
    assert.strictEqual(secondRollback.requires_host_rebind, false);
    assert.deepStrictEqual(fs.readFileSync(statePath), journalBeforeSecondRollback);
  }
);

// The updater keeps the proven legacy lane while accepting the first canonical
// candidate only when its signed digest binds the exact compatibility file and
// runtime bytes.  This avoids a cutover that would brick existing releases.
withEnv(
  {
    ...fakeEnvBase,
    M1ND_TEST_RUNTIME_VERSION: `m1nd-mcp ${STALE_VERSION}`,
  },
  () => {
    const tmp = mkTmpDir();
    const target = path.join(tmp, runtimeBinaryName());
    const release = path.join(tmp, "canonical-release-m1nd-mcp");
    const statePath = path.join(tmp, "canonical-update-state.json");
    writeVersionBinary(target, STALE_VERSION, "canonical-old");
    writeVersionBinary(release, CURRENT_VERSION, "canonical-new");
    const expectedBytes = fs.readFileSync(release);
    const fixture = writeCanonicalVerifiedReleaseFixture(tmp, release);
    const applied = withEnv(
      {
        M1ND_TEST_RELEASE_DIR: fixture.releaseDir,
        M1ND_TEST_COSIGN_PATH: fixture.cosign,
        M1ND_TEST_RUNTIME_VERSION: undefined,
        M1ND_UPDATE_BACKUP_DIR: path.join(tmp, "canonical-backups"),
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
    const action = applied.applied_actions.find((entry) => entry.id === "runtime-install-github-release");
    assert(
      action && action.ok,
      `canonical runtime install failed: ${JSON.stringify(
        action || applied.blocked_actions || applied.applied_actions
      )}`
    );
    assert.strictEqual(
      action.candidate_verification.candidate_schema,
      "m1nd-release-candidate-manifest-v1"
    );
    assert.strictEqual(action.candidate_verification.candidate_digest, fixture.candidateDigest);
    assert.strictEqual(
      action.candidate_verification.candidate_identity_kind,
      "canonical-domain-separated-digest"
    );
    assert.strictEqual(sha256Text(fs.readFileSync(target)), sha256Text(expectedBytes));
  }
);

withEnv(
  {
    ...fakeEnvBase,
    M1ND_TEST_RUNTIME_VERSION: `m1nd-mcp ${STALE_VERSION}`,
  },
  () => {
    const tmp = mkTmpDir();
    const target = path.join(tmp, runtimeBinaryName());
    const release = path.join(tmp, "tampered-canonical-release-m1nd-mcp");
    const statePath = path.join(tmp, "tampered-canonical-state.json");
    writeVersionBinary(target, STALE_VERSION, "canonical-old");
    writeVersionBinary(release, CURRENT_VERSION, "canonical-new");
    const before = fs.readFileSync(target);
    const fixture = writeCanonicalVerifiedReleaseFixture(tmp, release);
    fs.appendFileSync(fixture.compatibilityPath, "\n");
    const refused = withEnv(
      {
        M1ND_TEST_RELEASE_DIR: fixture.releaseDir,
        M1ND_TEST_COSIGN_PATH: fixture.cosign,
        M1ND_TEST_RUNTIME_VERSION: undefined,
        M1ND_UPDATE_BACKUP_DIR: path.join(tmp, "tampered-backups"),
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
    assert(refused.blocked_actions.some((entry) => entry.id === "runtime-install-failed"));
    assert.deepStrictEqual(fs.readFileSync(target), before);
    assert.strictEqual(fs.existsSync(statePath), false);
  }
);

} else {
  console.log(
    "# update apply/rollback harness scenarios skipped on win32 (shebang-only fake cosign; hosted G8 covers the real Windows updater)"
  );
}

withEnv(
  {
    ...fakeEnvBase,
    M1ND_TEST_RUNTIME_VERSION: `m1nd-mcp ${CURRENT_VERSION}`,
    // Use an isolated home dir so that a real ~/.claude.json does not affect
    // the "missing" assertion below.
    M1ND_TEST_HOME: mkTmpDir(),
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
    assert.strictEqual(
      ready.hosts[0].readiness,
      "ready",
      `host not ready: ${JSON.stringify({ host: ready.hosts[0], runtime: ready.runtime })}`
    );
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
        [selectedRuntime]: `m1nd-mcp ${CURRENT_VERSION}`,
        [realpathOrSame(selectedRuntime)]: `m1nd-mcp ${CURRENT_VERSION}`,
        [pathRuntime]: `m1nd-mcp ${STALE_VERSION}`,
        [realpathOrSame(pathRuntime)]: `m1nd-mcp ${STALE_VERSION}`,
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
        [selectedRuntime]: `m1nd-mcp ${CURRENT_VERSION}`,
        [realpathOrSame(selectedRuntime)]: `m1nd-mcp ${CURRENT_VERSION}`,
        [pathRuntime]: `m1nd-mcp ${STALE_VERSION}`,
        [realpathOrSame(pathRuntime)]: `m1nd-mcp ${STALE_VERSION}`,
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
        [selectedRuntime]: `m1nd-mcp ${CURRENT_VERSION}`,
        [realpathOrSame(selectedRuntime)]: `m1nd-mcp ${CURRENT_VERSION}`,
        [foreignRuntime]: `m1nd-mcp ${STALE_VERSION}`,
        [realpathOrSame(foreignRuntime)]: `m1nd-mcp ${STALE_VERSION}`,
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
    M1ND_TEST_RUNTIME_VERSION: `m1nd-mcp ${CURRENT_VERSION}`,
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
    M1ND_TEST_RUNTIME_VERSION: `m1nd-mcp ${CURRENT_VERSION}`,
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
    // --no-skills / --no-config disable ONLY the agent-pack and MCP-config writes;
    // doctrine and hook recipes have their own gating (--no-hooks) and still run.
    assert(!disabled.applied_actions.some((entry) => entry.id === "install-agent-pack"));
    assert(!disabled.applied_actions.some((entry) => entry.id === "write-mcp-config"));
    assert(disabled.blocked_actions.some((entry) => entry.id === "agent-pack-disabled"));
    assert(disabled.blocked_actions.some((entry) => entry.id === "config-disabled"));
  }
);

withEnv(
  {
    ...fakeEnvBase,
    M1ND_TEST_RUNTIME_VERSION: `m1nd-mcp ${CURRENT_VERSION}`,
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
    assert(fs.existsSync(path.join(testHome, ".codex", "skills", "m1nd-guardian", "SKILL.md")));
    assert(fs.existsSync(configPath));
    const config = fs.readFileSync(configPath, "utf8");
    assert(config.includes("[mcp_servers.m1nd]"));
    assert(config.includes("[mcp_servers.m1nd.env]"));
    assert(config.includes(`M1ND_WORKSPACE_ROOT = "${path.resolve(project).replace(/\\/g, "\\\\")}"`));
    assert(applied.changed_files.includes(configPath));
  }
);

// user-scope ~/.claude.json detection
withEnv(
  {
    ...fakeEnvBase,
    M1ND_TEST_RUNTIME_VERSION: `m1nd-mcp ${CURRENT_VERSION}`,
  },
  () => {
    const testHome = mkTmpDir();
    const project = mkTmpDir();
    // Write a fake ~/.claude.json as produced by `claude mcp add -s user`
    fs.writeFileSync(
      path.join(testHome, ".claude.json"),
      JSON.stringify({ mcpServers: { m1nd: { command: process.execPath, args: [], env: {} } } })
    );
    const status = withEnv(
      { M1ND_TEST_HOME: testHome },
      () =>
        hostStatus({
          _: ["hosts", "status"],
          host: "claude",
          project,
          binary: process.execPath,
        })
    );
    assert.strictEqual(status.hosts[0].host, "claude");
    assert.notStrictEqual(status.hosts[0].config.status, "missing");
    assert(
      status.hosts[0].config.status.includes("user-scope"),
      `Expected config.status to include "user-scope", got: ${status.hosts[0].config.status}`
    );
  }
);

const updateCheck = spawnSync(process.execPath, [cli, "update", "check", "--json", "--binary", process.execPath], {
  encoding: "utf8",
  env: {
    ...process.env,
    ...fakeEnvBase,
    M1ND_TEST_RUNTIME_VERSION: `m1nd-mcp ${CURRENT_VERSION}`,
  },
});
assert.strictEqual(updateCheck.status, 0, updateCheck.stderr);
assert.strictEqual(JSON.parse(updateCheck.stdout).schema, "m1nd-self-update-v0");

const updateStatus = spawnSync(process.execPath, [cli, "update", "status", "--json", "--binary", process.execPath], {
  encoding: "utf8",
  env: {
    ...process.env,
    ...fakeEnvBase,
    M1ND_TEST_RUNTIME_VERSION: `m1nd-mcp ${CURRENT_VERSION}`,
  },
});
assert.strictEqual(updateStatus.status, 0, updateStatus.stderr);
const updateStatusJson = JSON.parse(updateStatus.stdout);
assert.strictEqual(updateStatusJson.schema, "m1nd-self-update-v0");
assert.strictEqual(updateStatusJson.command, "status");
assert(updateStatusJson.status_summary);

// Cold-start bug 2: a fresh install must land on the PACKAGE's own version, never on a
// months-old beta. In the real registry the `beta` dist-tag trails far behind (an old
// 0.9.x) while `latest` tracks the shipped package — so the default channel must resolve
// `latest`, and target/plan must point at the package version, not the stale beta tag.
const registryBetaStale = JSON.stringify({
  "dist-tags": { beta: "0.9.0-beta.8", latest: CURRENT_VERSION },
  version: CURRENT_VERSION,
});
withEnv(
  {
    M1ND_TEST_NPM_VIEW_JSON: registryBetaStale,
    M1ND_TEST_GITHUB_RELEASE_AVAILABLE: "true",
    M1ND_TEST_CRATE_VERSION: CURRENT_VERSION,
    M1ND_TEST_HOME: mkTmpDir(),
  },
  () => {
    // Fresh install: no managed runtime yet (binary path does not exist).
    const freshPlan = selfUpdate({
      _: ["update", "plan"],
      binary: path.join(mkTmpDir(), "no-such-m1nd-mcp"),
      "no-kill": true,
    });
    // Default channel must be `latest`, and it must resolve the package version — not beta.
    assert.strictEqual(freshPlan.channel, "latest", "default channel must be latest, not beta");
    assert.strictEqual(
      freshPlan.latest_version,
      CURRENT_VERSION,
      "default channel must resolve the package version, not the stale beta dist-tag"
    );
    assert.strictEqual(freshPlan.target_version, CURRENT_VERSION, "target must be the package version");
    assert.strictEqual(freshPlan.install_state, "missing");
    const runtimeAction = freshPlan.planned_actions.find((entry) => entry.kind === "runtime");
    assert(runtimeAction, "a fresh install must plan a runtime install");
    assert.strictEqual(
      runtimeAction.target_version,
      CURRENT_VERSION,
      "runtime install must target the package version"
    );
    // The GitHub release for the package version is the primary source (v<CURRENT>).
    assert.strictEqual(runtimeAction.id, "runtime-install-github-release");
    assert(
      String(runtimeAction.url || "").includes(`/download/v${CURRENT_VERSION}/`),
      "runtime install must fetch the package-version GitHub release asset"
    );
    // No beta in the actions that actually execute (the registry echoes all dist-tags
    // for transparency, but nothing a fresh install RUNS may target the stale beta).
    assert(
      !JSON.stringify(freshPlan.planned_actions).includes("0.9.0-beta"),
      "fresh-install actions must not target the stale beta version"
    );

    // Doctor's fresh-install "next" advice must be a single sane step toward the package
    // version — no `--channel beta` gymnastics that would drag a stranger onto the old beta.
    const doctorRun = spawnSync(process.execPath, [cli, "doctor", "--json"], {
      encoding: "utf8",
      env: {
        ...process.env,
        M1ND_TEST_NPM_VIEW_JSON: registryBetaStale,
        M1ND_TEST_GITHUB_RELEASE_AVAILABLE: "true",
        M1ND_TEST_CRATE_VERSION: CURRENT_VERSION,
        // Simulate a stale runtime already on disk so the mismatch advice fires.
        M1ND_MCP_BINARY: process.execPath,
        M1ND_TEST_RUNTIME_VERSION: "m1nd-mcp 0.9.0-beta.8",
      },
    });
    assert.strictEqual(doctorRun.status, 0, doctorRun.stderr);
    const doctorJson = JSON.parse(doctorRun.stdout);
    const mismatchAdvice = doctorJson.next_actions.find((entry) => entry.includes("does not match package"));
    assert(mismatchAdvice, "doctor must flag the runtime/package mismatch");
    assert(!mismatchAdvice.includes("--channel beta"), "doctor must not steer a fresh user onto the beta channel");
  }
);

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
      M1ND_TEST_RUNTIME_VERSION: `m1nd-mcp ${CURRENT_VERSION}`,
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
      M1ND_TEST_RUNTIME_VERSION: `m1nd-mcp ${CURRENT_VERSION}`,
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
      M1ND_TEST_RUNTIME_VERSION: `m1nd-mcp ${CURRENT_VERSION}`,
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

// The fake m1nd-mcp runtime is a shebang script the agent CLI spawns over
// stdio; Windows cannot execute it, so the agent/kickstart scenarios below
// run on POSIX CI only. Cross-platform CLI contracts still run everywhere.
if (process.platform !== "win32") {

const fakeMcp = path.join(mkTmpDir(), runtimeBinaryName());
writeFakeMcpRuntime(fakeMcp);
const agentEnv = {
  ...process.env,
  ...fakeEnvBase,
  M1ND_TEST_RUNTIME_VERSION: `m1nd-mcp ${CURRENT_VERSION}`,
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

const agentTrustCallLog = path.join(mkTmpDir(), "calls.log");
const agentTrust = spawnSync(
  process.execPath,
  [cli, "agent", "trust", "--repo", agentScopeRepo, "--binary", fakeMcp, "--ensure-ingest", "--json"],
  {
    encoding: "utf8",
    env: {
      ...agentEnv,
      M1ND_FAKE_TRUST: "needs_ingest",
      M1ND_FAKE_CALL_LOG: agentTrustCallLog,
    },
  }
);
assert.strictEqual(agentTrust.status, 0, agentTrust.stderr);
const agentTrustJson = JSON.parse(agentTrust.stdout);
assert.strictEqual(agentTrustJson.schema, "m1nd-agent-cli-v0");
assert.strictEqual(agentTrustJson.command, "trust");
assert.strictEqual(agentTrustJson.ok, false);
assert.strictEqual(agentTrustJson.status, "needs_authority");
assert.strictEqual(agentTrustJson.proof_state, "NOT_PROVEN");
assert.strictEqual(agentTrustJson.trust.verdict, "needs_authority");
assert.strictEqual(agentTrustJson.authority.provider.configured, false);
assert.strictEqual(agentTrustJson.authority.mutation_attempted, false);
assert.strictEqual(agentTrustJson.mutation_policy.generic_ingest_called, false);
assert(agentTrustJson.authority.prohibited_fallbacks.includes("generic_ingest"));
assert(agentTrustJson.authority.recovery_instructions.some((entry) => entry.id === "configure_governed_provider"));
assert(!agentTrustJson.calls.some((entry) => entry.tool === "ingest"));
assert(!agentTrust.stdout.includes("run ingest for the intended repo"));
assert(!fs.readFileSync(agentTrustCallLog, "utf8").split(/\r?\n/).includes("ingest"));

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
assert(agentAutoJson.operating_contract.empty_graph_rule.includes("needs_authority/NOT_PROVEN"));
assert(agentAutoJson.action.action.command.includes(`--binary ${fakeMcp}`));
assert(agentAutoJson.next_actions[0].includes(`--binary ${fakeMcp}`));

const agentNextFirstMinute = spawnSync(
  process.execPath,
  [cli, "agent", "next", "--repo", agentOrientRepo, "--query", "use m1nd to understand this repo", "--binary", fakeMcp, "--json"],
  { encoding: "utf8", env: agentEnv }
);
assert.strictEqual(agentNextFirstMinute.status, 0, agentNextFirstMinute.stderr);
const agentNextFirstMinuteJson = JSON.parse(agentNextFirstMinute.stdout);
assert.strictEqual(agentNextFirstMinuteJson.action.route.kind, "first_minute");
assert.strictEqual(agentNextFirstMinuteJson.action.route.reason, "first_contact_broad_task");
assert(agentNextFirstMinuteJson.action.action.summary.includes("needs_authority/NOT_PROVEN"));
assert(agentNextFirstMinuteJson.next_actions[0].includes("agent first-minute"));
assert.strictEqual(agentNextFirstMinuteJson.task_profile.primary_intent, "deep_architecture");
assert(agentNextFirstMinuteJson.capability_suggestions[0].tools.includes("ghost_edges"));
assert(agentNextFirstMinuteJson.capability_suggestions[0].tools.includes("twins"));
assert(agentNextFirstMinuteJson.action.capability_suggestions[0].family_id === "retrobuilder");

const agentFirstMinuteCallLog = path.join(mkTmpDir(), "calls.log");
const agentFirstMinute = spawnSync(
  process.execPath,
  [cli, "agent", "first-minute", "--repo", agentOrientRepo, "--query", "audit architecture hidden coupling runtime bottlenecks duplicate refactor taint paths", "--binary", fakeMcp, "--json"],
  {
    encoding: "utf8",
    env: {
      ...agentEnv,
      M1ND_FAKE_TRUST: "needs_ingest",
      M1ND_FAKE_CALL_LOG: agentFirstMinuteCallLog,
    },
  }
);
assert.strictEqual(agentFirstMinute.status, 0, agentFirstMinute.stderr);
const agentFirstMinuteJson = JSON.parse(agentFirstMinute.stdout);
assert.strictEqual(agentFirstMinuteJson.command, "first-minute");
assert.strictEqual(agentFirstMinuteJson.ok, false);
assert.strictEqual(agentFirstMinuteJson.status, "needs_authority");
assert.strictEqual(agentFirstMinuteJson.proof_state, "NOT_PROVEN");
assert(!agentFirstMinuteJson.calls.some((entry) => entry.tool === "ingest"));
assert(!agentFirstMinuteJson.calls.some((entry) => entry.tool === "seek"));
assert.strictEqual(agentFirstMinuteJson.switch_to_direct_proof, true);
assert.strictEqual(agentFirstMinuteJson.m1nd_usage_mode, "authority_required_before_orientation");
assert(Array.isArray(agentFirstMinuteJson.anchors));
assert.strictEqual(agentFirstMinuteJson.anchors.length, 0);
assert(agentFirstMinuteJson.do_not.some((entry) => entry.includes("generic ingest")));
assert(!agentFirstMinute.stdout.includes("run ingest for the intended repo"));
assert.strictEqual(agentFirstMinuteJson.capability_suggestions[0].family_id, "retrobuilder");
assert.deepStrictEqual(
  ["ghost_edges", "taint_trace", "twins", "refactor_plan", "runtime_overlay"].every((tool) =>
    agentFirstMinuteJson.capability_suggestions[0].tools.includes(tool)
  ),
  true
);
assert(agentFirstMinuteJson.playbook.steps.some((step) => step.includes("RETROBUILDER")));
assert(!fs.readFileSync(agentFirstMinuteCallLog, "utf8").split(/\r?\n/).includes("ingest"));

// An already-ingested bound brain remains a read-only orientation lane.
const agentFirstMinuteReadyCallLog = path.join(mkTmpDir(), "calls.log");
const agentFirstMinuteReady = spawnSync(
  process.execPath,
  [cli, "agent", "first-minute", "--repo", agentOrientRepo, "--query", "audit architecture", "--binary", fakeMcp, "--json"],
  {
    encoding: "utf8",
    env: {
      ...agentEnv,
      M1ND_FAKE_CALL_LOG: agentFirstMinuteReadyCallLog,
    },
  }
);
assert.strictEqual(agentFirstMinuteReady.status, 0, agentFirstMinuteReady.stderr);
const agentFirstMinuteReadyJson = JSON.parse(agentFirstMinuteReady.stdout);
assert.strictEqual(agentFirstMinuteReadyJson.ok, true);
assert.strictEqual(agentFirstMinuteReadyJson.m1nd_usage_mode, "first_minute_orientation");
assert(agentFirstMinuteReadyJson.calls.some((entry) => entry.tool === "seek"));
assert(!agentFirstMinuteReadyJson.calls.some((entry) => entry.tool === "ingest"));
assert(agentFirstMinuteReadyJson.anchors.length > 0);
const agentFirstMinuteReadyCalls = fs.readFileSync(agentFirstMinuteReadyCallLog, "utf8").split(/\r?\n/);
assert(agentFirstMinuteReadyCalls.includes("seek"));
assert(!agentFirstMinuteReadyCalls.includes("ingest"));

// Legacy runtimes without trust_selftest stay compatible through the read-only
// session_handshake surface; their generic ingest verb is still never called.
const agentLegacyCallLog = path.join(mkTmpDir(), "calls.log");
const agentLegacyFirstMinute = spawnSync(
  process.execPath,
  [cli, "agent", "first-minute", "--repo", agentOrientRepo, "--query", "audit architecture", "--binary", fakeMcp, "--json"],
  {
    encoding: "utf8",
    env: {
      ...agentEnv,
      M1ND_FAKE_LEGACY_NO_TRUST_SELFTEST: "1",
      M1ND_FAKE_CALL_LOG: agentLegacyCallLog,
    },
  }
);
assert.strictEqual(agentLegacyFirstMinute.status, 0, agentLegacyFirstMinute.stderr);
const agentLegacyFirstMinuteJson = JSON.parse(agentLegacyFirstMinute.stdout);
assert.strictEqual(agentLegacyFirstMinuteJson.ok, true);
assert(!agentLegacyFirstMinuteJson.calls.some((entry) => entry.tool === "trust_selftest"));
assert(agentLegacyFirstMinuteJson.calls.some((entry) => entry.tool === "session_handshake"));
assert(agentLegacyFirstMinuteJson.calls.some((entry) => entry.tool === "seek"));
const agentLegacyCalls = fs.readFileSync(agentLegacyCallLog, "utf8").split(/\r?\n/);
assert(!agentLegacyCalls.includes("ingest"));

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
  [cli, "agent", "context", "--repo", agentOrientRepo, "--binary", fakeMcp, "--query", "trace chat flow", "--tokens", "800", "--json"],
  {
    encoding: "utf8",
    env: {
      ...agentEnv,
      M1ND_FAKE_SEARCH_FILE: "apps/experimental/tools_webhook/tool_caller.py",
    },
  }
);
assert.strictEqual(agentContext.status, 0, agentContext.stderr);
const agentContextJson = JSON.parse(agentContext.stdout);
assert.strictEqual(agentContextJson.command, "context");
assert.strictEqual(agentContextJson.ok, false);
assert.strictEqual(agentContextJson.needs_orientation_first, true);
assert.strictEqual(agentContextJson.context_confidence, "needs_orientation_first");
assert(!agentContextJson.calls.some((entry) => entry.tool === "surgical_context_v2"));

const agentContextAllowDiscovery = spawnSync(
  process.execPath,
  [cli, "agent", "context", "--repo", agentOrientRepo, "--binary", fakeMcp, "--query", "session boundary", "--allow-discovery", "--tokens", "800", "--json"],
  { encoding: "utf8", env: agentEnv }
);
assert.strictEqual(agentContextAllowDiscovery.status, 0, agentContextAllowDiscovery.stderr);
const agentContextAllowDiscoveryJson = JSON.parse(agentContextAllowDiscovery.stdout);
assert.strictEqual(agentContextAllowDiscoveryJson.command, "context");
assert(agentContextAllowDiscoveryJson.selected_file.endsWith(path.join("src", "session.js")));
assert.strictEqual(agentContextAllowDiscoveryJson.context_confidence, "discovery_allowed");
assert(agentContextAllowDiscoveryJson.calls.some((entry) => entry.tool === "surgical_context_v2"));

const directContextFile = path.join(agentOrientRepo, "src", "session.js");
fs.mkdirSync(path.dirname(directContextFile), { recursive: true });
fs.writeFileSync(directContextFile, "// direct context anchor\n");
const agentContextAnchor = spawnSync(
  process.execPath,
  [cli, "agent", "context", "--repo", agentOrientRepo, "--binary", fakeMcp, "--query", "session boundary", "--anchor", "src/session.js", "--tokens", "800", "--json"],
  { encoding: "utf8", env: agentEnv }
);
assert.strictEqual(agentContextAnchor.status, 0, agentContextAnchor.stderr);
const agentContextAnchorJson = JSON.parse(agentContextAnchor.stdout);
assert.strictEqual(agentContextAnchorJson.selected_file, directContextFile);
assert.strictEqual(agentContextAnchorJson.context_confidence, "direct_anchor");
assert(agentContextAnchorJson.calls.some((entry) => entry.tool === "surgical_context_v2"));

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
  [cli, "agent", "context", "--repo", agentOrientRepo, "--binary", fakeMcp, "--query", "session boundary", "--allow-discovery", "--tokens", "10", "--json"],
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
  [cli, "agent", "context", "--repo", agentOrientRepo, "--binary", fakeMcp, "--query", "session boundary", "--allow-discovery", "--json"],
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

// --- kickstart ---
// Smoke test for `m1nd kickstart` using the same fakeMcp/agentEnv pattern as
// the agent tests above. Verifies the m1nd-kickstart-v0 envelope is returned
// with required fields.
const kickstartRepo = mkTmpDir();
const agentKickstartRun = spawnSync(
  process.execPath,
  [cli, "kickstart", "--repo", kickstartRepo, "--binary", fakeMcp, "--json"],
  { encoding: "utf8", env: agentEnv }
);
assert.strictEqual(agentKickstartRun.status, 0, agentKickstartRun.stderr);
const kickstartJson = JSON.parse(agentKickstartRun.stdout);
assert.strictEqual(kickstartJson.schema, "m1nd-kickstart-v0", "schema must be m1nd-kickstart-v0");
assert.strictEqual(typeof kickstartJson.ok, "boolean", "ok must be boolean");
assert.strictEqual(typeof kickstartJson.node_count, "number", "node_count must be number");
assert.strictEqual(typeof kickstartJson.edge_count, "number", "edge_count must be number");
assert.strictEqual(typeof kickstartJson.next_action, "string", "next_action must be string");
assert(typeof kickstartJson.trust_verdict === "string", "trust_verdict must be string");
assert(kickstartJson.ingest && typeof kickstartJson.ingest.performed === "boolean", "ingest.performed must be boolean");
assert(typeof kickstartJson.audit_summary === "string", "audit_summary must be string");
assert(Array.isArray(kickstartJson.non_claims) && kickstartJson.non_claims.length >= 3, "non_claims must have >= 3 entries");
assert(kickstartJson.timing_ms && typeof kickstartJson.timing_ms.total === "number", "timing_ms.total must be number");
// With a fresh repo (no real graph) the kickstart should still return a valid envelope
// node_count comes from the fakeMcp which defaults to 12
assert.strictEqual(kickstartJson.node_count, 12);
assert.strictEqual(kickstartJson.edge_count, 21);
assert.strictEqual(kickstartJson.ok, true);
assert.strictEqual(kickstartJson.next_action, "ready_to_query");

} else {
  console.log(
    "# agent/kickstart scenarios skipped on win32 (shebang-only fake m1nd-mcp runtime)"
  );
}

// --- ambient recipes: hooks + doctrine per host --------------------------------

// 1. plan carries hook/doctrine per host and writes nothing to disk.
withEnv(
  {
    ...fakeEnvBase,
    M1ND_TEST_RUNTIME_VERSION: `m1nd-mcp ${CURRENT_VERSION}`,
    M1ND_TEST_HOME: mkTmpDir(),
  },
  () => {
    const tierA = new Set(["claude", "codex", "qwen", "kiro", "cline", "continue", "grok"]);
    const tierB = ["gemini", "antigravity", "cursor"];
    for (const host of [...tierA, ...tierB]) {
      const project = mkTmpDir();
      const plan = hostPlan({ _: ["hosts", "plan"], host, project, binary: process.execPath });
      const p = plan.plans[0];
      assert(p, `plan for ${host}`);
      assert(typeof p.doctrine.path === "string", `${host} doctrine path`);
      if (tierA.has(host)) {
        assert(p.hook && p.hook.event, `${host} tier-A hook`);
      } else {
        assert(p.hook && p.hook.reason, `${host} tier-B no-hook marker`);
      }
      if (host === "claude") {
        assert(typeof p.settings_block === "string" && p.settings_block.includes("SessionStart"), "claude settings_block");
      }
      // plan is pure print: nothing written.
      assert.strictEqual(fs.existsSync(p.doctrine.path), false, `${host} doctrine not written by plan`);
      if (p.hook && p.hook.config_path) {
        assert.strictEqual(fs.existsSync(p.hook.config_path), false, `${host} hook not written by plan`);
      }
    }
  }
);

// 2. apply idempotency for an owned-hook + doctrine host (codex).
withEnv(
  {
    ...fakeEnvBase,
    M1ND_TEST_RUNTIME_VERSION: `m1nd-mcp ${CURRENT_VERSION}`,
    M1ND_TEST_HOME: mkTmpDir(),
  },
  () => {
    const project = mkTmpDir();
    const applyArgs = { _: ["hosts", "apply"], host: "codex", project, binary: process.execPath, yes: true };
    const applied = hostApply(applyArgs);
    assert(applied.applied_actions.some((a) => a.id === "write-doctrine" && a.ok), "codex doctrine applied");
    assert(applied.applied_actions.some((a) => a.id === "write-hook-config" && a.ok), "codex hook applied");
    const idem = hostApply(applyArgs);
    assert(idem.applied_actions.some((a) => a.id === "write-hook-config" && a.changed === false), "codex hook idempotent");
    assert(idem.applied_actions.some((a) => a.id === "write-doctrine" && a.changed === false), "codex doctrine idempotent");
  }
);

// 3. never-clobber doctrine: foreign content preserved, managed block appended.
withEnv(
  {
    ...fakeEnvBase,
    M1ND_TEST_RUNTIME_VERSION: `m1nd-mcp ${CURRENT_VERSION}`,
    M1ND_TEST_HOME: mkTmpDir(),
  },
  () => {
    const project = mkTmpDir();
    const doctrinePath = path.join(project, "AGENTS.md");
    fs.mkdirSync(path.dirname(doctrinePath), { recursive: true });
    fs.writeFileSync(doctrinePath, "# my rules\nkeep this\n");
    hostApply({ _: ["hosts", "apply"], host: "codex", project, binary: process.execPath, yes: true });
    const text = fs.readFileSync(doctrinePath, "utf8");
    assert(text.includes("keep this"), "foreign doctrine content preserved");
    assert(text.includes("north"), "m1nd managed block appended");
  }
);

// 4. never-clobber hook: unrelated codex hook preserved, m1nd entry added.
withEnv(
  {
    ...fakeEnvBase,
    M1ND_TEST_RUNTIME_VERSION: `m1nd-mcp ${CURRENT_VERSION}`,
    M1ND_TEST_HOME: mkTmpDir(),
  },
  () => {
    const project = mkTmpDir();
    const hooksPath = path.join(homeForTest(), ".codex", "hooks.json");
    fs.mkdirSync(path.dirname(hooksPath), { recursive: true });
    fs.writeFileSync(
      hooksPath,
      JSON.stringify({ SessionStart: [{ matcher: "x", hooks: [{ type: "command", command: "echo other" }] }] }, null, 2)
    );
    hostApply({ _: ["hosts", "apply"], host: "codex", project, binary: process.execPath, yes: true });
    const parsed = JSON.parse(fs.readFileSync(hooksPath, "utf8"));
    const commands = parsed.SessionStart.flatMap((entry) => entry.hooks.map((h) => h.command));
    assert(commands.some((c) => c.includes("echo other")), "foreign hook preserved");
    assert(commands.some((c) => c.includes("m1nd-north-shim")), "m1nd hook added");
  }
);

// 5. claude settings surgical: apply must NOT write settings.json; block is carried.
withEnv(
  {
    ...fakeEnvBase,
    M1ND_TEST_RUNTIME_VERSION: `m1nd-mcp ${CURRENT_VERSION}`,
    M1ND_TEST_HOME: mkTmpDir(),
  },
  () => {
    const project = mkTmpDir();
    const applied = hostApply({ _: ["hosts", "apply"], host: "claude", project, binary: process.execPath, yes: true });
    assert.strictEqual(fs.existsSync(path.join(homeForTest(), ".claude", "settings.json")), false, "claude settings.json not written");
    const host = applied.hosts.find((h) => h.host === "claude");
    const hookAction = [...host.applied_actions, ...host.planned_actions].find((a) => a.kind === "hook");
    assert(hookAction, "claude hook action present");
    const block = hookAction.settings_block || hookAction.snippet || "";
    assert(block.includes("SessionStart"), "claude hook block includes SessionStart");
  }
);

// 6. codex TOML MCP config: exactly one [mcp_servers.m1nd] after apply (regression).
withEnv(
  {
    ...fakeEnvBase,
    M1ND_TEST_RUNTIME_VERSION: `m1nd-mcp ${CURRENT_VERSION}`,
    M1ND_TEST_HOME: mkTmpDir(),
  },
  () => {
    const project = mkTmpDir();
    const configPath = path.join(homeForTest(), ".codex", "config.toml");
    fs.mkdirSync(path.dirname(configPath), { recursive: true });
    fs.writeFileSync(configPath, '[mcp_servers.m1nd]\ncommand = "old"\nargs = ["--stdio"]\n');
    hostApply({ _: ["hosts", "apply"], host: "codex", project, binary: process.execPath, yes: true });
    const config = fs.readFileSync(configPath, "utf8");
    assert.strictEqual((config.match(/\[mcp_servers\.m1nd\]/g) || []).length, 1, "exactly one mcp_servers.m1nd section");
  }
);

// 7. cline OS gate (platform-independent, helper-level).
withEnv(
  {
    ...fakeEnvBase,
    M1ND_TEST_RUNTIME_VERSION: `m1nd-mcp ${CURRENT_VERSION}`,
    M1ND_TEST_HOME: mkTmpDir(),
  },
  () => {
    const project = mkTmpDir();
    const r = hostRecipe("cline", project);
    assert.deepStrictEqual(r.os_gate, ["darwin", "linux"], "cline declares os_gate");
    assert.strictEqual(osGateOk(r.os_gate, "win32"), false, "cline unsupported on win32");
    assert.strictEqual(osGateOk(r.os_gate, "darwin"), true, "cline supported on darwin");
    assert.strictEqual(osGateOk(null, "win32"), true, "null os_gate allows all");
  }
);

function homeForTest() {
  return process.env.M1ND_TEST_HOME || os.homedir();
}

// === restart --binary honesty (dry-run loudness) =========================
// Field bug: `restart --binary <target>` WITHOUT --yes is a silent dry-run —
// the operator reads "version: X -> X", assumes the swap happened, and nothing
// was installed. When a source IS installable, the plan must say so LOUDLY,
// naming the exact target, so the dry-run can never be mistaken for a swap.
{
  const repoRoot = path.resolve(__dirname, "..", "..");
  const target = path.join(mkTmpDir(), "m1nd-mcp");
  const plan = restart({
    source: repoRoot, // the m1nd checkout — sourceLooksBuildable() is true here
    binary: target,
    "no-kill": true, // do not touch real processes during the test
  });
  assert.strictEqual(plan.dry_run, true, "no --yes → dry run");
  assert.strictEqual(plan.source_buildable, true, "the m1nd checkout is buildable");
  assert(
    plan.next_actions.some(
      (action) =>
        action.includes("DRY RUN") &&
        action.includes("nothing was installed") &&
        action.includes(target)
    ),
    `dry-run plan must loudly say nothing was installed and name the target ${target}; got:\n` +
      plan.next_actions.map((a) => `  - ${a}`).join("\n")
  );
}

// When there is NO installable source, the loud dry-run install line must NOT
// appear (there was nothing to install — the honesty cuts both ways).
{
  const target = path.join(mkTmpDir(), "m1nd-mcp");
  const plan = restart({
    source: mkTmpDir(), // an empty dir — not a buildable m1nd checkout
    binary: target,
    "no-kill": true,
  });
  assert.strictEqual(plan.source_buildable, false, "an empty dir is not buildable");
  assert(
    !plan.next_actions.some((action) => action.includes("DRY RUN — nothing was installed")),
    "with no installable source, the loud install-swap line must be absent"
  );
}

// --- restart reload trio hardening (cli-operator #11) ---------------------

// (c) label parsing: `launchctl list` is `PID\tSTATUS\tLABEL`, and a label MAY
// contain whitespace. The label is columns 3..end, not the last token.
{
  assert.strictEqual(
    parseLaunchctlLabel("1234\t0\tcom.example.m1nd-serve"),
    "com.example.m1nd-serve",
    "a normal label parses whole"
  );
  assert.strictEqual(
    parseLaunchctlLabel("1234\t0\tcom.example.m1nd service with spaces"),
    "com.example.m1nd service with spaces",
    "a label WITH SPACES must survive intact (the .pop() bug kept only 'spaces')"
  );
  assert.strictEqual(
    parseLaunchctlLabel("-\t0\tcom.example.m1nd.idle"),
    "com.example.m1nd.idle",
    "a '-' PID column still yields the full label"
  );
  assert.strictEqual(parseLaunchctlLabel("PID\tStatus\tLabel"), "Label", "header parses by shape");
  assert.strictEqual(parseLaunchctlLabel(""), "", "a blank line yields no label");
  assert.strictEqual(parseLaunchctlLabel("only two"), "", "a malformed <3-column line yields no label");
}

// program-path parsing from `launchctl print` — both the `program = …` form and
// the `program-arguments` array first-entry fallback.
{
  const withProgram = [
    "com.example.m1nd = {",
    "\tactive count = 1",
    "\tprogram = /Users/<name>/.m1nd/bin/m1nd-mcp",
    "\targuments = {",
    "\t}",
    "}",
  ].join("\n");
  assert.strictEqual(
    parseLaunchctlProgramPath(withProgram),
    "/Users/<name>/.m1nd/bin/m1nd-mcp",
    "the explicit program line is preferred"
  );

  const withArgsOnly = [
    "com.example.m1nd = {",
    "\tprogram-arguments = {",
    "\t\t0 => /opt/m1nd/bin/m1nd-mcp",
    "\t\t1 => --serve",
    "\t}",
    "}",
  ].join("\n");
  assert.strictEqual(
    parseLaunchctlProgramPath(withArgsOnly),
    "/opt/m1nd/bin/m1nd-mcp",
    "falls back to the first program-argument"
  );

  assert.strictEqual(parseLaunchctlProgramPath("no program here"), null, "no program → null");
}

// (a) kickstart-scope: a label is kicked ONLY when its managed program IS the
// target binary. A different m1nd install must NOT match (that was the fleet-wide
// SIGKILL bug). A null program path is fail-closed (never a match).
{
  const tmp = mkTmpDir();
  const target = path.join(tmp, "m1nd-mcp");
  fs.writeFileSync(target, "#!/bin/sh\n");
  const other = path.join(tmp, "other-m1nd-mcp");
  fs.writeFileSync(other, "#!/bin/sh\n");

  assert.strictEqual(
    launchdLabelManagesTarget(target, target),
    true,
    "the exact target program matches"
  );
  assert.strictEqual(
    launchdLabelManagesTarget(other, target),
    false,
    "a DIFFERENT m1nd binary must NOT match — no fleet-wide kick"
  );
  assert.strictEqual(
    launchdLabelManagesTarget(null, target),
    false,
    "an undiscoverable program path is fail-closed (never kicked)"
  );
  assert.strictEqual(
    launchdLabelManagesTarget("", target),
    false,
    "an empty program path is fail-closed"
  );
}

// (b) codesign-gate: after an install, a FAILED darwin codesign must block the
// kickstart (re-execing an unsigned binary → OS kill loop). Everything else
// proceeds.
{
  assert.strictEqual(
    shouldKickstartAfterInstall("darwin", { attempted: true, ok: false }),
    false,
    "darwin + codesign FAILED must NOT kickstart"
  );
  assert.strictEqual(
    shouldKickstartAfterInstall("darwin", { attempted: true, ok: true }),
    true,
    "darwin + codesign ok proceeds"
  );
  assert.strictEqual(
    shouldKickstartAfterInstall("darwin", null),
    true,
    "darwin + no codesign attempted proceeds (e.g. no install this run)"
  );
  assert.strictEqual(
    shouldKickstartAfterInstall("linux", { attempted: true, ok: false }),
    true,
    "non-darwin ignores codesign entirely"
  );
}

// --- m1nd-north-shim (v3 ambient shim: north-first voice card) ---
// humanViewLines extracts the voice card from a top-level packet and from a
// first-minute-nested results[0], collapsing whitespace; no card -> [].
assert.deepStrictEqual(
  northShim.humanViewLines({ human_view: { lines: ["m1nd ╷ pulse", "  │ two"] } }),
  ["m1nd ╷ pulse", "│ two"],
  "humanViewLines reads a top-level human_view card"
);
assert.deepStrictEqual(
  northShim.humanViewLines({ results: [{ human_view: { lines: ["nested"] } }] }),
  ["nested"],
  "humanViewLines reads a first-minute nested card"
);
assert.deepStrictEqual(northShim.humanViewLines({}), [], "no card -> empty");

// renderNorthPacket opens with the card, then a blank line, then the summary.
const cardedPacket = northShim.renderNorthPacket({
  human_view: { lines: ["m1nd ╷ voice", "  │ line two"] },
  trust: { verdict: "grounded" },
});
assert(cardedPacket.startsWith("m1nd ╷ voice"), "packet opens with the voice card");
assert(cardedPacket.includes("\n\n[m1nd north]"), "blank line separates card and summary");

// No card -> exact prior behavior: the bare summary, no leading blank line.
const barePacket = northShim.renderNorthPacket({ trust: { verdict: "grounded" } });
assert(barePacket.startsWith("[m1nd north]"), "cardless packet is the bare summary");
assert(!barePacket.startsWith("\n"), "cardless packet has no leading blank line");

// capWholeLines never splits a line mid-way and drops a trailing blank separator.
assert.strictEqual(northShim.capWholeLines("aaa\nbbb\nccc", 5), "aaa", "keeps only whole lines under the cap");
assert.strictEqual(northShim.capWholeLines("aaa\n\nbbb", 4), "aaa", "drops a trailing blank left by the cut");
const longPacket = northShim.renderNorthPacket({
  human_view: { lines: ["A".repeat(400), "B".repeat(400), "C".repeat(400), "D".repeat(400)] },
  trust: { verdict: "ok" },
});
assert(longPacket.length <= 1200, "packet respects the 1200 cap");
assert(
  longPacket
    .split("\n")
    .filter(Boolean)
    .every((line) => line.length === 400 || line.startsWith("[m1nd north]")),
  "every kept line is a whole source line (no mid-line cut)"
);

// servedOwnerBaseUrls always ends with the default probe ports, deduped.
const shimUrls = northShim.servedOwnerBaseUrls();
assert(shimUrls.includes("http://127.0.0.1:1337"), "probe fallback includes :1337");
assert(shimUrls.includes("http://127.0.0.1:1338"), "probe fallback includes :1338");
assert.strictEqual(new Set(shimUrls).size, shimUrls.length, "candidate URLs are deduped");

console.log("npm cli tests ok");
