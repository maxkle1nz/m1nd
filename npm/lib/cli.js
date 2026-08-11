"use strict";

const crypto = require("crypto");
const fs = require("fs");
const os = require("os");
const path = require("path");
const { spawnSync } = require("child_process");
const { agentCommand, agentKickstart, AGENT_CLI_SCHEMA, KICKSTART_SCHEMA } = require("./agent-cli");

const PACKAGE_ROOT = path.resolve(__dirname, "..", "..");
const SKILLS_ROOT = path.join(PACKAGE_ROOT, "skills");
const UNIVERSAL_PACK = path.join(SKILLS_ROOT, "m1nd-universal-agent-pack.md");
const NPM_PACKAGE = "@maxkle1nz/m1nd";
const SELF_UPDATE_SCHEMA = "m1nd-self-update-v0";
const UPDATE_STATE_SCHEMA = "m1nd-self-update-rollback-state-v0";
const RELEASE_CANDIDATE_SCHEMA = "m1nd-release-candidate-v1";
const CANONICAL_RELEASE_CANDIDATE_SCHEMA = "m1nd-release-candidate-manifest-v1";
const CANONICAL_RELEASE_CANDIDATE_DOMAIN = CANONICAL_RELEASE_CANDIDATE_SCHEMA;
const CANONICAL_GATE_RECEIPT_SCHEMA = "m1nd-gate-receipt-v1";
// Custody floor of the authority custody era under which a receipt was minted
// (era-scoped; a successor Path-A era will carry a different value). Mirror of
// m1nd-control::release::{SECURE_ENCLAVE_CUSTODY_FLOOR_V1, RATIFIED_CUSTODY_FLOORS}.
// The production value comes from the ratified constant / ceremony receipt, never
// from request payload, and must be a member of this closed set.
const CANONICAL_SECURE_ENCLAVE_CUSTODY_FLOOR = "secure-enclave-single-host-v1";
const CANONICAL_RATIFIED_CUSTODY_FLOORS = new Set([CANONICAL_SECURE_ENCLAVE_CUSTODY_FLOOR]);
const CANONICAL_REVIEW_RECEIPT_SCHEMA = "m1nd-independent-adversarial-review-receipt-v1";
const CANONICAL_EVIDENCE_SET_EXTENSION_SCHEMA = "m1nd-release-evidence-set-json-extension-v1";
const CANONICALIZATION_VERSION = "m1nd-canonical-json-v1";
const CANONICAL_DIGEST_PREFIX = Buffer.from("m1nd-domain-separated-sha256-v1\0", "utf8");
const CANONICAL_COMPATIBILITY_SCHEMA = "m1nd-release-compatibility-manifest-v1";
const CANONICAL_COMPATIBILITY_FILE = "RELEASE-COMPATIBILITY.json";
const CANONICAL_COMPATIBILITY_ARTIFACT_KEY = "release_compatibility_manifest_v1";
const CANONICAL_ROLLBACK_ARTIFACT_KEY = "release_rollback_plan_v1";
const CANONICAL_RELEASE_ASSET_PREFIX = "release_asset:";
const CANONICAL_RELEASE_ARTIFACT_PREFIX = "release_artifact:";
const STRUCTURAL_RELEASE_STATUS = "STRUCTURALLY_VALID_NOT_CRYPTOGRAPHICALLY_VERIFIED";
const RELEASE_REPOSITORY = "maxkle1nz/m1nd";
const RELEASE_WORKFLOW = "release.yml";
const GITHUB_OIDC_ISSUER = "https://token.actions.githubusercontent.com";
const UPDATE_PHASES = new Set(["prepared", "installed", "rolled_back"]);
const SHA256_RE = /^[0-9a-f]{64}$/;
const RELEASE_DOWNLOAD_LIMITS = Object.freeze({
  "CANDIDATE.json": 16 * 1024 * 1024,
  "CANDIDATE.json.sigstore.json": 16 * 1024 * 1024,
  [CANONICAL_COMPATIBILITY_FILE]: 16 * 1024 * 1024,
  runtime: 256 * 1024 * 1024,
});
const HOST_READINESS_SCHEMA = "m1nd-host-readiness-v0";
const HOST_REBIND_PLAN_SCHEMA = "m1nd-host-rebind-plan-v0";
const HOST_APPLY_SCHEMA = "m1nd-host-apply-v0";
const PACK_ROUTING_CHECK_SCHEMA = "m1nd-agent-pack-routing-check-v0";

const HOST_LIST = [
  "codex", "claude", "gemini", "antigravity", "generic",
  "qwen", "kiro", "cline", "continue", "grok",
  "cursor", "windsurf", "zed", "vscode", "opencode",
  "warp", "trae", "jetbrains", "amp", "goose", "crush", "aider",
];
const HOSTS = new Set([...HOST_LIST, "all"]);

function usage() {
  return `m1nd installer

Usage:
  m1nd init [--host codex|claude|gemini|antigravity|generic|all] [--project <dir>]
  m1nd init --birth <repo>   Create a repo's graph (a one-time step you run yourself)
  m1nd install-skills <host> [--project <dir>]
  m1nd mcp-config <host> [--binary <path>] [--project <dir>]
  m1nd hosts status [--host codex|claude|gemini|antigravity|generic|all] [--project <dir>] [--binary <path>] [--json]
  m1nd hosts plan [--host codex|claude|gemini|antigravity|generic|all] [--project <dir>] [--binary <path>] [--json]
  m1nd hosts apply [--host codex|claude|gemini|antigravity|generic|qwen|kiro|cline|continue|grok|...|all] [--project <dir>] [--binary <path>] [--yes] [--no-skills] [--no-config] [--no-hooks] [--json]
  m1nd doctor [--json]
  m1nd version   (also: m1nd --version, m1nd -V)
  m1nd restart [--source <dir>] [--binary <path>] [--yes] [--json]
  m1nd update check [--channel beta|latest] [--json]
  m1nd update status [--channel beta|latest] [--json]
  m1nd update plan [--channel beta|latest] [--json]
  m1nd update apply [--channel beta|latest] [--yes] [--no-npm] [--no-runtime] [--no-skills] [--no-kill] [--json]
  m1nd update verify [--repo <dir>] [--transport stdio|http] [--json]
  m1nd update rollback [--json]
  m1nd agent scope --repo <dir> [--json]
  m1nd agent trust --repo <dir> [--ensure-ingest] [--json]
  m1nd agent first-minute --repo <dir> --query <text> [--mode short|normal|deep] [--no-attach] [--json]
  m1nd agent orient --repo <dir> --query <text> [--mode short|normal|deep] [--tool auto|search|seek|activate|audit|glob] [--json]
  m1nd agent auto --repo <dir> [--query <text> | --from <error|payload|stdin>] [--mode short|normal|deep] [--tool auto|search|seek|activate|audit|glob] [--json]
  m1nd agent next --repo <dir> [--query <text> | --from <error|payload|stdin>] [--mode short|normal|deep] [--tool auto|search|seek|activate|audit|glob] [--json]
  m1nd agent recover --repo <dir> --from <error|payload|stdin> [--json]
  m1nd agent context --repo <dir> --query <text> [--anchor <file>] [--allow-discovery] [--tokens <n>] [--no-attach] [--json]
  m1nd agent handoff --repo <dir> [--from last-run|mission] [--json]
  m1nd agent doctor --repo <dir> [--json]
  m1nd kickstart --repo <dir> [--audit-path <dir>] [--binary <path>] [--json]
  m1nd demo [--repo <dir>] [--transport stdio|http] [--binary <path>] [--json]
  m1nd smoke [--repo <dir>] [--transport stdio|http] [--binary <path>] [--json]
  m1nd pack-check [--json]
  m1nd pack-routing-check [--json]

This npm package installs the universal agent doctrine and host adapters. The
native runtime is still m1nd-mcp; doctor tells you whether it is visible.

restart is an external repair helper for stale host bindings. Without --yes it
prints the plan. With --yes it builds from source when available, installs the
native binary to the m1nd default path, and stops visible m1nd-mcp processes so
the host can relaunch/rebind.

update is the safe self-update surface. check/plan never mutate. apply mutates
only with --yes, writes runtime backups before replacement, and always reports
that active MCP hosts still need restart or rebind.

hosts status is a read-only universality-loop cockpit. It reports whether each
agent host has an agent pack, an MCP config hint, a current runtime, and the
remaining rebind caveat.

hosts plan is the read-only follow-through: it emits per-host install, config,
workspace, rebind, and verification recipes without editing any host files.

hosts apply is the opt-in local mutation step. Without --yes it is a dry-run.
With --yes it installs local agent packs and writes canonical MCP config files
for known hosts, but active clients still need restart/rebind.

agent is the host-neutral operating layer for coding agents. It first asks the
runtime whether a live serve owner already holds --repo (the same two questions
--attach auto asks: an owner for this runtime root, else an owner whose declared
ingest roots cover this repo). When one answers, agent bridges to it and reads
the machine's real graph; when none does, it launches an isolated m1nd-mcp
runtime bound to --repo and says so. Either way it emits deterministic JSON
outside any stale MCP host and tells the agent when to switch back to direct
proof. Pass --no-attach to force the isolated runtime.

agent auto/next is the deterministic route picker. It does not claim proof; it
chooses the next bounded agent step or hands control back to direct proof. For
deep architecture, hidden coupling, security/taint, duplication/refactor, or
runtime-heat tasks, it emits RETROBUILDER capability_suggestions.

agent first-minute is the safest first contact for a new repo. It scopes,
checks trust, and runs one bounded read-only orientation pass only when the bound
brain already has an ingested graph — the live serve owner's graph when one
covers this repo, the isolated runtime's otherwise. An empty graph returns
deterministic needs_authority/NOT_PROVEN recovery instructions naming why no
owner was reached; the npm CLI never calls generic
ingest, legacy bootstrap, or a software-test authority fallback. It can also surface RETROBUILDER
tools such as ghost_edges, taint_trace, twins, refactor_plan, and
runtime_overlay when the query asks for those deeper lenses.

agent context is anchor-first. Use --anchor or a concrete file path for capsules;
use --allow-discovery only when you intentionally accept discovery overhead.`;
}

function parseArgs(args) {
  const parsed = { _: [] };
  for (let index = 0; index < args.length; index += 1) {
    const arg = args[index];
    if (!arg.startsWith("--")) {
      parsed._.push(arg);
      continue;
    }
    const key = arg.slice(2);
    if (
      [
        "build",
        "allow-discovery",
        "help",
        "install",
        "ensure-ingest",
        "json",
        "kill",
        "no-attach",
        "no-build",
        "no-config",
        "no-hooks",
        "no-install",
        "no-kill",
        "no-npm",
        "no-runtime",
        "no-skills",
        "shared-runtime",
        "skip-ingest",
        "version",
        "yes",
      ].includes(key)
    ) {
      parsed[key] = true;
      continue;
    }
    const value = args[index + 1];
    if (!value || value.startsWith("--")) {
      throw new Error(`missing value for --${key}`);
    }
    parsed[key] = value;
    index += 1;
  }
  return parsed;
}

function ensureDir(dir) {
  fs.mkdirSync(dir, { recursive: true });
}

function copyFile(source, target) {
  ensureDir(path.dirname(target));
  fs.copyFileSync(source, target);
}

const GENERATED_SKILL_ARTIFACTS = new Set([
  "graph_snapshot.json",
  "plasticity_state.json",
  "query_memory.json",
]);

function isGeneratedSkillArtifact(entryName) {
  return GENERATED_SKILL_ARTIFACTS.has(entryName);
}

function cleanGeneratedSkillArtifacts(target) {
  for (const entryName of GENERATED_SKILL_ARTIFACTS) {
    fs.rmSync(path.join(target, entryName), { force: true, recursive: true });
  }
}

function copyDir(source, target) {
  ensureDir(target);
  cleanGeneratedSkillArtifacts(target);
  for (const entry of fs.readdirSync(source, { withFileTypes: true })) {
    const sourcePath = path.join(source, entry.name);
    const targetPath = path.join(target, entry.name);
    if (isGeneratedSkillArtifact(entry.name)) {
      fs.rmSync(targetPath, { force: true, recursive: true });
      continue;
    }
    if (entry.isDirectory()) {
      copyDir(sourcePath, targetPath);
    } else if (entry.isFile()) {
      copyFile(sourcePath, targetPath);
    }
  }
}

function which(binary) {
  const paths = (process.env.PATH || "").split(path.delimiter);
  const extensions = process.platform === "win32" ? ["", ".exe", ".cmd", ".bat"] : [""];
  for (const dir of paths) {
    for (const extension of extensions) {
      const candidate = path.join(dir, `${binary}${extension}`);
      if (fs.existsSync(candidate)) {
        return candidate;
      }
    }
  }
  return null;
}

// Release verification is a privileged mutation path.  It must not inherit an
// executable from an arbitrary repository-controlled PATH entry.  Search only
// fixed operating-system/package-manager locations and return the canonical
// regular executable.  General discovery keeps using `which`; updater trust
// decisions never do.
function trustedUpdateTool(binary) {
  if (typeof binary !== "string" || !/^[A-Za-z0-9._+-]+$/.test(binary)) return null;
  const directories =
    process.platform === "win32"
      ? [
          process.env.SystemRoot ? path.join(process.env.SystemRoot, "System32") : null,
          process.env.ProgramFiles ? path.join(process.env.ProgramFiles, "cosign") : null,
        ]
      : ["/usr/bin", "/bin", "/usr/sbin", "/sbin", "/usr/local/bin", "/opt/homebrew/bin", "/opt/local/bin", "/snap/bin"];
  const extensions = process.platform === "win32" ? [".exe", ".cmd"] : [""];
  for (const directory of directories.filter(Boolean)) {
    for (const extension of extensions) {
      const candidate = path.join(directory, `${binary}${extension}`);
      try {
        const canonical = fs.realpathSync.native(candidate);
        const stat = fs.statSync(canonical);
        if (!path.isAbsolute(canonical) || !stat.isFile()) continue;
        if (process.platform !== "win32" && (stat.mode & 0o111) === 0) continue;
        if (process.platform !== "win32" && (stat.mode & 0o002) !== 0) continue;
        return canonical;
      } catch (_) {
        // Missing, dangling, unreadable, or non-regular candidates are not
        // updater authorities. Continue to the next fixed location.
      }
    }
  }
  return null;
}

function trustedNodePackageManager() {
  const node = trustedUpdateTool("node");
  const npm = trustedUpdateTool("npm");
  if (!node || !npm) return null;
  // Unix npm launchers are JavaScript entry points with an env-based shebang.
  // Execute the canonical script with the separately trusted Node binary so a
  // hostile PATH cannot choose the interpreter. Windows `.cmd` launchers are
  // intentionally refused until a fixed-path, non-shell npm entry point is
  // resolved and proven there.
  if (process.platform === "win32" || path.extname(npm).toLowerCase() === ".cmd") {
    return null;
  }
  return { node, prefix: [npm], source: "trusted-fixed-node-and-npm" };
}

function runtimeBinaryName(platform = process.platform) {
  return platform === "win32" ? "m1nd-mcp.exe" : "m1nd-mcp";
}

function homeDir() {
  return process.env.M1ND_TEST_HOME || os.homedir();
}

function defaultRuntimePath(platform = process.platform, homeDirValue = homeDir()) {
  const pathModule = platform === "win32" ? path.win32 : path;
  return pathModule.join(homeDirValue, ".m1nd", "bin", runtimeBinaryName(platform));
}

function findRuntimeBinary() {
  // Test seam, same family as the M1ND_TEST_* hooks below: force the
  // no-runtime branch deterministically so the doctor's "which door?" answer
  // can be pinned without depending on the host's PATH.
  if (process.env.M1ND_TEST_RUNTIME_ABSENT) return null;
  const managedRuntime = defaultRuntimePath();
  return (
    process.env.M1ND_MCP_BINARY ||
    process.env.M1ND_MCP_BIN ||
    (fs.existsSync(managedRuntime) ? managedRuntime : null) ||
    which("m1nd-mcp")
  );
}

function readPackageVersion() {
  const packageJson = JSON.parse(fs.readFileSync(path.join(PACKAGE_ROOT, "package.json"), "utf8"));
  return packageJson.version;
}

function assertPackShape() {
  const required = [
    path.join(SKILLS_ROOT, "m1nd-first", "SKILL.md"),
    path.join(SKILLS_ROOT, "m1nd-guardian", "SKILL.md"),
    path.join(SKILLS_ROOT, "m1nd-operator", "SKILL.md"),
    path.join(SKILLS_ROOT, "m1nd-operator", "references", "routing-playbooks.md"),
    path.join(SKILLS_ROOT, "m1nd-operator", "references", "tool-families.md"),
    path.join(SKILLS_ROOT, "m1nd-operator", "references", "full-spec-agent-os.md"),
    path.join(SKILLS_ROOT, "m1nd-operator", "references", "runtime-and-refresh.md"),
    path.join(SKILLS_ROOT, "m1nd-operator", "references", "l1ght-and-docs.md"),
    path.join(SKILLS_ROOT, "m1nd-operator", "scripts", "probe_m1nd.py"),
    path.join(PACKAGE_ROOT, "docs", "M1ND-GUARDIAN-METHOD.md"),
    UNIVERSAL_PACK,
  ];
  const missing = required.filter((file) => !fs.existsSync(file));
  return { ok: missing.length === 0, missing, required };
}

const DEFAULT_PACK_ROUTING_FILES = [
  {
    id: "m1nd-first",
    relative_path: "skills/m1nd-first/SKILL.md",
    checks: [
      { id: "session-companion-section", needles: ["Session Companion Bridge"] },
      { id: "companion-continuity-only", needles: ["COMPANION", "continuity"] },
      { id: "m1nd-agent-next-route", needles: ["m1nd agent next", "current task"] },
      { id: "m1nd-agent-first-minute-route", needles: ["m1nd agent first-minute", "first contact"] },
      { id: "retrobuilder-routing", needles: ["RETROBUILDER", "ghost_edges", "runtime_overlay", "direct source"] },
      { id: "no-companion-code-truth", needles: ["code truth"] },
      { id: "direct-proof-final-truth", needles: ["direct proof", "decides what is true"] },
    ],
  },
  {
    id: "m1nd-guardian",
    relative_path: "skills/m1nd-guardian/SKILL.md",
    checks: [
      { id: "one-active-front", needles: ["One active front", "proof boundary"] },
      { id: "proof-state-separation", needles: ["SOURCE_IMPLEMENTED", "LIVE_PROVEN", "RELEASE_PROVEN", "ACTIVE"] },
      { id: "same-candidate", needles: ["same immutable candidate", "candidate digest"] },
      { id: "human-only-sovereignty", needles: ["Human-only", "self-ratify"] },
      { id: "succession-test", needles: ["Succession test", "cold agent"] },
    ],
  },
  {
    id: "m1nd-operator",
    relative_path: "skills/m1nd-operator/SKILL.md",
    checks: [
      { id: "session-companion-routing", needles: ["Session Companion Routing"] },
      { id: "companion-continuity", needles: ["COMPANION", "conversation continuity"] },
      { id: "companion-orientation-only", needles: ["companion_orientation_only"] },
      { id: "m1nd-agent-next-first-move", needles: ["m1nd agent next", "first safe repo move"] },
      { id: "m1nd-agent-first-minute-route", needles: ["m1nd agent first-minute", "first contact"] },
      { id: "context-anchor-first", needles: ["agent context", "anchor"] },
      { id: "retrobuilder-routing", needles: ["RETROBUILDER", "ghost_edges", "runtime_overlay", "direct source"] },
      { id: "m1nd-mcp-structural-role", needles: ["m1nd MCP tools", "structural context"] },
      { id: "direct-proof-role", needles: ["Direct proof", "focused probes"] },
      { id: "global-search-warning", needles: ["Global companion search", "candidate discovery only"] },
    ],
  },
  {
    id: "m1nd-universal-agent-pack",
    relative_path: "skills/m1nd-universal-agent-pack.md",
    checks: [
      { id: "session-companions-section", needles: ["Session Companions"] },
      { id: "companion-continuity", needles: ["COMPANION", "continuity"] },
      { id: "companion-orientation-only", needles: ["companion_orientation_only"] },
      { id: "m1nd-agent-next-first-move", needles: ["m1nd agent next", "first safe repo move"] },
      { id: "m1nd-agent-first-minute-route", needles: ["m1nd agent first-minute", "first contact"] },
      { id: "context-anchor-first", needles: ["agent context", "anchor"] },
      { id: "retrobuilder-routing", needles: ["RETROBUILDER", "ghost_edges", "runtime_overlay", "direct source"] },
      { id: "m1nd-mcp-structural-role", needles: ["m1nd MCP tools", "structural"] },
      { id: "direct-proof-final-truth", needles: ["direct proof", "final truth"] },
    ],
  },
  {
    id: "agent-packs-doc",
    relative_path: "docs/AGENT-PACKS.md",
    checks: [
      { id: "session-memory-companions-section", needles: ["Session Memory Companions"] },
      { id: "companion-continuity", needles: ["COMPANION", "continuity"] },
      { id: "companion-orientation-only", needles: ["companion_orientation_only"] },
      { id: "m1nd-agent-next-first-move", needles: ["m1nd agent next", "first safe repo move"] },
      { id: "m1nd-agent-first-minute-route", needles: ["m1nd agent first-minute", "first contact"] },
      { id: "context-anchor-first", needles: ["agent context", "anchor"] },
      { id: "retrobuilder-routing", needles: ["RETROBUILDER", "ghost_edges", "runtime_overlay", "direct source"] },
      { id: "m1nd-mcp-structural-role", needles: ["m1nd MCP tools", "structural"] },
      { id: "direct-proof-final-truth", needles: ["direct proof", "source, tests"] },
    ],
  },
];

const DEFAULT_PACK_ROUTING_CONTRACT = [
  {
    id: "session-companion-is-continuity",
    description: "session companions are for continuity and prior decisions",
    needles: ["session companion", "continuity", "prior decisions"],
  },
  {
    id: "m1nd-agent-next-is-first-repo-move",
    description: "m1nd agent next is the first safe repo move",
    needles: ["m1nd agent next", "first safe repo move"],
  },
  {
    id: "m1nd-agent-first-minute-is-first-contact",
    description: "m1nd agent first-minute is the safest first contact loop",
    needles: ["m1nd agent first-minute", "first contact"],
  },
  {
    id: "agent-context-is-anchor-first",
    description: "agent context requires anchors before capsules",
    needles: ["agent context", "anchor"],
  },
  {
    id: "m1nd-mcp-is-structural-context",
    description: "m1nd MCP tools provide graph/docs/impact/mission context",
    needles: ["m1nd MCP tools", "structural"],
  },
  {
    id: "direct-proof-is-final-truth",
    description: "direct proof remains final truth for code behavior",
    needles: ["direct proof", "final truth"],
  },
  {
    id: "retrobuilder-is-taught",
    description: "RETROBUILDER deep graph tools are taught in distributed packs",
    needles: ["RETROBUILDER", "ghost_edges", "taint_trace", "twins", "refactor_plan", "runtime_overlay"],
  },
  {
    id: "companion-search-is-not-code-truth",
    description: "global companion search is not code truth",
    needles: ["global memory search", "code truth"],
  },
];

function needleMatches(text, needle) {
  if (needle instanceof RegExp) return needle.test(text);
  const haystack = text.toLowerCase().replace(/\s+/g, " ");
  const target = String(needle).toLowerCase().replace(/\s+/g, " ");
  return haystack.includes(target);
}

function needleLabel(needle) {
  return needle instanceof RegExp ? needle.toString() : String(needle);
}

function evaluateNeedles(text, check) {
  const missing = (check.needles || []).filter((needle) => !needleMatches(text, needle));
  return {
    id: check.id,
    ok: missing.length === 0,
    description: check.description || null,
    required: (check.needles || []).map(needleLabel),
    missing: missing.map(needleLabel),
  };
}

function packRoutingCheck(options = {}) {
  const fileSpecs = options.files || DEFAULT_PACK_ROUTING_FILES;
  const contractSpecs = options.contractChecks || DEFAULT_PACK_ROUTING_CONTRACT;
  const files = fileSpecs.map((spec) => {
    const filePath = spec.path || path.join(PACKAGE_ROOT, spec.relative_path);
    const exists = fs.existsSync(filePath);
    const text = exists ? fs.readFileSync(filePath, "utf8") : "";
    const checks = (spec.checks || []).map((check) => evaluateNeedles(text, check));
    return {
      id: spec.id,
      path: filePath,
      relative_path: spec.relative_path || path.relative(PACKAGE_ROOT, filePath),
      exists,
      ok: exists && checks.every((check) => check.ok),
      checks,
      _text: text,
    };
  });
  const aggregateText = files.map((file) => file._text).join("\n\n");
  const contract_checks = contractSpecs.map((check) => evaluateNeedles(aggregateText, check));
  const missing = [];
  for (const file of files) {
    if (!file.exists) {
      missing.push({ file: file.relative_path, check: "file-exists" });
    }
    for (const check of file.checks) {
      for (const needle of check.missing) {
        missing.push({ file: file.relative_path, check: check.id, missing: needle });
      }
    }
  }
  for (const check of contract_checks) {
    for (const needle of check.missing) {
      missing.push({ file: "*", check: check.id, missing: needle });
    }
  }
  return {
    schema: PACK_ROUTING_CHECK_SCHEMA,
    ok: missing.length === 0,
    files: files.map(({ _text, ...file }) => file),
    contract_checks,
    missing,
    non_claims: [
      "pack-routing-check verifies packaged doctrine text, not live host behavior.",
      "pack-routing-check does not prove a session companion such as COMPANION is installed.",
      "pack-routing-check does not prove m1nd retrieval correctness or code behavior.",
      "pack-routing-check does not refresh MCP host bindings or cached tool lists.",
    ],
  };
}

function installCodex() {
  const targetRoot = path.join(homeDir(), ".codex", "skills");
  copyDir(path.join(SKILLS_ROOT, "m1nd-first"), path.join(targetRoot, "m1nd-first"));
  copyDir(path.join(SKILLS_ROOT, "m1nd-guardian"), path.join(targetRoot, "m1nd-guardian"));
  copyDir(path.join(SKILLS_ROOT, "m1nd-operator"), path.join(targetRoot, "m1nd-operator"));
  return {
    host: "codex",
    installed: [
      path.join(targetRoot, "m1nd-first"),
      path.join(targetRoot, "m1nd-guardian"),
      path.join(targetRoot, "m1nd-operator"),
    ],
  };
}

function hostRuleFilename(host) {
  switch (host) {
    case "claude":
      return "CLAUDE.md";
    case "gemini":
      return "GEMINI.md";
    case "antigravity":
      return "AGENTS.md";
    default:
      return "m1nd-agent-rules.md";
  }
}

function installPortable(host, projectDir) {
  const targetRoot = path.join(projectDir, ".m1nd", "agent-pack");
  copyDir(SKILLS_ROOT, path.join(targetRoot, "skills"));
  const ruleFile = path.join(targetRoot, hostRuleFilename(host));
  copyFile(UNIVERSAL_PACK, ruleFile);
  return {
    host,
    installed: [targetRoot, ruleFile],
    note: "Point your agent host at the rule file or paste it into the host custom-instructions surface.",
  };
}

function installSkills(host, projectDir) {
  if (!HOSTS.has(host)) {
    throw new Error(`unsupported host '${host}'. Supported hosts: ${Array.from(HOSTS).join(", ")}`);
  }
  if (host === "all") {
    return [
      installCodex(),
      installPortable("claude", projectDir),
      installPortable("gemini", projectDir),
      installPortable("antigravity", projectDir),
      installPortable("generic", projectDir),
    ];
  }
  if (host === "codex") {
    return [installCodex()];
  }
  return [installPortable(host, projectDir)];
}

function tomlEscape(value) {
  return String(value).replace(/\\/g, "\\\\").replace(/"/g, '\\"');
}

function mcpConfig(host, binary, projectDir = null) {
  const command = binary || findRuntimeBinary() || defaultRuntimePath();
  const env = projectDir ? { M1ND_WORKSPACE_ROOT: path.resolve(projectDir) } : null;
  if (host === "codex") {
    let snippet = `[mcp_servers.m1nd]
command = "${tomlEscape(command)}"
args = ["--stdio", "--no-gui"]
`;
    if (env) {
      snippet += `
[mcp_servers.m1nd.env]
M1ND_WORKSPACE_ROOT = "${tomlEscape(env.M1ND_WORKSPACE_ROOT)}"
`;
    }
    return snippet;
  }
  return JSON.stringify(
    {
      mcpServers: {
        m1nd: {
          command,
          args: ["--stdio", "--no-gui"],
          ...(env ? { env } : {}),
        },
      },
    },
    null,
    2
  );
}

function fileContains(file, needle) {
  if (!fs.existsSync(file)) return false;
  try {
    return fs.readFileSync(file, "utf8").includes(needle);
  } catch (_) {
    return false;
  }
}

function readText(file) {
  if (!fs.existsSync(file)) return "";
  try {
    return fs.readFileSync(file, "utf8");
  } catch (_) {
    return "";
  }
}

function realPathOrResolved(file) {
  if (!file) return null;
  try {
    return fs.realpathSync.native(file);
  } catch (_) {
    return path.resolve(file);
  }
}

function sameFilesystemTarget(left, right) {
  if (!left || !right) return false;
  return realPathOrResolved(left) === realPathOrResolved(right);
}

function extractRuntimePaths(text, selectedBinary = null) {
  const paths = new Set();
  const normalized = String(text || "");
  const runtimePathPattern = process.platform === "win32"
    ? /[A-Za-z]:\\[^"'\s,\]]*m1nd-mcp(?:\.exe)?/g
    : /\/[^"'\s,\]]*m1nd-mcp(?:\.exe)?/g;
  for (const match of normalized.matchAll(runtimePathPattern)) {
    paths.add(match[0]);
  }
  if (selectedBinary && normalized.includes(selectedBinary)) {
    paths.add(selectedBinary);
  }
  return Array.from(paths);
}

function runtimeBindingsFromText(text, selectedBinary, packageVersion) {
  return extractRuntimePaths(text, selectedBinary).map((runtimePath) => {
    const exists = fs.existsSync(runtimePath);
    const version = exists ? runtimeVersion(runtimePath) : null;
    return {
      path: runtimePath,
      exists,
      version,
      current: Boolean(version && version.includes(packageVersion)),
      matches_selected: Boolean(selectedBinary && exists && sameFilesystemTarget(runtimePath, selectedBinary)),
    };
  });
}

function tomlSections(text, predicate) {
  const lines = String(text || "").split(/\r?\n/);
  const selected = [];
  let include = false;
  for (const line of lines) {
    const section = line.match(/^\s*\[([^\]]+)\]\s*$/);
    if (section) {
      include = predicate(section[1]);
    }
    if (include) selected.push(line);
  }
  return selected.join("\n");
}

function hostScopedConfigText(host, text) {
  if (host === "codex") {
    return tomlSections(
      text,
      (section) => section === "mcp_servers.m1nd" || section.startsWith("mcp_servers.m1nd.")
    );
  }
  const parsed = safeJsonParse(text);
  const server =
    parsed &&
    parsed.mcpServers &&
    typeof parsed.mcpServers === "object" &&
    parsed.mcpServers.m1nd &&
    typeof parsed.mcpServers.m1nd === "object"
      ? parsed.mcpServers.m1nd
      : null;
  return server ? JSON.stringify(server) : text;
}

function portableAgentPackPaths(host, projectDir) {
  const targetRoot = path.join(projectDir, ".m1nd", "agent-pack");
  const skillsRoot = path.join(targetRoot, "skills");
  return {
    target_root: targetRoot,
    skills_root: skillsRoot,
    first_skill: path.join(skillsRoot, "m1nd-first", "SKILL.md"),
    guardian_skill: path.join(skillsRoot, "m1nd-guardian", "SKILL.md"),
    operator_skill: path.join(skillsRoot, "m1nd-operator", "SKILL.md"),
    rule_file: path.join(targetRoot, hostRuleFilename(host)),
  };
}

function agentPackStatusForHost(host, projectDir) {
  if (host === "codex") {
    const skillRoot = path.join(homeDir(), ".codex", "skills");
    const required = [
      path.join(skillRoot, "m1nd-first", "SKILL.md"),
      path.join(skillRoot, "m1nd-guardian", "SKILL.md"),
      path.join(skillRoot, "m1nd-operator", "SKILL.md"),
    ];
    const missing = required.filter((file) => !fs.existsSync(file));
    return {
      install_kind: "codex-skill-root",
      target_root: skillRoot,
      rule_file: null,
      required,
      missing,
      installed: missing.length === 0,
    };
  }

  const paths = portableAgentPackPaths(host, projectDir);
  const required = [paths.first_skill, paths.guardian_skill, paths.operator_skill, paths.rule_file];
  const missing = required.filter((file) => !fs.existsSync(file));
  return {
    install_kind: "project-local-portable-pack",
    target_root: paths.target_root,
    rule_file: paths.rule_file,
    required,
    missing,
    installed: missing.length === 0,
  };
}

function hostConfigCandidates(host, projectDir) {
  switch (host) {
    case "codex":
      return [path.join(homeDir(), ".codex", "config.toml")];
    case "claude":
      return [
        path.join(projectDir, ".claude", "mcp.json"),
        path.join(projectDir, "claude_mcp.json"),
        path.join(homeDir(), ".claude.json"),
      ];
    case "gemini":
      return [path.join(projectDir, ".gemini", "settings.json"), path.join(projectDir, "gemini_mcp.json")];
    case "antigravity":
      return [path.join(projectDir, "mcp_config.json")];
    default:
      return [];
  }
}

function hostConfigStatus(host, projectDir, binary, packageVersion = readPackageVersion()) {
  const candidates = hostConfigCandidates(host, projectDir);
  const snippetCommand = `m1nd mcp-config ${host} --project ${projectDir}`;
  if (candidates.length === 0) {
    return {
      status: "manual",
      candidates: [],
      expected_command: binary || defaultRuntimePath(),
      snippet_command: snippetCommand,
      note: "Generic hosts have no canonical config path; paste the generated MCP snippet into the host's config surface.",
    };
  }

  const checked = candidates.map((file) => {
    const content = readText(file);
    const scopedContent = hostScopedConfigText(host, content);
    return {
      file,
      exists: fs.existsSync(file),
      mentions_m1nd: content.includes("m1nd"),
      mentions_workspace_root: scopedContent.includes("M1ND_WORKSPACE_ROOT"),
      // JSON configs escape backslashes, so on Windows the raw path never
      // appears verbatim; match the JSON-encoded spelling as well.
      mentions_project_dir:
        scopedContent.includes(projectDir) ||
        scopedContent.includes(JSON.stringify(projectDir).slice(1, -1)),
      runtime_bindings: runtimeBindingsFromText(scopedContent, binary, packageVersion),
    };
  });
  const userScopePath = host === "claude" ? path.join(homeDir(), ".claude.json") : null;
  const configured = checked.some((candidate) => candidate.exists && candidate.mentions_m1nd);
  const configuredUserScope =
    userScopePath !== null &&
    checked.some((candidate) => candidate.file === userScopePath && candidate.exists && candidate.mentions_m1nd);
  const configuredProjectScope =
    checked.some((candidate) => candidate.file !== userScopePath && candidate.exists && candidate.mentions_m1nd);
  const workspaceConfigured = checked.some(
    (candidate) =>
      candidate.exists &&
      candidate.mentions_m1nd &&
      candidate.mentions_workspace_root &&
      candidate.mentions_project_dir
  );
  const anyPresent = checked.some((candidate) => candidate.exists);
  const runtimeBindings = checked.flatMap((candidate) => candidate.runtime_bindings);
  const selectedRuntimeConfigured = runtimeBindings.some((binding) => binding.matches_selected);
  const selectedRuntimeConfiguredCurrent = runtimeBindings.some(
    (binding) => binding.matches_selected && binding.current
  );
  const currentRuntimeConfigured = runtimeBindings.some((binding) => binding.current);
  const runtimeBindingStatus = !configured
    ? "unconfigured"
    : runtimeBindings.length === 0
      ? "unknown"
      : currentRuntimeConfigured
        ? "current"
        : "stale_or_missing";
  const configStatus = configured
    ? configuredUserScope && !configuredProjectScope
      ? "configured (user-scope)"
      : "configured"
    : anyPresent
      ? "present_without_m1nd"
      : "missing";
  return {
    status: configStatus,
    workspace_configured: workspaceConfigured,
    candidates: checked,
    expected_command: binary || defaultRuntimePath(),
    snippet_command: snippetCommand,
    runtime_bindings: runtimeBindings,
    runtime_binding_status: runtimeBindingStatus,
    selected_runtime_configured: selectedRuntimeConfigured,
    selected_runtime_configured_current: selectedRuntimeConfiguredCurrent,
    current_runtime_configured: currentRuntimeConfigured,
    note: configured
      ? "A config candidate mentions m1nd; workspace env and host rebind are still checked separately."
      : "No config candidate currently proves that this host is wired to m1nd.",
  };
}

function hostPathShadow(pathRuntimeCurrent, pathBinary, pathRuntimeText, config) {
  if (!pathBinary || !pathRuntimeText) {
    return {
      status: "absent",
      blocking: false,
      path_binary: pathBinary || null,
      path_version: pathRuntimeText,
      selected_runtime_configured_current: Boolean(config.selected_runtime_configured_current),
      message: "No m1nd-mcp binary was found on PATH.",
    };
  }
  if (pathRuntimeCurrent) {
    return {
      status: "current",
      blocking: false,
      path_binary: pathBinary,
      path_version: pathRuntimeText,
      selected_runtime_configured_current: Boolean(config.selected_runtime_configured_current),
      message: "The m1nd-mcp binary on PATH matches the package version.",
    };
  }
  if (config.selected_runtime_configured_current) {
    return {
      status: "shadow_warning",
      blocking: false,
      path_binary: pathBinary,
      path_version: pathRuntimeText,
      selected_runtime_configured_current: true,
      message: "PATH has a stale m1nd-mcp, but this host config points to the selected current runtime. Align PATH only for hosts that launch PATH.",
    };
  }
  return {
    status: "actionable",
    blocking: true,
    path_binary: pathBinary,
    path_version: pathRuntimeText,
    selected_runtime_configured_current: false,
    message: "PATH has a stale m1nd-mcp and this host does not prove an absolute current runtime binding.",
  };
}

function hostReadinessNonClaims() {
  return [
    "m1nd hosts status is read-only and does not mutate host configuration.",
    "m1nd hosts status does not prove that an already-open MCP host has rebound.",
    "m1nd hosts status does not refresh a host's cached MCP tool list.",
    "m1nd hosts status does not repair graph contents, ingest roots, or semantic retrieval.",
    "m1nd hosts status does not prove that every possible agent host is configured.",
  ];
}

function hostPlanNonClaims() {
  return [
    "m1nd hosts plan is read-only and does not mutate host configuration.",
    "m1nd hosts plan does not prove that a host has applied the generated snippet.",
    "m1nd hosts plan does not restart or rebind any active MCP host.",
    "m1nd hosts plan does not refresh a host's cached MCP tool list.",
    "m1nd hosts plan does not ingest workspaces, repair graph contents, or fix semantic retrieval.",
    "m1nd hosts plan does not write hook or doctrine files; apply --yes does.",
  ];
}

function hostApplyNonClaims() {
  return [
    "m1nd hosts apply does not prove that an already-open MCP host has rebound.",
    "m1nd hosts apply does not refresh a host's cached MCP tool list.",
    "m1nd hosts apply does not repair graph contents, ingest roots, or semantic retrieval.",
    "m1nd hosts apply does not align system PATH or update root-owned runtime binaries.",
    "m1nd hosts apply does not know every possible generic host config path.",
    "m1nd hosts apply does not write a host's settings.json hooks section; it prints the block for you to confirm.",
    "m1nd hosts apply is not production-grade unattended host management.",
  ];
}

function hostInstallCommand(host, projectDir) {
  if (host === "codex") return "m1nd install-skills codex";
  return `m1nd install-skills ${host} --project ${projectDir}`;
}

const SHIM_BIN = "m1nd-north-shim";
const DOCTRINE_FENCE_BEGIN = "<!-- BEGIN m1nd-orient (managed) -->";
const DOCTRINE_FENCE_END = "<!-- END m1nd-orient (managed) -->";

function doctrineGateBody() {
  return [
    "# m1nd — orient before you act",
    "",
    "BEFORE responding to the first user message or reading/editing any file, call the",
    "m1nd MCP tool `north` with the current task and treat the returned packet (binding",
    "trust, task anchors, prior memory, honest gaps) as ground truth for this session.",
    "If m1nd is not connected, run: m1nd agent first-minute --repo . --query \"orient\" --json",
    "Direct proof (source, tests, compiler/runtime) remains the final truth for behavior.",
    "",
  ].join("\n");
}

function doctrineManagedBlock() {
  return `${DOCTRINE_FENCE_BEGIN}\n${doctrineGateBody()}${DOCTRINE_FENCE_END}\n`;
}

function doctrineFileContent(host) {
  if (host === "cursor") {
    return `---\nalwaysApply: true\n---\n\n${doctrineGateBody()}`;
  }
  return doctrineGateBody();
}

function osGateOk(osGate, platform = process.platform) {
  return !osGate || osGate.includes(platform);
}

function hostRecipe(host, projectDir) {
  const home = homeDir();
  const doctrineHook = (event, configPath, matcher, owned, command, gotcha) => ({
    event,
    config_path: configPath,
    config_kind: "json",
    matcher: matcher || null,
    owned,
    command,
    gotcha: gotcha || null,
  });

  switch (host) {
    case "claude":
      return {
        tier: "A",
        matrix_section: "§3.1",
        os_gate: null,
        hook: doctrineHook(
          "SessionStart",
          path.join(home, ".claude", "settings.json"),
          "startup|resume",
          false,
          `${SHIM_BIN} --repo "$CLAUDE_PROJECT_DIR" --query "orient"`,
          "apply PRINTS the settings block; it never writes settings.json"
        ),
        doctrine: { path: path.join(projectDir, ".claude", "CLAUDE.md"), note: null },
        extra_note: null,
      };
    case "codex":
      return {
        tier: "A",
        matrix_section: "§3.2",
        os_gate: null,
        hook: doctrineHook(
          "SessionStart",
          path.join(home, ".codex", "hooks.json"),
          "startup|resume",
          true,
          `${SHIM_BIN} --repo "$CODEX_CWD" --query "orient"`,
          "also set [features] hooks=true in ~/.codex/config.toml; approve hooks once, never use --dangerously-bypass-hook-trust"
        ),
        doctrine: { path: path.join(projectDir, "AGENTS.md"), note: null },
        extra_note: null,
      };
    case "qwen":
      return {
        tier: "A",
        matrix_section: "§3.3",
        os_gate: null,
        hook: doctrineHook(
          "SessionStart",
          path.join(home, ".qwen", "settings.json"),
          null,
          true,
          `${SHIM_BIN} --repo "$PWD" --query "orient"`,
          "do not rename QWEN.md to AGENTS.md (bug #727); instructions field [unverified]"
        ),
        doctrine: { path: path.join(projectDir, "QWEN.md"), note: null },
        extra_note: null,
      };
    case "kiro":
      return {
        tier: "A",
        matrix_section: "§3.4",
        os_gate: null,
        hook: doctrineHook(
          "agentSpawn",
          path.join(projectDir, ".kiro", "hooks", "agentSpawn.json"),
          null,
          false,
          `m1nd agent first-minute --repo "$PWD" --query "orient" --json`,
          "render-bug #5372: injection works, display may not; agentSpawn hook lives in the host agent config"
        ),
        doctrine: { path: path.join(projectDir, ".kiro", "steering", "m1nd.md"), note: null },
        extra_note: null,
      };
    case "cline":
      return {
        tier: "A",
        matrix_section: "§3.4",
        os_gate: ["darwin", "linux"],
        hook: doctrineHook(
          "TaskStart",
          path.join(projectDir, ".clinerules"),
          null,
          false,
          `${SHIM_BIN} --repo "$PWD" --query "orient" --event TaskStart`,
          "macOS/Linux only; the TaskStart hook is host-managed, so it is printed to add by hand"
        ),
        doctrine: { path: path.join(projectDir, ".clinerules"), note: "the .clinerules file is the doctrine surface" },
        extra_note: null,
      };
    case "continue":
      return {
        tier: "A",
        matrix_section: "§3.5",
        os_gate: null,
        hook: doctrineHook(
          "SessionStart",
          path.join(projectDir, ".continue", "hooks.json"),
          null,
          true,
          `${SHIM_BIN} --repo "$PWD" --query "orient"`,
          "Continue does not auto-read AGENTS.md; additionalContext field name [unverified]"
        ),
        doctrine: { path: path.join(projectDir, ".continue", "rules", "00-m1nd.md"), note: null },
        extra_note: null,
      };
    case "grok":
      return {
        tier: "A",
        matrix_section: "§3.7",
        os_gate: null,
        hook: doctrineHook(
          "SessionStart",
          path.join(home, ".grok", "user-settings.json"),
          null,
          true,
          `${SHIM_BIN} --repo "$PWD" --query "orient"`,
          "official Grok Build has NO SessionStart — confirm the superagent-ai fork; .override.md wins"
        ),
        doctrine: { path: path.join(projectDir, "AGENTS.md"), note: null },
        extra_note: null,
      };
    case "gemini":
      return tierBRecipe("§4", path.join(projectDir, "GEMINI.md"), "instructions is the primary channel (rendered); doctrine reinforces");
    case "antigravity":
      return tierBRecipe("§4", path.join(home, ".gemini", "GEMINI.md"), "collision WARNING: Gemini CLI shares ~/.gemini/GEMINI.md (#16058); doctrine leaks between the two tools");
    case "cursor":
      return tierBRecipe("§4", path.join(projectDir, ".cursor", "rules", "00-m1nd-orient.mdc"), "alwaysApply:true; keep ASCII paths, do not mix globs with alwaysApply; sessionStart additional_context bug (no fix)");
    case "windsurf":
      return tierBRecipe("§4", path.join(projectDir, ".windsurf", "rules", "m1nd.md"), "also global_rules.md; Devin Desktop rebrand prefers .devin/rules/");
    case "zed":
      return tierBRecipe("§4", path.join(projectDir, "AGENTS.md"), "first-match-wins — ship exactly one; context_servers in settings.json");
    case "vscode":
      return tierBRecipe("§4", path.join(projectDir, ".github", "copilot-instructions.md"), ".vscode/mcp.json for MCP");
    case "opencode":
      return tierBRecipe("§4", path.join(projectDir, "AGENTS.md"), "opencode.json \"mcp\" for MCP");
    case "warp":
      return tierBRecipe("§4", path.join(projectDir, "WARP.md"), "ALL-CAPS; ~/.warp/.mcp.json for MCP; wins over AGENTS.md");
    case "trae":
      return tierBRecipe("§4", path.join(projectDir, ".trae", "project_rules.md"), ".trae/mcp.json for MCP");
    case "jetbrains":
      return tierBRecipe("§4", path.join(projectDir, ".junie", "AGENTS.md"), ".junie/mcp/mcp.json for MCP");
    case "amp":
      return tierBRecipe("§4", path.join(projectDir, "AGENTS.md"), "amp.mcpServers for MCP");
    case "goose":
      return tierBRecipe("§4", path.join(projectDir, ".goosehints"), "extensions for MCP");
    case "crush":
      return tierBRecipe("§4", path.join(projectDir, "CRUSH.md"), "MCP config; PreToolUse context is the D channel");
    case "aider":
      return tierBRecipe("§4", path.join(projectDir, "CONVENTIONS.md"), "via --read; no native MCP so B+D");
    default:
      return null;
  }
}

function tierBRecipe(matrixSection, doctrinePath, note) {
  return {
    tier: "B",
    matrix_section: matrixSection,
    os_gate: null,
    hook: null,
    doctrine: { path: doctrinePath, note: note || null },
    extra_note: note || null,
  };
}

function renderHookSnippet(recipe) {
  if (!recipe || !recipe.hook) return null;
  const hook = recipe.hook;
  const commandEntry = { type: "command", command: hook.command };
  if (recipe.matrix_section === "§3.2") {
    // codex hooks.json: top-level event array.
    const entry = { hooks: [commandEntry] };
    if (hook.matcher) entry.matcher = hook.matcher;
    return JSON.stringify({ [hook.event]: [entry] }, null, 2);
  }
  if (recipe.doctrine && recipe.hook.event === "agentSpawn") {
    // kiro: agentSpawn config shape.
    return JSON.stringify({ hooks: { [hook.event]: [{ command: hook.command }] } }, null, 2);
  }
  const entry = { hooks: [commandEntry] };
  if (hook.matcher) entry.matcher = hook.matcher;
  return JSON.stringify({ hooks: { [hook.event]: [entry] } }, null, 2);
}

function claudeSettingsBlock(command) {
  return JSON.stringify(
    {
      hooks: {
        SessionStart: [
          {
            matcher: "startup|resume",
            hooks: [{ type: "command", command }],
          },
        ],
      },
    },
    null,
    2
  );
}

function writeDoctrineFile(file, host) {
  ensureDir(path.dirname(file));
  const before = readText(file);
  if (!before.trim()) {
    fs.writeFileSync(file, doctrineFileContent(host));
    return { changed: true, file };
  }
  const block = doctrineManagedBlock();
  if (before.includes(DOCTRINE_FENCE_BEGIN) && before.includes(DOCTRINE_FENCE_END)) {
    const pattern = new RegExp(
      `${escapeRegExp(DOCTRINE_FENCE_BEGIN)}[\\s\\S]*?${escapeRegExp(DOCTRINE_FENCE_END)}\\n?`
    );
    if (before.includes(block)) return { changed: false, file };
    const after = before.replace(pattern, block);
    if (before === after) return { changed: false, file };
    fs.writeFileSync(file, after);
    return { changed: true, file };
  }
  if (/m1nd/i.test(before)) {
    // File already mentions m1nd but has no managed fence (e.g. our own body written
    // directly). Stay conservative and do not duplicate.
    return { changed: false, file };
  }
  // Foreign content: append a clearly-fenced managed block, preserving the original.
  const separator = before.endsWith("\n") ? "\n" : "\n\n";
  fs.writeFileSync(file, `${before}${separator}${block}`);
  return { changed: true, file };
}

function escapeRegExp(text) {
  return String(text).replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

function writeHookConfig(recipe, host) {
  const file = recipe.hook.config_path;
  const before = readText(file);
  const parsed = before.trim() ? safeJsonParse(before) : {};
  if (before.trim() && !parsed) {
    throw new Error(`cannot safely update invalid JSON hook config at ${file}`);
  }
  const config = parsed && typeof parsed === "object" && !Array.isArray(parsed) ? parsed : {};
  const command = recipe.hook.command;
  const commandEntry = { type: "command", command };
  const isM1ndEntry = (entry) =>
    entry &&
    Array.isArray(entry.hooks) &&
    entry.hooks.some(
      (inner) =>
        inner &&
        typeof inner.command === "string" &&
        (inner.command.includes(SHIM_BIN) || inner.command.includes("m1nd agent first-minute"))
    );

  if (recipe.matrix_section === "§3.2") {
    // codex: top-level SessionStart array.
    const event = recipe.hook.event;
    const existing = Array.isArray(config[event]) ? config[event] : [];
    const kept = existing.filter((entry) => !isM1ndEntry(entry));
    const entry = { hooks: [commandEntry] };
    if (recipe.hook.matcher) entry.matcher = recipe.hook.matcher;
    kept.push(entry);
    config[event] = kept;
  } else {
    // qwen / continue / grok: nested config.hooks[event] array.
    const event = recipe.hook.event;
    config.hooks =
      config.hooks && typeof config.hooks === "object" && !Array.isArray(config.hooks) ? config.hooks : {};
    const existing = Array.isArray(config.hooks[event]) ? config.hooks[event] : [];
    const kept = existing.filter((entry) => !isM1ndEntry(entry));
    const entry = { hooks: [commandEntry] };
    if (recipe.hook.matcher) entry.matcher = recipe.hook.matcher;
    kept.push(entry);
    config.hooks[event] = kept;
  }

  const after = `${JSON.stringify(config, null, 2)}\n`;
  ensureDir(path.dirname(file));
  if (before === after) return { changed: false, file };
  fs.writeFileSync(file, after);
  return { changed: true, file };
}

function hostStatus(args) {
  const hostSelection = args.host || args._[2] || "all";
  if (!HOSTS.has(hostSelection)) {
    throw new Error(`unsupported host '${hostSelection}'. Supported hosts: ${Array.from(HOSTS).join(", ")}`);
  }

  const selectedHosts = hostSelection === "all" ? HOST_LIST : [hostSelection];
  const projectDir = path.resolve(args.project || process.cwd());
  const packageVersion = readPackageVersion();
  const binary = args.binary ? path.resolve(args.binary) : findRuntimeBinary();
  const runtimeText = runtimeVersion(binary);
  const runtimeCurrent = Boolean(binary && runtimeText && runtimeText.includes(packageVersion));
  const pathBinary = which("m1nd-mcp");
  const pathRuntimeText = pathBinary ? runtimeVersion(pathBinary) : null;
  const pathRuntimeCurrent = Boolean(!pathRuntimeText || pathRuntimeText.includes(packageVersion));
  const workspaceRoot = process.env.M1ND_WORKSPACE_ROOT || null;
  const workspaceRootMatches = Boolean(workspaceRoot && path.resolve(workspaceRoot) === projectDir);

  const runtime = {
    package_version: packageVersion,
    binary: binary || null,
    version: runtimeText,
    current: runtimeCurrent,
    default_install_path: defaultRuntimePath(),
    path_binary: pathBinary || null,
    path_version: pathRuntimeText,
    path_runtime_current: pathRuntimeCurrent,
  };

  const workspace = {
    project_dir: projectDir,
    m1nd_workspace_root: workspaceRoot,
    matches_project: workspaceRootMatches,
    status: workspaceRoot ? (workspaceRootMatches ? "aligned" : "different") : "unset",
    recommendation: `Set M1ND_WORKSPACE_ROOT=${projectDir} in host MCP config when the host supports env vars.`,
  };

  const hosts = selectedHosts.map((host) => {
    const agentPack = agentPackStatusForHost(host, projectDir);
    const config = hostConfigStatus(host, projectDir, binary, packageVersion);
    const pathShadow = hostPathShadow(pathRuntimeCurrent, pathBinary, pathRuntimeText, config);
    const workspaceReady = workspace.status === "aligned" || Boolean(config.workspace_configured);
    const configReady = config.status === "configured" && workspaceReady;
    const readiness = runtimeCurrent && !pathShadow.blocking && agentPack.installed && configReady ? "ready" : "attention";
    const nextActions = [];
    const warnings = [];
    if (!runtimeCurrent) {
      nextActions.push("Run m1nd update status --json, then m1nd update plan/apply if the runtime is stale or missing.");
    }
    if (pathShadow.status === "actionable") {
      nextActions.push("Align the m1nd-mcp binary found on PATH or pass --binary to target the runtime this host launches.");
    }
    if (pathShadow.status === "shadow_warning") {
      warnings.push(pathShadow.message);
    }
    if (!agentPack.installed) {
      nextActions.push(`Run ${hostInstallCommand(host, projectDir)}.`);
    }
    if (config.status === "missing") {
      nextActions.push(`Run ${config.snippet_command} and add the snippet to one of the listed host config paths.`);
    }
    if (config.status === "present_without_m1nd") {
      nextActions.push(`Update the existing host config with ${config.snippet_command}.`);
    }
    if (config.status === "configured" && !config.workspace_configured && workspace.status !== "aligned") {
      nextActions.push(`Update the existing host config with ${config.snippet_command} so it carries M1ND_WORKSPACE_ROOT=${projectDir}.`);
    }
    if (workspace.status !== "aligned" && !config.workspace_configured) {
      nextActions.push(workspace.recommendation);
    }
    nextActions.push("Restart/rebind the host, then call trust_selftest or session_handshake before retrieval.");

    return {
      host,
      readiness,
      agent_pack: agentPack,
      config,
      workspace,
      path_shadow: pathShadow,
      warnings,
      host_rebind_proven: false,
      next_actions: nextActions,
    };
  });

  const uniqueNextActions = Array.from(new Set(hosts.flatMap((host) => host.next_actions)));
  return {
    schema: HOST_READINESS_SCHEMA,
    package_name: NPM_PACKAGE,
    package_version: packageVersion,
    project_dir: projectDir,
    host_selection: hostSelection,
    runtime,
    workspace,
    hosts,
    summary: {
      host_count: hosts.length,
      ready_count: hosts.filter((host) => host.readiness === "ready").length,
      attention_count: hosts.filter((host) => host.readiness !== "ready").length,
      overall_readiness: hosts.every((host) => host.readiness === "ready") ? "ready" : "attention",
      host_rebind_proven: false,
    },
    next_actions: uniqueNextActions,
    non_claims: hostReadinessNonClaims(),
  };
}

function hostPlan(args) {
  const status = hostStatus(args);
  const projectDir = status.project_dir;
  const binary = args.binary ? path.resolve(args.binary) : status.runtime.binary || defaultRuntimePath();
  const plans = status.hosts.map((host) => {
    const configCandidateFiles = host.config.candidates.map((candidate) => candidate.file);
    const recipe = hostRecipe(host.host, projectDir);
    const hook = recipe && recipe.hook
      ? {
          tier: recipe.tier,
          event: recipe.hook.event,
          config_path: recipe.hook.config_path,
          config_kind: recipe.hook.config_kind,
          matcher: recipe.hook.matcher,
          matrix_section: recipe.matrix_section,
          owned: recipe.hook.owned,
          snippet: renderHookSnippet(recipe),
          os_supported: osGateOk(recipe.os_gate),
          gotcha: recipe.hook.gotcha,
        }
      : {
          tier: recipe ? recipe.tier : "B",
          reason: "no session-start hook on this host",
          matrix_section: recipe ? recipe.matrix_section : null,
        };
    const doctrine = recipe
      ? {
          path: recipe.doctrine.path,
          matrix_section: recipe.matrix_section,
          note: recipe.doctrine.note,
          content_preview: doctrineGateBody().split("\n").slice(0, 3).join("\n"),
        }
      : null;
    const settingsBlock =
      recipe && recipe.hook && recipe.hook.owned === false && host.host === "claude"
        ? claudeSettingsBlock(recipe.hook.command)
        : null;
    return {
      host: host.host,
      readiness: host.readiness,
      read_only: true,
      hook,
      doctrine,
      settings_block: settingsBlock,
      install_agent_pack: {
        needed: !host.agent_pack.installed,
        command: hostInstallCommand(host.host, projectDir),
        target_root: host.agent_pack.target_root,
        rule_file: host.agent_pack.rule_file,
      },
      configure_mcp: {
        status: host.config.status,
        command: host.config.snippet_command,
        candidate_paths: configCandidateFiles,
        snippet: mcpConfig(host.host, binary, projectDir),
      },
      workspace_binding: {
        status: host.workspace.status,
        env: {
          M1ND_WORKSPACE_ROOT: projectDir,
        },
        reason: "Bind the host to the intended repository before trusting scoped retrieval.",
      },
      runtime: {
        binary,
        version: status.runtime.version,
        current: status.runtime.current,
        path_binary: status.runtime.path_binary,
        path_version: status.runtime.path_version,
        path_runtime_current: status.runtime.path_runtime_current,
        path_shadow: host.path_shadow,
      },
      rebind_steps: [
        "Run the install_agent_pack command if needed.",
        "Add or update the MCP config with the configure_mcp snippet.",
        "Ensure the host config carries M1ND_WORKSPACE_ROOT for the intended project.",
        "Restart or rebind the host MCP client, or open a fresh session.",
        "Call trust_selftest or session_handshake with the intended scope before retrieval.",
      ],
      verification: [
        `m1nd hosts status --host ${host.host} --project ${projectDir} --json`,
        "In the host MCP session: trust_selftest or session_handshake with the same scope.",
        "If retrieval is still blocked after a full-trust handshake, call recovery_playbook with the suspicious tool evidence.",
      ],
      host_rebind_proven: false,
    };
  });

  return {
    schema: HOST_REBIND_PLAN_SCHEMA,
    package_name: NPM_PACKAGE,
    package_version: status.package_version,
    project_dir: projectDir,
    host_selection: status.host_selection,
    read_only: true,
    status_summary: status.summary,
    plans,
    next_actions: Array.from(new Set(plans.flatMap((plan) => plan.rebind_steps))),
    non_claims: hostPlanNonClaims(),
  };
}

function canonicalHostConfigPath(host, projectDir) {
  switch (host) {
    case "codex":
      return path.join(homeDir(), ".codex", "config.toml");
    case "claude":
      return path.join(projectDir, ".claude", "mcp.json");
    case "gemini":
      return path.join(projectDir, ".gemini", "settings.json");
    case "antigravity":
      return path.join(projectDir, "mcp_config.json");
    default:
      return null;
  }
}

function removeTomlSections(text, sectionNames) {
  const lines = String(text || "").split(/\r?\n/);
  const kept = [];
  let skipping = false;
  for (const line of lines) {
    const section = line.match(/^\s*\[([^\]]+)\]\s*$/);
    if (section) {
      skipping = sectionNames.has(section[1]);
      if (skipping) continue;
    }
    if (!skipping) kept.push(line);
  }
  return kept.join("\n").trimEnd();
}

function writeCodexMcpConfig(file, binary, projectDir) {
  const before = readText(file);
  const snippet = mcpConfig("codex", binary, projectDir).trimEnd();
  const withoutM1nd = removeTomlSections(
    before,
    new Set(["mcp_servers.m1nd", "mcp_servers.m1nd.env"])
  );
  const after = `${withoutM1nd ? `${withoutM1nd}\n\n` : ""}${snippet}\n`;
  ensureDir(path.dirname(file));
  if (before === after) return { changed: false, file };
  fs.writeFileSync(file, after);
  return { changed: true, file };
}

function mcpServerEntry(binary, projectDir) {
  return {
    command: binary || findRuntimeBinary() || defaultRuntimePath(),
    args: ["--stdio", "--no-gui"],
    env: {
      M1ND_WORKSPACE_ROOT: path.resolve(projectDir),
    },
  };
}

function writeJsonMcpConfig(file, binary, projectDir) {
  const before = readText(file);
  const parsed = before.trim() ? safeJsonParse(before) : {};
  if (before.trim() && !parsed) {
    throw new Error(`cannot safely update invalid JSON config at ${file}`);
  }
  const config = parsed && typeof parsed === "object" && !Array.isArray(parsed) ? parsed : {};
  config.mcpServers = config.mcpServers && typeof config.mcpServers === "object" && !Array.isArray(config.mcpServers)
    ? config.mcpServers
    : {};
  config.mcpServers.m1nd = mcpServerEntry(binary, projectDir);
  const after = `${JSON.stringify(config, null, 2)}\n`;
  ensureDir(path.dirname(file));
  if (before === after) return { changed: false, file, parse_ok: Boolean(parsed || !before.trim()) };
  fs.writeFileSync(file, after);
  return { changed: true, file, parse_ok: true };
}

function writeHostConfig(host, file, binary, projectDir) {
  if (host === "codex") return writeCodexMcpConfig(file, binary, projectDir);
  return writeJsonMcpConfig(file, binary, projectDir);
}

function hostApply(args) {
  const hostSelection = args.host || args._[2] || "all";
  if (!HOSTS.has(hostSelection)) {
    throw new Error(`unsupported host '${hostSelection}'. Supported hosts: ${Array.from(HOSTS).join(", ")}`);
  }

  const selectedHosts = hostSelection === "all" ? HOST_LIST : [hostSelection];
  const projectDir = path.resolve(args.project || process.cwd());
  const binary = args.binary ? path.resolve(args.binary) : findRuntimeBinary() || defaultRuntimePath();
  const yes = Boolean(args.yes);
  const noSkills = Boolean(args["no-skills"]);
  const noConfig = Boolean(args["no-config"]);
  const noHooks = Boolean(args["no-hooks"]);
  const statusBefore = hostStatus({ ...args, host: hostSelection, project: projectDir, binary });
  const plannedActions = [];
  const appliedActions = [];
  const blockedActions = [];
  const changedFiles = [];
  const warnings = [];
  const hostResults = [];

  for (const hostName of selectedHosts) {
    const hostBefore = statusBefore.hosts.find((host) => host.host === hostName);
    const hostResult = {
      host: hostName,
      readiness_before: hostBefore ? hostBefore.readiness : "unknown",
      planned_actions: [],
      applied_actions: [],
      blocked_actions: [],
      changed_files: [],
    };

    if (!noSkills) {
      const skillAction = action("install-agent-pack", "agent-pack", `install ${hostName} agent pack`, {
        host: hostName,
        command: hostInstallCommand(hostName, projectDir),
      });
      plannedActions.push(skillAction);
      hostResult.planned_actions.push(skillAction);
      if (yes) {
        try {
          const installs = installSkills(hostName, projectDir);
          const applied = {
            id: skillAction.id,
            kind: skillAction.kind,
            host: hostName,
            ok: true,
            installed: installs.flatMap((entry) => entry.installed || []),
          };
          appliedActions.push(applied);
          hostResult.applied_actions.push(applied);
          for (const installed of applied.installed) {
            if (!changedFiles.includes(installed)) changedFiles.push(installed);
            if (!hostResult.changed_files.includes(installed)) hostResult.changed_files.push(installed);
          }
        } catch (error) {
          const blocked = action("install-agent-pack-failed", "agent-pack", `failed to install ${hostName} agent pack`, {
            host: hostName,
            error: error instanceof Error ? error.message : String(error),
          });
          blockedActions.push(blocked);
          hostResult.blocked_actions.push(blocked);
        }
      }
    } else {
      const blocked = action("agent-pack-disabled", "agent-pack", "agent pack install disabled by --no-skills", {
        host: hostName,
      });
      blockedActions.push(blocked);
      hostResult.blocked_actions.push(blocked);
    }

    const configFile = canonicalHostConfigPath(hostName, projectDir);
    if (noConfig) {
      const blocked = action("config-disabled", "config", "MCP config write disabled by --no-config", {
        host: hostName,
      });
      blockedActions.push(blocked);
      hostResult.blocked_actions.push(blocked);
    } else if (!configFile) {
      const blocked = action("config-manual", "config", "generic host config path is manual; paste the snippet into the target host", {
        host: hostName,
        snippet: mcpConfig("generic", binary, projectDir),
      });
      blockedActions.push(blocked);
      hostResult.blocked_actions.push(blocked);
    } else {
      const configAction = action("write-mcp-config", "config", `write ${hostName} MCP config with M1ND_WORKSPACE_ROOT`, {
        host: hostName,
        file: configFile,
        command: binary,
        workspace_root: projectDir,
      });
      plannedActions.push(configAction);
      hostResult.planned_actions.push(configAction);
      if (yes) {
        try {
          const writeResult = writeHostConfig(hostName, configFile, binary, projectDir);
          const applied = {
            id: configAction.id,
            kind: configAction.kind,
            host: hostName,
            ok: true,
            file: writeResult.file,
            changed: writeResult.changed,
            workspace_root: projectDir,
          };
          appliedActions.push(applied);
          hostResult.applied_actions.push(applied);
          if (writeResult.changed) {
            changedFiles.push(writeResult.file);
            hostResult.changed_files.push(writeResult.file);
          }
        } catch (error) {
          const blocked = action("write-mcp-config-failed", "config", `failed to write ${hostName} MCP config`, {
            host: hostName,
            file: configFile,
            error: error instanceof Error ? error.message : String(error),
          });
          blockedActions.push(blocked);
          hostResult.blocked_actions.push(blocked);
        }
      }
    }

    const recipe = hostRecipe(hostName, projectDir);
    if (recipe) {
      const doctrineAction = action("write-doctrine", "doctrine", `write ${hostName} doctrine ${recipe.doctrine.path}`, {
        host: hostName,
        file: recipe.doctrine.path,
      });
      plannedActions.push(doctrineAction);
      hostResult.planned_actions.push(doctrineAction);
      if (yes) {
        try {
          const writeResult = writeDoctrineFile(recipe.doctrine.path, hostName);
          const applied = {
            id: doctrineAction.id,
            kind: doctrineAction.kind,
            host: hostName,
            ok: true,
            file: writeResult.file,
            changed: writeResult.changed,
          };
          appliedActions.push(applied);
          hostResult.applied_actions.push(applied);
          if (writeResult.changed) {
            if (!changedFiles.includes(writeResult.file)) changedFiles.push(writeResult.file);
            if (!hostResult.changed_files.includes(writeResult.file)) hostResult.changed_files.push(writeResult.file);
          }
        } catch (error) {
          const blocked = action("write-doctrine-failed", "doctrine", `failed to write ${hostName} doctrine`, {
            host: hostName,
            file: recipe.doctrine.path,
            error: error instanceof Error ? error.message : String(error),
          });
          blockedActions.push(blocked);
          hostResult.blocked_actions.push(blocked);
        }
      }
    }

    if (recipe && recipe.hook && !noHooks) {
      if (!osGateOk(recipe.os_gate)) {
        const blocked = action("hook-unsupported-os", "hook", `${hostName} hook unsupported on ${process.platform}`, {
          host: hostName,
          os_gate: recipe.os_gate,
          config_path: recipe.hook.config_path,
        });
        blockedActions.push(blocked);
        hostResult.blocked_actions.push(blocked);
      } else if (recipe.hook.owned === false) {
        const printAction = action("print-hook-block", "hook", `${hostName} hook must be added by hand`, {
          host: hostName,
          config_path: recipe.hook.config_path,
          event: recipe.hook.event,
          snippet: renderHookSnippet(recipe),
          settings_block: hostName === "claude" ? claudeSettingsBlock(recipe.hook.command) : null,
          note:
            hostName === "claude"
              ? "add this to your settings.json hooks section"
              : "add this to your host agent/hook config",
        });
        plannedActions.push(printAction);
        hostResult.planned_actions.push(printAction);
        if (yes) {
          appliedActions.push(printAction);
          hostResult.applied_actions.push(printAction);
        }
      } else {
        const hookAction = action("write-hook-config", "hook", `write ${hostName} ${recipe.hook.event} hook ${recipe.hook.config_path}`, {
          host: hostName,
          file: recipe.hook.config_path,
          event: recipe.hook.event,
        });
        plannedActions.push(hookAction);
        hostResult.planned_actions.push(hookAction);
        if (yes) {
          try {
            const writeResult = writeHookConfig(recipe, hostName);
            const applied = {
              id: hookAction.id,
              kind: hookAction.kind,
              host: hostName,
              ok: true,
              file: writeResult.file,
              changed: writeResult.changed,
              event: recipe.hook.event,
            };
            appliedActions.push(applied);
            hostResult.applied_actions.push(applied);
            if (writeResult.changed) {
              if (!changedFiles.includes(writeResult.file)) changedFiles.push(writeResult.file);
              if (!hostResult.changed_files.includes(writeResult.file)) hostResult.changed_files.push(writeResult.file);
            }
          } catch (error) {
            const blocked = action("write-hook-config-failed", "hook", `failed to write ${hostName} hook config`, {
              host: hostName,
              file: recipe.hook.config_path,
              error: error instanceof Error ? error.message : String(error),
            });
            blockedActions.push(blocked);
            hostResult.blocked_actions.push(blocked);
          }
        }
      }
    } else if (recipe && recipe.hook && noHooks) {
      const blocked = action("hook-disabled", "hook", "hook write disabled by --no-hooks", { host: hostName });
      blockedActions.push(blocked);
      hostResult.blocked_actions.push(blocked);
    }

    hostResults.push(hostResult);
  }

  if (!yes) {
    warnings.push("dry-run only; re-run with --yes to install agent packs and write known host MCP configs.");
  }

  let statusAfter = null;
  if (yes) {
    statusAfter = hostStatus({ ...args, host: hostSelection, project: projectDir, binary });
  }

  return {
    schema: HOST_APPLY_SCHEMA,
    package_name: NPM_PACKAGE,
    package_version: readPackageVersion(),
    host_selection: hostSelection,
    project_dir: projectDir,
    binary,
    dry_run: !yes,
    status_before: statusBefore.summary,
    status_after: statusAfter ? statusAfter.summary : null,
    hosts: hostResults,
    planned_actions: plannedActions,
    applied_actions: appliedActions,
    blocked_actions: blockedActions,
    changed_files: Array.from(new Set(changedFiles)),
    warnings,
    requires_host_rebind: true,
    host_rebind_proven: false,
    next_actions: [
      "Restart or rebind each affected MCP host, or open a fresh host session.",
      "Call trust_selftest or session_handshake with the intended scope before retrieval.",
      "If retrieval is still blocked after full trust, call recovery_playbook with the suspicious tool evidence.",
    ],
    non_claims: hostApplyNonClaims(),
  };
}

function doctor() {
  const pack = assertPackShape();
  const binary = findRuntimeBinary();
  const binaryVersion = runtimeVersion(binary);
  const packageVersion = readPackageVersion();
  const codexSkillRoot = path.join(homeDir(), ".codex", "skills");
  const codexSkillsInstalled =
    fs.existsSync(path.join(codexSkillRoot, "m1nd-first", "SKILL.md")) &&
    fs.existsSync(path.join(codexSkillRoot, "m1nd-operator", "SKILL.md"));

  const result = {
    schema: "m1nd-npm-doctor-v0",
    package_version: packageVersion,
    package_root: PACKAGE_ROOT,
    pack_ok: pack.ok,
    missing_pack_files: pack.missing,
    runtime: {
      platform: process.platform,
      arch: process.arch,
      binary: binary || null,
      version: binaryVersion,
      default_install_path: defaultRuntimePath(),
      visible_on_path_or_env: Boolean(binary),
      hint: binary
        ? "m1nd-mcp is visible. Use m1nd smoke from a repo checkout for a live MCP smoke."
        : "m1nd-mcp is not visible. Build from source or install the native runtime before wiring MCP hosts.",
    },
    codex: {
      skill_root: codexSkillRoot,
      skills_installed: codexSkillsInstalled,
    },
    next_actions: [],
  };

  if (!pack.ok) {
    result.next_actions.push("Reinstall or rebuild the npm package; required agent-pack files are missing.");
  }
  if (!binary) {
    // The npm user's door is the verified installer, not a source checkout.
    // Measured on a stranger install (2026-08-02): with no runtime present,
    // doctor sent the newcomer to `cargo build` — a path most npm users cannot
    // take — while the working command sat one line away.
    result.next_actions.push("Install the native runtime: m1nd update apply --yes (verified download; needs cosign on PATH)");
    result.next_actions.push(`Or from a source checkout: cargo build --release -p m1nd-mcp, then copy ${runtimeBinaryName()} to ${defaultRuntimePath()}`);
  }
  if (binary && (!binaryVersion || !binaryVersion.includes(packageVersion))) {
    result.next_actions.push(`Runtime version ${binaryVersion || "unknown"} does not match package ${packageVersion}; run m1nd update apply --yes to install the matching runtime, then rebind the host.`);
  }
  if (!codexSkillsInstalled) {
    result.next_actions.push("For Codex: m1nd install-skills codex");
  }
  if (result.next_actions.length === 0) {
    result.next_actions.push("Run trust_selftest in your MCP host, then ingest your repo.");
  }
  return result;
}

function runCommand(command, args, options = {}) {
  const result = spawnSync(command, args, {
    cwd: options.cwd,
    encoding: "utf8",
    stdio: options.stdio || "pipe",
    timeout: options.timeout,
    killSignal: options.killSignal || "SIGKILL",
  });
  return {
    command,
    args,
    cwd: options.cwd || process.cwd(),
    status: result.status,
    ok: !result.error && result.status === 0,
    error: result.error ? result.error.message : null,
    stdout: result.stdout || "",
    stderr: result.stderr || "",
  };
}

// The default probe budget for a runtime that is already warm on disk.
const RUNTIME_VERSION_PROBE_MS = 1500;
// The budget for a binary written SECONDS ago: the shipped runtime is ~70 MB
// and its first exec pages in cold. Measured 2026-08-02: first run 1.66s,
// second run 0.00s — so the 1.5s default expired on exactly the one call that
// matters, the verified installer asking "which version did I just install?".
// It answered `version-check-timeout`, which matches no version string, and a
// correct install reported `runtime-version-mismatch-after-install`: the
// updater refusing the very binary it had verified, staged and installed.
const RUNTIME_VERSION_PROBE_AFTER_INSTALL_MS = 30000;

function runtimeVersion(binary, timeoutMs = RUNTIME_VERSION_PROBE_MS) {
  if (!binary || !fs.existsSync(binary)) return null;
  if (process.env.M1ND_TEST_RUNTIME_VERSION_BY_PATH) {
    const versions = safeJsonParse(process.env.M1ND_TEST_RUNTIME_VERSION_BY_PATH) || {};
    const candidates = [binary, path.resolve(binary), realPathOrResolved(binary)].filter(Boolean);
    for (const candidate of candidates) {
      if (Object.prototype.hasOwnProperty.call(versions, candidate)) {
        return versions[candidate];
      }
    }
  }
  if (process.env.M1ND_TEST_RUNTIME_VERSION) return process.env.M1ND_TEST_RUNTIME_VERSION;
  const result = runCommand(binary, ["--version"], { timeout: timeoutMs });
  if (result.error && result.error.includes("ETIMEDOUT")) return "version-check-timeout";
  if (!result.ok) return null;
  return result.stdout.trim() || null;
}

function selfUpdateNonClaims() {
  return [
    "m1nd update does not refresh an active MCP host's cached tool list by itself.",
    "m1nd update does not prove that a currently open host has rebound to the new runtime.",
    "m1nd update does not repair graph contents.",
    "m1nd update does not correct ingest roots or workspace selection.",
    "m1nd update does not fix semantic retrieval by itself.",
    "m1nd update does not update all agent hosts.",
    "m1nd update is not production-grade unattended auto-update.",
  ];
}

function versionFromText(text) {
  const match = String(text || "").match(/\b(\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?)\b/);
  return match ? match[1] : null;
}

function parseSemver(version) {
  const text = versionFromText(version);
  if (!text) return null;
  const match = text.match(/^(\d+)\.(\d+)\.(\d+)(?:-([0-9A-Za-z.-]+))?$/);
  if (!match) return null;
  return {
    raw: text,
    major: Number(match[1]),
    minor: Number(match[2]),
    patch: Number(match[3]),
    prerelease: match[4] || "",
  };
}

function comparePrerelease(left, right) {
  if (left === right) return 0;
  if (!left) return 1;
  if (!right) return -1;
  const leftParts = left.split(".");
  const rightParts = right.split(".");
  const length = Math.max(leftParts.length, rightParts.length);
  for (let index = 0; index < length; index += 1) {
    const a = leftParts[index];
    const b = rightParts[index];
    if (a === b) continue;
    if (a === undefined) return -1;
    if (b === undefined) return 1;
    const an = Number(a);
    const bn = Number(b);
    if (Number.isInteger(an) && Number.isInteger(bn)) return an < bn ? -1 : 1;
    return a < b ? -1 : 1;
  }
  return 0;
}

function compareSemver(left, right) {
  const a = parseSemver(left);
  const b = parseSemver(right);
  if (!a || !b) return null;
  for (const key of ["major", "minor", "patch"]) {
    if (a[key] !== b[key]) return a[key] < b[key] ? -1 : 1;
  }
  return comparePrerelease(a.prerelease, b.prerelease);
}

function safeJsonParse(text) {
  try {
    return JSON.parse(text);
  } catch (_) {
    return null;
  }
}

function normalizeChannel(channel) {
  // Default to `latest`: a fresh install must land on the shipped package version. The
  // `beta` dist-tag trails far behind (an old 0.9.x) — defaulting to it dragged new users
  // onto a months-old prerelease. Opt into beta explicitly with --channel beta.
  const normalized = channel || "latest";
  if (!["beta", "latest"].includes(normalized)) {
    throw new Error(`unsupported update channel '${normalized}'. Supported channels: beta, latest`);
  }
  return normalized;
}

function readNpmRegistry(channel) {
  if (process.env.M1ND_TEST_NPM_VIEW_JSON) {
    const payload = safeJsonParse(process.env.M1ND_TEST_NPM_VIEW_JSON) || {};
    const tags = payload["dist-tags"] || payload.distTags || {};
    const version = payload.version || null;
    return {
      ok: true,
      package: NPM_PACKAGE,
      dist_tags: tags,
      version,
      latest_version: tags[channel] || version,
      source: "M1ND_TEST_NPM_VIEW_JSON",
      error: null,
    };
  }

  const npm = trustedNodePackageManager();
  if (!npm) {
    return {
      ok: false,
      package: NPM_PACKAGE,
      dist_tags: {},
      version: null,
      latest_version: null,
      source: "trusted-npm-unavailable",
      error: "fixed-path node/npm pair unavailable; updater will not execute an ambient PATH package manager",
    };
  }
  const tagsResult = runCommand(npm.node, [...npm.prefix, "view", NPM_PACKAGE, "dist-tags", "--json"], { timeout: 7000 });
  const versionResult = runCommand(npm.node, [...npm.prefix, "view", NPM_PACKAGE, "version", "--json"], { timeout: 7000 });
  const tags = tagsResult.ok ? safeJsonParse(tagsResult.stdout) || {} : {};
  const parsedVersion = versionResult.ok ? safeJsonParse(versionResult.stdout) : null;
  const version = typeof parsedVersion === "string" ? parsedVersion : null;
  return {
    ok: tagsResult.ok || versionResult.ok,
    package: NPM_PACKAGE,
    dist_tags: tags,
    version,
    latest_version: tags[channel] || version,
    source: "npm-view",
    error: tagsResult.ok || versionResult.ok ? null : (tagsResult.stderr || versionResult.stderr || tagsResult.error || versionResult.error || "").trim(),
  };
}

function readCrateVersion(crateName = "m1nd-mcp") {
  if (process.env.M1ND_TEST_CRATE_VERSION) {
    return {
      ok: true,
      crate: crateName,
      version: process.env.M1ND_TEST_CRATE_VERSION,
      source: "M1ND_TEST_CRATE_VERSION",
      error: null,
    };
  }
  return {
    ok: false,
    crate: crateName,
    version: null,
    source: "cargo-fallback-disabled",
    error: "unverified Cargo fallback is disabled; only a signed release candidate may update the runtime",
  };
}

function githubReleaseAssetName(platform = process.platform, arch = process.arch) {
  if (platform === "darwin" && arch === "arm64") return "m1nd-mcp-macos-aarch64";
  if (platform === "darwin" && arch === "x64") return "m1nd-mcp-macos-x86_64";
  if (platform === "linux" && arch === "x64") return "m1nd-mcp-linux-x86_64";
  if (platform === "win32" && arch === "x64") return "m1nd-mcp-windows-x86_64.exe";
  return null;
}

function githubReleaseTargetName(platform = process.platform, arch = process.arch) {
  if (platform === "darwin" && arch === "arm64") return "macos-aarch64";
  if (platform === "darwin" && arch === "x64") return "macos-x86_64";
  if (platform === "linux" && arch === "x64") return "linux-x86_64";
  if (platform === "win32" && arch === "x64") return "windows-x86_64";
  return null;
}

function githubReleaseAssetUrl(version, platform = process.platform, arch = process.arch) {
  const asset = githubReleaseAssetName(platform, arch);
  if (!asset || !version) return null;
  return `https://github.com/maxkle1nz/m1nd/releases/download/v${version}/${asset}`;
}

function githubReleaseAvailability(
  version,
  platform = process.platform,
  arch = process.arch,
  testDependencies = null
) {
  const asset = githubReleaseAssetName(platform, arch);
  const url = githubReleaseAssetUrl(version, platform, arch);
  if (!asset || !url) {
    return {
      ok: false,
      available: false,
      asset: null,
      url: null,
      source: "platform-map",
      error: `no v0 release asset is mapped for ${platform}-${arch}`,
    };
  }
  if (testDependencies && testDependencies.releaseDirectory) {
    const fixture = path.join(testDependencies.releaseDirectory, asset);
    const available = fs.existsSync(fixture) && fs.statSync(fixture).isFile();
    return {
      ok: true,
      available,
      asset,
      url,
      source: "test-release-directory",
      error: available ? null : `test release raw asset is missing: ${asset}`,
    };
  }
  if (process.env.M1ND_TEST_GITHUB_RELEASE_AVAILABLE) {
    const available = process.env.M1ND_TEST_GITHUB_RELEASE_AVAILABLE !== "false";
    return {
      ok: true,
      available,
      asset,
      url,
      source: "M1ND_TEST_GITHUB_RELEASE_AVAILABLE",
      error: available ? null : "test override reported unavailable",
    };
  }
  if (platform === "win32") {
    return {
      ok: false,
      available: false,
      asset,
      url,
      source: "windows-phase-2",
      error: `m1nd ${version} does not ship a Windows binary; Windows support is phase-2`,
    };
  }
  const curl = trustedUpdateTool("curl");
  if (!curl) {
    return {
      ok: false,
      available: false,
      asset,
      url,
      source: "curl-missing",
      error: "curl not found; cannot probe GitHub release asset",
    };
  }
  const result = runCommand(
    curl,
    [
      "-fsIL",
      "--proto",
      "=https",
      "--proto-redir",
      "=https",
      "--max-redirs",
      "5",
      "--connect-timeout",
      "5",
      "--write-out",
      "%{url_effective}",
      "--output",
      os.devNull,
      url,
    ],
    { timeout: 8000 }
  );
  const effectiveUrl = result.stdout.trim();
  const transportAllowed = result.ok && allowedReleaseTransportUrl(effectiveUrl);
  return {
    ok: transportAllowed,
    available: transportAllowed,
    asset,
    url,
    source: "github-release-head",
    error: transportAllowed
      ? null
      : result.ok
        ? `release redirect escaped the accepted HTTPS host policy: ${effectiveUrl || "missing effective URL"}`
        : (result.stderr || result.error || "").trim(),
  };
}

function updateTestOverrides(testDependencies = null) {
  const active = [];
  if (testDependencies && testDependencies.releaseDirectory) active.push("release-transport-directory");
  if (testDependencies && testDependencies.cosignPath) active.push("cosign-executable-path");
  return {
    active: active.length > 0,
    release_transport:
      testDependencies && testDependencies.releaseDirectory
        ? "local-test-directory"
        : "github-release-https",
    verifier_source:
      testDependencies && testDependencies.cosignPath
        ? "explicit-test-executable"
        : "trusted-fixed-path",
    overrides: active,
  };
}

function updateStatePath() {
  return process.env.M1ND_UPDATE_STATE_PATH || path.join(homeDir(), ".m1nd", "update-state.json");
}

function updateBackupPath(targetBinary, beforeVersion) {
  const safeVersion = versionFromText(beforeVersion) || "unknown";
  const stamp = new Date().toISOString().replace(/[:.]/g, "-");
  const backupRoot = process.env.M1ND_UPDATE_BACKUP_DIR || path.join(homeDir(), ".m1nd", "backups");
  return path.join(backupRoot, `${path.basename(targetBinary)}-${safeVersion}-${stamp}`);
}

function sha256File(file) {
  const digest = crypto.createHash("sha256");
  digest.update(fs.readFileSync(file));
  return digest.digest("hex");
}

function fsyncFile(file) {
  // Windows FlushFileBuffers requires a writable handle; "r+" works everywhere.
  const descriptor = fs.openSync(file, "r+");
  try {
    fs.fsyncSync(descriptor);
  } finally {
    fs.closeSync(descriptor);
  }
}

function fsyncDirectory(directory) {
  let descriptor = null;
  try {
    descriptor = fs.openSync(directory, "r");
    fs.fsyncSync(descriptor);
  } catch (_) {
    // Windows and some filesystems do not permit fsync on directory handles.
    // The file itself is still fsynced before the same-directory rename.
  } finally {
    if (descriptor !== null) fs.closeSync(descriptor);
  }
}

function writeJsonAtomic(file, value) {
  const directory = path.dirname(file);
  ensureDir(directory);
  const temporary = path.join(
    directory,
    `.${path.basename(file)}.${process.pid}.${crypto.randomBytes(6).toString("hex")}.tmp`
  );
  let descriptor = null;
  try {
    descriptor = fs.openSync(temporary, "wx", 0o600);
    fs.writeFileSync(descriptor, `${JSON.stringify(value, null, 2)}\n`);
    fs.fsyncSync(descriptor);
    fs.closeSync(descriptor);
    descriptor = null;
    fs.renameSync(temporary, file);
    fsyncDirectory(directory);
  } finally {
    if (descriptor !== null) fs.closeSync(descriptor);
    fs.rmSync(temporary, { force: true });
  }
}

function selectTargetVersion(packageVersion, registryVersion) {
  if (!registryVersion) {
    return {
      target_version: packageVersion,
      registry_lag: false,
      reason: "npm registry version unavailable; using local package version",
    };
  }
  const comparison = compareSemver(packageVersion, registryVersion);
  if (comparison !== null && comparison > 0) {
    return {
      target_version: packageVersion,
      registry_lag: true,
      reason: `npm registry channel is behind local package (${registryVersion} < ${packageVersion})`,
    };
  }
  return {
    target_version: registryVersion,
    registry_lag: false,
    reason: "using npm registry channel version",
  };
}

function action(id, kind, description, extra = {}) {
  return { id, kind, description, ...extra };
}

function buildSelfUpdateProof(args, command = "check", testDependencies = null) {
  const channel = normalizeChannel(args.channel);
  const packageVersion = readPackageVersion();
  const registry = readNpmRegistry(channel);
  const selected = selectTargetVersion(packageVersion, registry.latest_version);
  const targetVersion = selected.target_version;
  const requestedBinary = args.binary ? path.resolve(args.binary) : findRuntimeBinary();
  const binary = requestedBinary && fs.existsSync(requestedBinary) ? requestedBinary : null;
  const targetBinary = path.resolve(args.binary || defaultRuntimePath());
  const runtimeText = runtimeVersion(binary);
  const runtimeParsedVersion = versionFromText(runtimeText);
  const pathBinary = which("m1nd-mcp");
  const pathRuntimeText = pathBinary ? runtimeVersion(pathBinary) : null;
  const pack = assertPackShape();
  const crate = readCrateVersion("m1nd-mcp");
  const release = githubReleaseAvailability(
    targetVersion,
    process.platform,
    process.arch,
    testDependencies
  );
  const testOverrides = updateTestOverrides(testDependencies);
  const plannedActions = [];
  const blockedActions = [];
  const staleSurfaces = [];
  let unknown = false;

  if (registry.latest_version) {
    const npmComparison = compareSemver(packageVersion, registry.latest_version);
    if (npmComparison !== null && npmComparison < 0 && !args["no-npm"]) {
      staleSurfaces.push("npm-package");
      blockedActions.push(action("npm-signed-artifact-required", "npm", `refuse registry-only install of ${NPM_PACKAGE}@${registry.latest_version}`, {
        package: NPM_PACKAGE,
        channel,
        target_version: registry.latest_version,
        source: "signed-release-required",
        reason: "the npm tarball digest is not yet bound to the signed release candidate",
        next_action: "publish a candidate-bound npm tarball and install those exact verified bytes",
      }));
    } else if (npmComparison !== null && npmComparison < 0 && args["no-npm"]) {
      staleSurfaces.push("npm-package");
      blockedActions.push(action("npm-disabled", "npm", "npm package update disabled by --no-npm", {
        target_version: registry.latest_version,
      }));
    } else if (selected.registry_lag) {
      blockedActions.push(action("npm-registry-lag", "npm", selected.reason, {
        registry_version: registry.latest_version,
        local_package_version: packageVersion,
      }));
    }
  } else {
    unknown = true;
    blockedActions.push(action("npm-registry-unknown", "npm", "could not resolve npm registry channel version", {
      error: registry.error,
    }));
  }

  if (!binary) {
    staleSurfaces.push("runtime");
    if (args["no-runtime"]) {
      blockedActions.push(action("runtime-disabled", "runtime", "runtime install disabled by --no-runtime", {
        target_binary: targetBinary,
      }));
    } else {
      const install = runtimeInstallAction(release, crate, targetVersion, targetBinary, "runtime missing");
      (install.id === "runtime-install-github-release" ? plannedActions : blockedActions).push(install);
    }
  } else if (!runtimeParsedVersion || (targetVersion && !runtimeText.includes(targetVersion))) {
    staleSurfaces.push("runtime");
    if (args["no-runtime"]) {
      blockedActions.push(action("runtime-disabled", "runtime", "runtime install disabled by --no-runtime", {
        current_binary: binary,
        current_version: runtimeText,
        target_binary: targetBinary,
      }));
    } else {
      const install = runtimeInstallAction(release, crate, targetVersion, targetBinary, "runtime stale or unknown");
      (install.id === "runtime-install-github-release" ? plannedActions : blockedActions).push(install);
    }
  }

  const npmWillChange = plannedActions.some((planned) => planned.kind === "npm");
  if (!pack.ok || npmWillChange) {
    staleSurfaces.push("agent-pack");
    if (args["no-skills"]) {
      blockedActions.push(action("skills-disabled", "agent-pack", "agent pack refresh disabled by --no-skills", {
        missing_pack_files: pack.missing,
      }));
    } else {
      plannedActions.push(action("skills-refresh", "agent-pack", "refresh Codex agent skills from current package", {
        host: "codex",
        missing_pack_files: pack.missing,
        reason: npmWillChange ? "npm package update may carry agent-pack changes" : "agent-pack files are missing from package",
      }));
    }
  }

  if (
    pathBinary &&
    binary &&
    path.resolve(pathBinary) !== path.resolve(binary) &&
    pathRuntimeText &&
    targetVersion &&
    !pathRuntimeText.includes(targetVersion)
  ) {
    blockedActions.push(action("path-runtime-stale", "runtime", "m1nd-mcp on PATH reports a different version than the selected managed runtime", {
      path_binary: pathBinary,
      path_runtime_version: pathRuntimeText,
      selected_binary: binary,
      selected_runtime_version: runtimeText,
      target_version: targetVersion,
      suggested_action: `re-run with --binary ${pathBinary} if this PATH runtime should be updated too`,
    }));
  }

  const runtimeWillChange = plannedActions.some((planned) => planned.kind === "runtime");
  if (runtimeWillChange && args["no-kill"]) {
    blockedActions.push(action("kill-disabled", "process", "runtime process stop disabled by --no-kill"));
  } else if (runtimeWillChange && command === "apply") {
    plannedActions.push(action("stop-runtime-processes", "process", "stop visible m1nd-mcp processes after runtime replacement"));
  }

  let installState = "current";
  if (!binary) {
    installState = "missing";
  } else if (staleSurfaces.length > 1) {
    installState = "mixed";
  } else if (staleSurfaces.length === 1) {
    installState = "stale";
  } else if (unknown || !runtimeText) {
    installState = "unknown";
  }

  return {
    schema: SELF_UPDATE_SCHEMA,
    command,
    package_name: NPM_PACKAGE,
    package_version: packageVersion,
    runtime_version: runtimeText,
    runtime_parsed_version: runtimeParsedVersion,
    latest_version: registry.latest_version || null,
    target_version: targetVersion,
    channel,
    install_state: installState,
    registry,
    crates: {
      m1nd_mcp: crate,
    },
    github_release: release,
    test_overrides: testOverrides,
    runtime: {
      platform: process.platform,
      arch: process.arch,
      binary: binary || null,
      target_binary: targetBinary,
      default_install_path: defaultRuntimePath(),
      path_binary: pathBinary || null,
      path_version: pathRuntimeText,
      path_matches_selected: Boolean(pathBinary && binary && path.resolve(pathBinary) === path.resolve(binary)),
    },
    agent_pack: {
      ok: pack.ok,
      missing: pack.missing,
    },
    planned_actions: plannedActions,
    applied_actions: [],
    blocked_actions: blockedActions,
    requires_host_rebind: plannedActions.some((planned) => ["npm", "runtime", "agent-pack", "process"].includes(planned.kind)),
    dry_run: true,
    non_claims: [
      ...selfUpdateNonClaims(),
      ...(testOverrides.active
        ? ["This proof used explicit local test transport or verifier seams and is not a live GitHub/Sigstore receipt."]
        : []),
    ],
    next_actions: [],
  };
}

function runtimeInstallAction(release, crate, targetVersion, targetBinary, reason) {
  if (release.available) {
    return action("runtime-install-github-release", "runtime", `install native runtime ${targetVersion} from GitHub release`, {
      reason,
      source: "github-release",
      url: release.url,
      asset: release.asset,
      target_binary: targetBinary,
      target_version: targetVersion,
    });
  }
  return action("runtime-release-unavailable", "runtime", `signed release candidate unavailable for native runtime ${targetVersion}; refusing unverified fallback`, {
    reason,
    source: "signed-release-required",
    candidate_verification: "required-before-any-runtime-effect",
    rollback_available: false,
    crate: crate.crate,
    crate_version: crate.version,
    target_binary: targetBinary,
    target_version: targetVersion,
    release_error: release.error,
    next_action: "publish or restore the exact signed GitHub release candidate, then retry",
  });
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

function requireUnicodeScalarString(value, location) {
  for (let index = 0; index < value.length; index += 1) {
    const unit = value.charCodeAt(index);
    if (unit >= 0xd800 && unit <= 0xdbff) {
      const next = value.charCodeAt(index + 1);
      if (!(next >= 0xdc00 && next <= 0xdfff)) {
        throw new Error(`unpaired high surrogate at ${location}`);
      }
      index += 1;
    } else if (unit >= 0xdc00 && unit <= 0xdfff) {
      throw new Error(`unpaired low surrogate at ${location}`);
    }
  }
  return value;
}

function parseIntegerJson(text, description = "JSON") {
  let index = 0;

  function fail(detail) {
    throw new Error(`invalid ${description} at byte ${index}: ${detail}`);
  }

  function whitespace() {
    while (index < text.length && /[\t\n\r ]/.test(text[index])) index += 1;
  }

  function stringValue() {
    const start = index;
    index += 1;
    while (index < text.length) {
      const character = text[index];
      if (character === '"') {
        index += 1;
        try {
          return requireUnicodeScalarString(
            JSON.parse(text.slice(start, index)),
            `${description} string`
          );
        } catch (error) {
          fail(error.message);
        }
      }
      if (character === "\\") {
        index += 2;
      } else {
        index += 1;
      }
    }
    fail("unterminated string");
  }

  function numberValue() {
    const rest = text.slice(index);
    const match = rest.match(/^-?(?:0|[1-9][0-9]*)(?:\.[0-9]+)?(?:[eE][+-]?[0-9]+)?/);
    if (!match) fail("invalid number");
    const source = match[0];
    index += source.length;
    if (/[.eE]/.test(source)) {
      throw new Error(`invalid ${description}: non-integer JSON number refused: ${source}`);
    }
    const exact = BigInt(source);
    if (exact >= BigInt(Number.MIN_SAFE_INTEGER) && exact <= BigInt(Number.MAX_SAFE_INTEGER)) {
      return Number(source);
    }
    return exact;
  }

  function arrayValue() {
    index += 1;
    const result = [];
    whitespace();
    if (text[index] === "]") {
      index += 1;
      return result;
    }
    while (index < text.length) {
      result.push(value());
      whitespace();
      if (text[index] === "]") {
        index += 1;
        return result;
      }
      if (text[index] !== ",") fail("expected ',' or ']' in array");
      index += 1;
      whitespace();
    }
    fail("unterminated array");
  }

  function objectValue() {
    index += 1;
    const result = {};
    whitespace();
    if (text[index] === "}") {
      index += 1;
      return result;
    }
    while (index < text.length) {
      if (text[index] !== '"') fail("object key must be a string");
      const key = stringValue();
      if (Object.prototype.hasOwnProperty.call(result, key)) {
        fail(`duplicate object key refused: ${JSON.stringify(key)}`);
      }
      whitespace();
      if (text[index] !== ":") fail("expected ':' after object key");
      index += 1;
      const item = value();
      Object.defineProperty(result, key, {
        value: item,
        configurable: true,
        enumerable: true,
        writable: true,
      });
      whitespace();
      if (text[index] === "}") {
        index += 1;
        return result;
      }
      if (text[index] !== ",") fail("expected ',' or '}' in object");
      index += 1;
      whitespace();
    }
    fail("unterminated object");
  }

  function literal(expected, result) {
    if (text.slice(index, index + expected.length) !== expected) fail(`expected ${expected}`);
    index += expected.length;
    return result;
  }

  function value() {
    whitespace();
    const character = text[index];
    if (character === "{") return objectValue();
    if (character === "[") return arrayValue();
    if (character === '"') return stringValue();
    if (character === "t") return literal("true", true);
    if (character === "f") return literal("false", false);
    if (character === "n") return literal("null", null);
    if (character === "-" || /[0-9]/.test(character || "")) return numberValue();
    fail("unexpected token");
  }

  try {
    const result = value();
    whitespace();
    if (index !== text.length) fail("trailing content");
    return result;
  } catch (error) {
    if (String(error.message).startsWith(`invalid ${description}`)) throw error;
    throw new Error(`invalid ${description}: ${error.message}`);
  }
}

function canonicalJsonV1(value) {
  function encode(candidate, location) {
    if (candidate === null) return "null";
    if (typeof candidate === "string") {
      requireUnicodeScalarString(candidate, location);
      return JSON.stringify(candidate);
    }
    if (typeof candidate === "boolean") {
      return JSON.stringify(candidate);
    }
    if (typeof candidate === "bigint") return candidate.toString(10);
    if (typeof candidate === "number") {
      if (!Number.isSafeInteger(candidate)) {
        throw new Error(`canonical JSON number at ${location} is not a safe integer`);
      }
      return JSON.stringify(candidate);
    }
    if (Array.isArray(candidate)) {
      return `[${candidate.map((item, index) => encode(item, `${location}[${index}]`)).join(",")}]`;
    }
    if (candidate && typeof candidate === "object") {
      const prototype = Object.getPrototypeOf(candidate);
      if (prototype !== Object.prototype && prototype !== null) {
        throw new Error(`canonical JSON value at ${location} is not a plain object`);
      }
      return `{${Object.keys(candidate)
        .map((key) => requireUnicodeScalarString(key, `${location}.<key>`))
        // Rust String/BTreeMap ordering is UTF-8 byte ordering.  JavaScript's
        // default UTF-16 sort diverges for astral keys versus high BMP keys.
        .sort((left, right) => Buffer.compare(Buffer.from(left, "utf8"), Buffer.from(right, "utf8")))
        .map((key) => `${JSON.stringify(key)}:${encode(candidate[key], `${location}.${key}`)}`)
        .join(",")}}`;
    }
    throw new Error(`unsupported canonical JSON value at ${location}: ${typeof candidate}`);
  }
  return encode(value, "$");
}

function domainSeparatedDigest(domain, value) {
  const domainBytes = Buffer.from(domain, "utf8");
  const payload = Buffer.from(canonicalJsonV1(value), "utf8");
  const domainLength = Buffer.alloc(8);
  const payloadLength = Buffer.alloc(8);
  domainLength.writeBigUInt64BE(BigInt(domainBytes.length));
  payloadLength.writeBigUInt64BE(BigInt(payload.length));
  return crypto
    .createHash("sha256")
    .update(CANONICAL_DIGEST_PREFIX)
    .update(domainLength)
    .update(domainBytes)
    .update(payloadLength)
    .update(payload)
    .digest("hex");
}

const CANONICAL_HEX_64_RE = /^[0-9A-Fa-f]{64}$/;
const CANONICAL_GATE_IDS = Array.from({ length: 11 }, (_unused, index) => `G${index}`);
const CANONICAL_GATE_VERDICTS = new Set(["PASS", "FAIL", "NOT_RUN", "NOT_PROVEN"]);
const CANONICAL_FINDING_SEVERITIES = new Set(["P0", "P1", "P2", "P3", "Info"]);
const CANONICAL_FINDING_STATUSES = new Set(["OPEN", "CLOSED"]);
const CANONICAL_ACTIVE_MODES = new Set(["HUMAN_GATED", "POLICY_AUTONOMOUS", "FULL_AUTONOMY"]);

function requireExactFields(value, fields, description) {
  requireCandidateObject(value, description);
  const expected = [...fields].sort();
  const actual = Object.keys(value).sort();
  if (JSON.stringify(actual) !== JSON.stringify(expected)) {
    throw new Error(`${description} fields differ: expected=${expected.join(",")}, actual=${actual.join(",")}`);
  }
  return value;
}

function requireCanonicalText(value, field, trim = true) {
  if (typeof value !== "string" || (trim ? value.trim().length === 0 : value.length === 0)) {
    throw new Error(`required field ${field} is empty or not text`);
  }
  return value;
}

function requireCanonicalDigest(value, field) {
  if (typeof value !== "string" || !CANONICAL_HEX_64_RE.test(value)) {
    throw new Error(`required digest ${field} is not 64 hexadecimal characters`);
  }
  return value;
}

function requireCanonicalU64(value, field) {
  const integer = typeof value === "bigint" ? value : Number.isSafeInteger(value) ? BigInt(value) : null;
  if (integer === null || integer < 0n || integer > 18446744073709551615n) {
    throw new Error(`${field} must be a u64 integer`);
  }
  return integer;
}

function requireCanonicalI32OrNull(value, field) {
  if (value === null) return null;
  if (!Number.isInteger(value) || value < -2147483648 || value > 2147483647) {
    throw new Error(`${field} must be null or an i32 integer`);
  }
  return value;
}

function requireCanonicalMap(value, field, digestValues) {
  requireCandidateObject(value, field);
  const names = Object.keys(value);
  if (names.length === 0) throw new Error(`required map ${field} is empty`);
  for (const name of names) {
    requireCanonicalText(name, `${field}.key`);
    if (digestValues) requireCanonicalDigest(value[name], `${field}.${name}`);
    else requireCanonicalText(value[name], `${field}.${name}`);
  }
  return value;
}

function validateCanonicalFinding(value) {
  const finding = requireExactFields(
    value,
    ["finding_id", "severity", "status", "statement", "evidence_digest"],
    "release finding"
  );
  requireCanonicalText(finding.finding_id, "finding_id");
  if (!CANONICAL_FINDING_SEVERITIES.has(finding.severity)) {
    throw new Error(`invalid finding severity: ${String(finding.severity)}`);
  }
  if (!CANONICAL_FINDING_STATUSES.has(finding.status)) {
    throw new Error(`invalid finding status: ${String(finding.status)}`);
  }
  requireCanonicalText(finding.statement, "finding.statement");
  requireCanonicalDigest(finding.evidence_digest, "finding.evidence_digest");
  return finding;
}

function validateCanonicalFindings(value) {
  if (!Array.isArray(value)) throw new Error("findings must be an array");
  const ids = new Set();
  for (const finding of value.map(validateCanonicalFinding)) {
    if (ids.has(finding.finding_id)) throw new Error(`duplicate finding id ${finding.finding_id}`);
    ids.add(finding.finding_id);
  }
  return value;
}

const CANONICAL_CANDIDATE_CORE_FIELDS = [
  "repo_commits",
  "artifact_digests",
  "schema_policy_versions",
  "tool_catalog_digest",
  "safety_kernel_digest",
  "previous_governance_runtime_digest",
  "constitution_epoch_digest",
  "autonomy_epoch_grants_digest",
  "independence_quorum_policy_digest",
  "intended_active_mode",
  "compatibility_manifest_digest",
  "rollback_plan_digest",
  "harness_fixture_threat_digests",
  "build_environment_digest",
  "built_at",
];

function validateCanonicalCandidateCore(value) {
  const core = requireExactFields(value, CANONICAL_CANDIDATE_CORE_FIELDS, "release candidate core");
  requireCanonicalMap(core.repo_commits, "repo_commits", false);
  requireCanonicalMap(core.artifact_digests, "artifact_digests", true);
  requireCanonicalMap(core.schema_policy_versions, "schema_policy_versions", false);
  requireCanonicalMap(core.harness_fixture_threat_digests, "harness_fixture_threat_digests", true);
  for (const field of [
    "tool_catalog_digest",
    "safety_kernel_digest",
    "previous_governance_runtime_digest",
    "constitution_epoch_digest",
    "autonomy_epoch_grants_digest",
    "independence_quorum_policy_digest",
    "compatibility_manifest_digest",
    "rollback_plan_digest",
    "build_environment_digest",
  ]) {
    requireCanonicalDigest(core[field], field);
  }
  if (!CANONICAL_ACTIVE_MODES.has(core.intended_active_mode)) {
    throw new Error(`invalid intended_active_mode: ${String(core.intended_active_mode)}`);
  }
  requireCanonicalU64(core.built_at, "built_at");
  return core;
}

function validateCanonicalCandidate(value) {
  const candidate = requireExactFields(
    value,
    ["schema", "core", "candidate_digest", "provenance_signature"],
    "canonical release candidate"
  );
  if (candidate.schema !== CANONICAL_RELEASE_CANDIDATE_SCHEMA) {
    throw new Error(`unexpected canonical release candidate schema: ${String(candidate.schema)}`);
  }
  const core = validateCanonicalCandidateCore(candidate.core);
  requireCanonicalDigest(candidate.candidate_digest, "candidate_digest");
  // Exact Rust structural law: non-empty opaque bytes; no prefix is required.
  requireCanonicalText(candidate.provenance_signature, "provenance_signature", false);
  const expected = domainSeparatedDigest(CANONICAL_RELEASE_CANDIDATE_DOMAIN, core);
  if (candidate.candidate_digest !== expected) {
    throw new Error(`canonical candidate digest mismatch: expected ${expected}, got ${candidate.candidate_digest}`);
  }
  return candidate;
}

function validateCanonicalCompatibility(value) {
  const manifest = requireExactFields(
    value,
    ["schema", "version", "commit", "source_ref", "targets"],
    "canonical release compatibility manifest"
  );
  if (manifest.schema !== CANONICAL_COMPATIBILITY_SCHEMA) {
    throw new Error(`unexpected release compatibility schema: ${String(manifest.schema)}`);
  }
  if (!/^[0-9]+\.[0-9]+\.[0-9]+(?:-[0-9A-Za-z.-]+)?$/.test(String(manifest.version))) {
    throw new Error("canonical compatibility version is invalid");
  }
  if (!/^[0-9a-f]{40}$/.test(String(manifest.commit))) {
    throw new Error("canonical compatibility commit must be a full lowercase SHA-1");
  }
  if (manifest.source_ref !== `refs/tags/v${manifest.version}`) {
    throw new Error("canonical compatibility source_ref does not match its version");
  }
  if (!Array.isArray(manifest.targets) || manifest.targets.length === 0) {
    throw new Error("canonical compatibility targets must be a non-empty array");
  }
  const seen = new Set();
  for (const target of manifest.targets) {
    requireExactFields(target, ["target", "asset", "sha256", "size_bytes"], "canonical compatibility target");
    requireCanonicalText(target.target, "compatibility.target");
    if (!/^[a-z0-9_-]+$/.test(target.target)) {
      throw new Error(`canonical compatibility target is invalid: ${String(target.target)}`);
    }
    requireCanonicalText(target.asset, "compatibility.asset");
    const expectedAsset = `m1nd-mcp-${target.target}${target.target.startsWith("windows-") ? ".exe" : ""}`;
    if (target.asset !== expectedAsset) {
      throw new Error(`canonical compatibility asset ${String(target.asset)} does not match ${expectedAsset}`);
    }
    requireCanonicalDigest(target.sha256, "compatibility.sha256");
    const size = requireCanonicalU64(target.size_bytes, "compatibility.size_bytes");
    if (size === 0n) throw new Error("compatibility.size_bytes must be positive");
    if (seen.has(target.target)) throw new Error(`duplicate compatibility target ${target.target}`);
    seen.add(target.target);
  }
  return manifest;
}

function validateCanonicalRollback(value) {
  const plan = requireExactFields(
    value,
    ["schema", "version", "commit", "source_ref", "runtime_bindings", "activation", "rollback"],
    "canonical release rollback plan"
  );
  if (plan.schema !== "m1nd-release-rollback-plan-v1") {
    throw new Error(`unexpected release rollback schema: ${String(plan.schema)}`);
  }
  if (!/^[0-9]+\.[0-9]+\.[0-9]+(?:-[0-9A-Za-z.-]+)?$/.test(String(plan.version))) {
    throw new Error("canonical rollback version is invalid");
  }
  if (!/^[0-9a-f]{40}$/.test(String(plan.commit))) {
    throw new Error("canonical rollback commit must be a full lowercase SHA-1");
  }
  if (plan.source_ref !== `refs/tags/v${plan.version}`) {
    throw new Error("canonical rollback source_ref does not match its version");
  }
  if (!Array.isArray(plan.runtime_bindings) || plan.runtime_bindings.length === 0) {
    throw new Error("canonical rollback plan requires runtime bindings");
  }
  const seen = new Set();
  for (const binding of plan.runtime_bindings) {
    requireExactFields(
      binding,
      ["archive", "archive_member", "artifact_smoke_receipt", "raw_binary", "runtime_sha256", "size_bytes", "target"],
      "canonical rollback runtime binding"
    );
    requireCanonicalText(binding.target, "rollback.target");
    if (!/^[a-z0-9_-]+$/.test(binding.target)) {
      throw new Error(`canonical rollback target is invalid: ${String(binding.target)}`);
    }
    if (seen.has(binding.target)) throw new Error(`duplicate canonical rollback target ${binding.target}`);
    seen.add(binding.target);
    const windows = binding.target.startsWith("windows-");
    const expectedArchive = `m1nd-mcp-${binding.target}.${windows ? "zip" : "tar.gz"}`;
    const expectedMember = windows ? "m1nd-mcp.exe" : "m1nd-mcp";
    const expectedRaw = `m1nd-mcp-${binding.target}${windows ? ".exe" : ""}`;
    const expectedReceipt = `GATE-ARTIFACT-SMOKE-${binding.target}.json`;
    if (binding.archive !== expectedArchive) throw new Error(`canonical rollback archive mismatch for ${binding.target}`);
    if (binding.archive_member !== expectedMember) throw new Error(`canonical rollback archive member mismatch for ${binding.target}`);
    if (binding.raw_binary !== expectedRaw) throw new Error(`canonical rollback raw runtime mismatch for ${binding.target}`);
    if (binding.artifact_smoke_receipt !== expectedReceipt) {
      throw new Error(`canonical rollback smoke receipt mismatch for ${binding.target}`);
    }
    requireCanonicalDigest(binding.runtime_sha256, "rollback.runtime_sha256");
    const size = requireCanonicalU64(binding.size_bytes, "rollback.size_bytes");
    if (size === 0n) throw new Error("rollback.size_bytes must be positive");
  }
  const activation = requireExactFields(plan.activation, ["automatic", "command"], "canonical activation plan");
  if (activation.automatic !== false || activation.command !== "m1nd update apply --yes") {
    throw new Error("canonical activation must remain explicit and non-automatic");
  }
  const rollback = requireExactFields(
    plan.rollback,
    ["automatic", "command", "requires_local_state_schema", "source"],
    "canonical rollback action"
  );
  if (rollback.automatic !== false || rollback.command !== "m1nd update rollback") {
    throw new Error("canonical rollback must remain explicit and non-automatic");
  }
  if (rollback.requires_local_state_schema !== UPDATE_STATE_SCHEMA) {
    throw new Error("canonical rollback state schema drifted");
  }
  if (rollback.source !== "pre-activation local runtime backup") {
    throw new Error("canonical rollback source drifted");
  }
  return plan;
}

function validateCanonicalOperationalPair(compatibility, rollback) {
  for (const field of ["version", "commit", "source_ref"]) {
    if (compatibility[field] !== rollback[field]) {
      throw new Error(`compatibility and rollback ${field} differ`);
    }
  }
  const compatibleTargets = new Map(compatibility.targets.map((target) => [target.target, target]));
  const rollbackTargets = new Map(rollback.runtime_bindings.map((binding) => [binding.target, binding]));
  if (
    compatibleTargets.size !== rollbackTargets.size ||
    [...compatibleTargets.keys()].some((target) => !rollbackTargets.has(target))
  ) {
    throw new Error("compatibility and rollback target sets differ");
  }
  for (const [target, compatible] of compatibleTargets) {
    const binding = rollbackTargets.get(target);
    if (
      binding.raw_binary !== compatible.asset ||
      binding.runtime_sha256 !== compatible.sha256 ||
      requireCanonicalU64(binding.size_bytes, "rollback.size_bytes") !==
        requireCanonicalU64(compatible.size_bytes, "compatibility.size_bytes")
    ) {
      throw new Error(`compatibility and rollback runtime bytes differ for ${target}`);
    }
  }
}

function validateCanonicalGateReceipt(value) {
  const receipt = requireExactFields(
    value,
    ["schema", "core", "receipt_id", "receipt_digest", "signature"],
    "canonical gate receipt"
  );
  if (receipt.schema !== CANONICAL_GATE_RECEIPT_SCHEMA) throw new Error("invalid canonical gate receipt schema");
  const core = requireExactFields(
    receipt.core,
    [
      "candidate_digest", "gate_id", "custody_floor", "spec_version", "metric_spec_digest",
      "harness_fixture_digest", "environment_digest", "provider_id", "provider_key_version",
      "input_digests", "command", "started_at", "ended_at", "exit_code", "verdict",
      "findings", "artifact_digests",
    ],
    "canonical gate receipt core"
  );
  requireCanonicalDigest(core.candidate_digest, "candidate_digest");
  if (!CANONICAL_GATE_IDS.includes(core.gate_id)) throw new Error(`invalid gate_id: ${String(core.gate_id)}`);
  requireCanonicalText(core.custody_floor, "custody_floor");
  if (!CANONICAL_RATIFIED_CUSTODY_FLOORS.has(core.custody_floor)) {
    throw new Error(`custody_floor ${String(core.custody_floor)} is outside the ratified custody-floor set`);
  }
  requireCanonicalText(core.spec_version, "spec_version");
  if (core.metric_spec_digest !== null) requireCanonicalDigest(core.metric_spec_digest, "metric_spec_digest");
  requireCanonicalDigest(core.harness_fixture_digest, "harness_fixture_digest");
  requireCanonicalDigest(core.environment_digest, "environment_digest");
  requireCanonicalText(core.provider_id, "provider_id");
  requireCanonicalText(core.provider_key_version, "provider_key_version");
  requireCanonicalMap(core.input_digests, "input_digests", true);
  requireCanonicalText(core.command, "command");
  const started = requireCanonicalU64(core.started_at, "started_at");
  const ended = requireCanonicalU64(core.ended_at, "ended_at");
  if (ended < started) throw new Error("invalid gate time window");
  const exitCode = requireCanonicalI32OrNull(core.exit_code, "exit_code");
  if (!CANONICAL_GATE_VERDICTS.has(core.verdict)) throw new Error(`invalid gate verdict: ${String(core.verdict)}`);
  if (core.verdict === "PASS" && exitCode !== 0) throw new Error("PASS requires exit_code=0");
  if (core.verdict === "NOT_RUN" && exitCode !== null) throw new Error("NOT_RUN cannot claim an exit code");
  validateCanonicalFindings(core.findings);
  requireCanonicalMap(core.artifact_digests, "artifact_digests", true);
  requireCanonicalDigest(receipt.receipt_digest, "receipt_digest");
  if (receipt.receipt_id !== `gate:${receipt.receipt_digest}`) throw new Error("canonical gate receipt_id mismatch");
  requireCanonicalText(receipt.signature, "signature", false);
  const expected = domainSeparatedDigest(CANONICAL_GATE_RECEIPT_SCHEMA, core);
  if (receipt.receipt_digest !== expected) throw new Error("canonical gate receipt digest mismatch");
  return receipt;
}

function validateCanonicalReviewReceipt(value) {
  const receipt = requireExactFields(
    value,
    ["schema", "core", "receipt_id", "receipt_digest", "signature"],
    "canonical independent review receipt"
  );
  if (receipt.schema !== CANONICAL_REVIEW_RECEIPT_SCHEMA) throw new Error("invalid canonical review schema");
  const core = requireExactFields(
    receipt.core,
    [
      "candidate_digest", "threat_matrix_digest", "provider_id", "provider_model_version",
      "provider_key_version", "reviewed_inputs_digest", "binding_changes", "started_at",
      "ended_at", "verdict", "findings",
    ],
    "canonical independent review core"
  );
  requireCanonicalDigest(core.candidate_digest, "candidate_digest");
  requireCanonicalDigest(core.threat_matrix_digest, "threat_matrix_digest");
  requireCanonicalText(core.provider_id, "provider_id");
  requireCanonicalText(core.provider_model_version, "provider_model_version");
  requireCanonicalText(core.provider_key_version, "provider_key_version");
  requireCanonicalDigest(core.reviewed_inputs_digest, "reviewed_inputs_digest");
  if (!Array.isArray(core.binding_changes) || !core.binding_changes.every((entry) => typeof entry === "string")) {
    throw new Error("binding_changes must be an array of strings");
  }
  const started = requireCanonicalU64(core.started_at, "started_at");
  const ended = requireCanonicalU64(core.ended_at, "ended_at");
  if (ended < started) throw new Error("invalid independent review time window");
  if (!CANONICAL_GATE_VERDICTS.has(core.verdict)) throw new Error(`invalid review verdict: ${String(core.verdict)}`);
  validateCanonicalFindings(core.findings);
  requireCanonicalDigest(receipt.receipt_digest, "receipt_digest");
  if (receipt.receipt_id !== `iar:${receipt.receipt_digest}`) throw new Error("canonical review receipt_id mismatch");
  requireCanonicalText(receipt.signature, "signature", false);
  const expected = domainSeparatedDigest(CANONICAL_REVIEW_RECEIPT_SCHEMA, core);
  if (receipt.receipt_digest !== expected) throw new Error("canonical independent review digest mismatch");
  return receipt;
}

function hasCanonicalOpenP0P1(findings) {
  return findings.some((finding) => finding.status === "OPEN" && ["P0", "P1"].includes(finding.severity));
}

function validateCanonicalEvidenceSet(value) {
  const evidence = requireExactFields(
    value,
    ["schema", "contract_status", "candidate", "gate_receipts", "independent_review"],
    "canonical evidence-set JSON extension"
  );
  if (evidence.schema !== CANONICAL_EVIDENCE_SET_EXTENSION_SCHEMA) {
    throw new Error("invalid evidence-set JSON extension schema");
  }
  if (evidence.contract_status !== STRUCTURAL_RELEASE_STATUS) {
    throw new Error("evidence-set does not disclose structural-only validation");
  }
  const candidate = validateCanonicalCandidate(evidence.candidate);
  const review = validateCanonicalReviewReceipt(evidence.independent_review);
  if (review.core.candidate_digest !== candidate.candidate_digest) throw new Error("review candidate mismatch");
  if (review.core.verdict !== "PASS") throw new Error("independent review is not PASS");
  if (hasCanonicalOpenP0P1(review.core.findings)) throw new Error("independent review has open P0/P1");
  if (!Array.isArray(evidence.gate_receipts)) throw new Error("gate_receipts must be an array");
  const observed = new Set();
  for (const receipt of evidence.gate_receipts.map(validateCanonicalGateReceipt)) {
    const gate = receipt.core.gate_id;
    if (receipt.core.candidate_digest !== candidate.candidate_digest) throw new Error(`${gate} candidate mismatch`);
    if (observed.has(gate)) throw new Error(`duplicate gate ${gate}`);
    observed.add(gate);
    if (receipt.core.verdict !== "PASS") throw new Error(`${gate} is not PASS`);
    if (hasCanonicalOpenP0P1(receipt.core.findings)) throw new Error(`${gate} has open P0/P1`);
  }
  const missing = CANONICAL_GATE_IDS.filter((gate) => !observed.has(gate));
  if (missing.length > 0) throw new Error(`missing gates: ${missing.join(",")}`);
  return evidence;
}

function verifyCanonicalReleaseVectors(file) {
  const vectors = parseIntegerJson(fs.readFileSync(file, "utf8"), "canonical release vectors");
  if (vectors.schema !== "m1nd-release-cross-language-vectors-v1") throw new Error("invalid vector schema");
  if (vectors.canonicalization_version !== CANONICALIZATION_VERSION) throw new Error("canonicalization version drifted");
  if (vectors.digest_prefix_hex !== CANONICAL_DIGEST_PREFIX.toString("hex")) throw new Error("digest prefix drifted");
  requireExactFields(
    vectors.artifact_digest_keys,
    ["compatibility", "release_artifact_prefix", "release_asset_prefix", "rollback"],
    "artifact digest key vectors"
  );
  if (
    vectors.artifact_digest_keys.compatibility !== CANONICAL_COMPATIBILITY_ARTIFACT_KEY ||
    vectors.artifact_digest_keys.rollback !== CANONICAL_ROLLBACK_ARTIFACT_KEY ||
    vectors.artifact_digest_keys.release_asset_prefix !== CANONICAL_RELEASE_ASSET_PREFIX ||
    vectors.artifact_digest_keys.release_artifact_prefix !== CANONICAL_RELEASE_ARTIFACT_PREFIX
  ) {
    throw new Error("artifact digest key vectors drifted");
  }
  for (const vector of vectors.canonical_cases) {
    if (canonicalJsonV1(vector.value) !== vector.canonical_json) throw new Error(`canonical text mismatch: ${vector.name}`);
    if (domainSeparatedDigest(vector.domain, vector.value) !== vector.digest) throw new Error(`canonical digest mismatch: ${vector.name}`);
  }
  for (const vector of vectors.refusal_cases) {
    let refused = false;
    try {
      parseIntegerJson(vector.json, vector.name);
    } catch (_error) {
      refused = true;
    }
    if (!refused) throw new Error(`refusal vector accepted: ${vector.name}`);
  }
  const compatibility = vectors.operational_manifests.compatibility;
  validateCanonicalCompatibility(compatibility);
  const compatibilityDigest = sha256Text(canonicalJsonV1(compatibility));
  if (compatibilityDigest !== vectors.operational_manifests.compatibility_sha256) {
    throw new Error("compatibility vector digest mismatch");
  }
  const rollback = vectors.operational_manifests.rollback;
  validateCanonicalRollback(rollback);
  validateCanonicalOperationalPair(compatibility, rollback);
  const rollbackDigest = sha256Text(canonicalJsonV1(rollback));
  if (rollbackDigest !== vectors.operational_manifests.rollback_sha256) {
    throw new Error("rollback vector digest mismatch");
  }
  const vectorCore = vectors.evidence_set.candidate.core;
  if (
    vectorCore.artifact_digests[CANONICAL_COMPATIBILITY_ARTIFACT_KEY] !==
      vectorCore.compatibility_manifest_digest ||
    vectorCore.artifact_digests[CANONICAL_ROLLBACK_ARTIFACT_KEY] !==
      vectorCore.rollback_plan_digest
  ) {
    throw new Error("operational artifact key vectors drifted");
  }
  validateCanonicalEvidenceSet(vectors.evidence_set);
  return { ok: true, status: STRUCTURAL_RELEASE_STATUS };
}

function sha256Text(text) {
  return crypto.createHash("sha256").update(text).digest("hex");
}

function fileSha256OrNull(file) {
  if (!fs.existsSync(file)) return null;
  if (!fs.statSync(file).isFile()) throw new Error(`runtime target is not a regular file: ${file}`);
  return sha256File(file);
}

function requireCandidateObject(value, description) {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    throw new Error(`${description} must be a JSON object`);
  }
  return value;
}

function requireCandidateDigest(value, description) {
  if (typeof value !== "string" || !SHA256_RE.test(value)) {
    throw new Error(`${description} must be a lowercase SHA-256 digest`);
  }
  return value;
}

function requireCandidateSize(value, description) {
  if (!Number.isSafeInteger(value) || value <= 0) {
    throw new Error(`${description} must be a positive safe integer`);
  }
  return value;
}

function releaseCertificateIdentity(version) {
  return `https://github.com/${RELEASE_REPOSITORY}/.github/workflows/${RELEASE_WORKFLOW}@refs/tags/v${version}`;
}

function releaseFileUrl(planned, name) {
  const expectedAssetUrl = githubReleaseAssetUrl(planned.target_version);
  if (planned.url !== expectedAssetUrl) {
    throw new Error(`release asset URL does not match the exact repository/tag/target contract: ${planned.url}`);
  }
  return `${planned.url.slice(0, planned.url.lastIndexOf("/") + 1)}${name}`;
}

function releaseDownloadLimit(name, planned) {
  if (Object.prototype.hasOwnProperty.call(RELEASE_DOWNLOAD_LIMITS, name)) {
    return RELEASE_DOWNLOAD_LIMITS[name];
  }
  if (name === planned.asset) return RELEASE_DOWNLOAD_LIMITS.runtime;
  throw new Error(`release file has no bounded download policy: ${name}`);
}

function allowedReleaseTransportUrl(value) {
  try {
    const parsed = new URL(value);
    const hostname = parsed.hostname.toLowerCase();
    return (
      parsed.protocol === "https:" &&
      !parsed.username &&
      !parsed.password &&
      (hostname === "github.com" || hostname.endsWith(".githubusercontent.com"))
    );
  } catch (_) {
    return false;
  }
}

function requireBoundedReleaseFile(file, limit, description) {
  const stat = fs.lstatSync(file);
  if (!stat.isFile() || stat.isSymbolicLink()) {
    throw new Error(`${description} is not a regular non-symlink file`);
  }
  if (stat.size <= 0 || stat.size > limit) {
    throw new Error(`${description} size ${stat.size} is outside the accepted range 1..${limit}`);
  }
}

function copyOrDownloadReleaseFile(planned, name, destination, testDependencies = null) {
  const limit = releaseDownloadLimit(name, planned);
  if (testDependencies && testDependencies.releaseDirectory) {
    const root = fs.realpathSync.native(testDependencies.releaseDirectory);
    const requested = path.join(root, name);
    if (!fs.existsSync(requested)) {
      throw new Error(`required test release file is missing: ${name}`);
    }
    if (fs.lstatSync(requested).isSymbolicLink()) {
      throw new Error(`symlink test release file refused: ${name}`);
    }
    const source = fs.realpathSync.native(requested);
    const relative = path.relative(root, source);
    if (relative.startsWith("..") || path.isAbsolute(relative)) {
      throw new Error(`test release file escapes the explicit fixture directory: ${name}`);
    }
    requireBoundedReleaseFile(source, limit, `test release file ${name}`);
    fs.copyFileSync(source, destination);
    requireBoundedReleaseFile(destination, limit, `staged release file ${name}`);
    return "local-test-directory";
  }
  const curl = trustedUpdateTool("curl");
  if (!curl) throw new Error("curl not found; cannot download the verified GitHub release candidate");
  const sourceUrl = releaseFileUrl(planned, name);
  if (!allowedReleaseTransportUrl(sourceUrl)) {
    throw new Error(`release download source is outside the fixed HTTPS host policy: ${sourceUrl}`);
  }
  const result = runCommand(
    curl,
    [
      "-fsSL",
      "--proto",
      "=https",
      "--proto-redir",
      "=https",
      "--max-redirs",
      "5",
      "--connect-timeout",
      "15",
      "--speed-limit",
      "1024",
      "--speed-time",
      "30",
      "--max-filesize",
      String(limit),
      "--write-out",
      "%{url_effective}",
      sourceUrl,
      "-o",
      destination,
    ],
    { timeout: 120000 }
  );
  if (!result.ok) {
    throw new Error((result.stderr || result.error || `GitHub release download failed for ${name}`).trim());
  }
  const effectiveUrl = result.stdout.trim();
  if (!allowedReleaseTransportUrl(effectiveUrl)) {
    fs.rmSync(destination, { force: true });
    throw new Error(`release redirect escaped the accepted HTTPS host policy: ${effectiveUrl || "missing effective URL"}`);
  }
  requireBoundedReleaseFile(destination, limit, `downloaded release file ${name}`);
  return "github-release-https";
}

function resolveCosignBinary(testDependencies = null) {
  if (testDependencies && testDependencies.cosignPath) {
    const candidate = path.resolve(testDependencies.cosignPath);
    if (!fs.existsSync(candidate) || !fs.statSync(candidate).isFile()) {
      throw new Error(`configured test cosign executable is missing: ${candidate}`);
    }
    return { binary: candidate, source: "explicit-test-executable" };
  }
  const binary = trustedUpdateTool("cosign");
  if (!binary) {
    throw new Error(
      "cosign not found; install cosign, then retry the verified release update (the unverified Cargo fallback is disabled)"
    );
  }
  return { binary, source: "trusted-fixed-path" };
}

function validateLegacyReleaseCandidateManifest(manifest, planned, rawBinary) {
  requireCandidateObject(manifest, "release candidate");
  if (manifest.schema !== RELEASE_CANDIDATE_SCHEMA) {
    throw new Error(`unexpected release candidate schema: ${String(manifest.schema)}`);
  }
  if (manifest.version !== planned.target_version) {
    throw new Error(`release candidate version ${String(manifest.version)} does not match planned ${planned.target_version}`);
  }
  const expectedRef = `refs/tags/v${planned.target_version}`;
  if (manifest.source_ref !== expectedRef) {
    throw new Error(`release candidate source_ref ${String(manifest.source_ref)} does not match ${expectedRef}`);
  }
  if (typeof manifest.commit !== "string" || !/^[0-9a-f]{40}$/.test(manifest.commit)) {
    throw new Error("release candidate commit must be a full lowercase 40-character SHA-1");
  }
  if (!Array.isArray(manifest.artifacts) || !Array.isArray(manifest.runtime_bindings)) {
    throw new Error("release candidate artifacts and runtime_bindings must be arrays");
  }
  const buildPolicy = requireCandidateObject(manifest.build_policy, "release candidate build_policy");
  if (!Array.isArray(buildPolicy.targets) || new Set(buildPolicy.targets).size !== buildPolicy.targets.length) {
    throw new Error("release candidate build_policy.targets must be a unique array");
  }
  const target = githubReleaseTargetName();
  const expectedAsset = githubReleaseAssetName();
  if (!target || !expectedAsset || planned.asset !== expectedAsset || !buildPolicy.targets.includes(target)) {
    throw new Error(`release candidate does not authorize platform target ${target || "unmapped"}`);
  }

  const bindings = manifest.runtime_bindings.filter((entry) => entry && entry.target === target);
  if (bindings.length !== 1) {
    throw new Error(`release candidate must contain exactly one runtime binding for ${target}`);
  }
  const binding = requireCandidateObject(bindings[0], `runtime binding ${target}`);
  if (binding.raw_binary !== expectedAsset) {
    throw new Error(`release candidate runtime binding names ${String(binding.raw_binary)}, expected ${expectedAsset}`);
  }
  const bindingDigest = requireCandidateDigest(binding.runtime_sha256, `runtime binding ${target} digest`);
  const bindingSize = requireCandidateSize(binding.size_bytes, `runtime binding ${target} size`);

  const artifacts = manifest.artifacts.filter(
    (entry) => entry && entry.kind === "runtime_binary" && entry.target === target
  );
  if (artifacts.length !== 1) {
    throw new Error(`release candidate must contain exactly one runtime_binary artifact for ${target}`);
  }
  const artifact = requireCandidateObject(artifacts[0], `runtime artifact ${target}`);
  if (artifact.name !== expectedAsset) {
    throw new Error(`release candidate runtime artifact names ${String(artifact.name)}, expected ${expectedAsset}`);
  }
  const artifactDigest = requireCandidateDigest(artifact.sha256, `runtime artifact ${target} digest`);
  const artifactSize = requireCandidateSize(artifact.size_bytes, `runtime artifact ${target} size`);
  if (artifactDigest !== bindingDigest || artifactSize !== bindingSize) {
    throw new Error(`release candidate artifact/binding mismatch for ${target}`);
  }

  const rawDigest = sha256File(rawBinary);
  const rawSize = fs.statSync(rawBinary).size;
  if (rawDigest !== bindingDigest || rawSize !== bindingSize) {
    throw new Error(`downloaded runtime bytes do not match signed candidate for ${target}`);
  }

  const seed = {
    artifacts: manifest.artifacts,
    commit: manifest.commit,
    runtime_bindings: manifest.runtime_bindings,
    source_ref: manifest.source_ref,
    version: manifest.version,
  };
  const expectedCandidateId = `sha256:${sha256Text(canonicalJson(seed))}`;
  if (manifest.candidate_id !== expectedCandidateId) {
    throw new Error(
      `release candidate id mismatch: expected ${expectedCandidateId}, got ${String(manifest.candidate_id)}`
    );
  }
  return {
    artifact: expectedAsset,
    candidate_id: expectedCandidateId,
    commit: manifest.commit,
    manifest_sha256: sha256File(path.join(path.dirname(rawBinary), "CANDIDATE.json")),
    raw_sha256: rawDigest,
    raw_size_bytes: rawSize,
    source_ref: manifest.source_ref,
    target,
    version: manifest.version,
  };
}

function validateCanonicalReleaseCandidateManifest(manifest, compatibility, compatibilityPath, planned, rawBinary) {
  validateCanonicalCandidate(manifest);
  validateCanonicalCompatibility(compatibility);
  const expectedCompatibilityBytes = canonicalJsonV1(compatibility);
  const observedCompatibilityBytes = fs.readFileSync(compatibilityPath, "utf8");
  if (observedCompatibilityBytes !== expectedCompatibilityBytes) {
    throw new Error("RELEASE-COMPATIBILITY.json is not exact canonical UTF-8/no-newline JSON");
  }
  const compatibilityDigest = sha256File(compatibilityPath);
  if (manifest.core.compatibility_manifest_digest !== compatibilityDigest) {
    throw new Error("signed canonical candidate does not bind RELEASE-COMPATIBILITY.json bytes");
  }
  if (manifest.core.artifact_digests[CANONICAL_COMPATIBILITY_ARTIFACT_KEY] !== compatibilityDigest) {
    throw new Error("signed canonical candidate compatibility artifact key drifted");
  }
  if (
    manifest.core.artifact_digests[CANONICAL_ROLLBACK_ARTIFACT_KEY] !==
    manifest.core.rollback_plan_digest
  ) {
    throw new Error("signed canonical candidate rollback artifact key drifted");
  }
  if (compatibility.version !== planned.target_version) {
    throw new Error(
      `canonical release candidate version ${String(compatibility.version)} does not match planned ${planned.target_version}`
    );
  }
  const expectedRef = `refs/tags/v${planned.target_version}`;
  if (compatibility.source_ref !== expectedRef) {
    throw new Error(`canonical release candidate source_ref ${String(compatibility.source_ref)} does not match ${expectedRef}`);
  }
  if (manifest.core.repo_commits.m1nd !== compatibility.commit) {
    throw new Error("canonical candidate repo_commits.m1nd does not match compatibility commit");
  }
  const target = githubReleaseTargetName();
  const expectedAsset = githubReleaseAssetName();
  if (!target || !expectedAsset || planned.asset !== expectedAsset) {
    throw new Error(`canonical release candidate cannot map platform target ${target || "unmapped"}`);
  }
  const targets = compatibility.targets.filter((entry) => entry && entry.target === target);
  if (targets.length !== 1) {
    throw new Error(`canonical compatibility must contain exactly one target ${target}`);
  }
  const binding = targets[0];
  if (binding.asset !== expectedAsset) {
    throw new Error(`canonical compatibility asset ${String(binding.asset)} does not match ${expectedAsset}`);
  }
  const rawDigest = sha256File(rawBinary);
  const rawSize = BigInt(fs.statSync(rawBinary).size);
  const declaredSize = requireCanonicalU64(binding.size_bytes, "compatibility.size_bytes");
  if (binding.sha256 !== rawDigest || declaredSize !== rawSize) {
    throw new Error(`downloaded runtime bytes do not match canonical compatibility for ${target}`);
  }
  const artifactKey = `${CANONICAL_RELEASE_ASSET_PREFIX}${expectedAsset}`;
  if (manifest.core.artifact_digests[artifactKey] !== rawDigest) {
    throw new Error(`signed canonical candidate does not bind runtime artifact key ${artifactKey}`);
  }
  return {
    artifact: expectedAsset,
    candidate_digest: manifest.candidate_digest,
    // Operational compatibility alias for the existing rollback journal.  Its
    // kind is explicit so callers cannot confuse it with the legacy sha256: id.
    candidate_id: manifest.candidate_digest,
    candidate_identity_kind: "canonical-domain-separated-digest",
    candidate_schema: CANONICAL_RELEASE_CANDIDATE_SCHEMA,
    commit: compatibility.commit,
    manifest_sha256: sha256File(path.join(path.dirname(rawBinary), "CANDIDATE.json")),
    raw_sha256: rawDigest,
    raw_size_bytes: Number(rawSize),
    source_ref: compatibility.source_ref,
    target,
    version: compatibility.version,
  };
}

function validateReleaseCandidateManifest(manifest, planned, rawBinary, compatibility, compatibilityPath) {
  if (manifest && manifest.schema === RELEASE_CANDIDATE_SCHEMA) {
    return validateLegacyReleaseCandidateManifest(manifest, planned, rawBinary);
  }
  if (manifest && manifest.schema === CANONICAL_RELEASE_CANDIDATE_SCHEMA) {
    if (!compatibility || !compatibilityPath) {
      throw new Error("canonical release candidate requires RELEASE-COMPATIBILITY.json");
    }
    return validateCanonicalReleaseCandidateManifest(
      manifest,
      compatibility,
      compatibilityPath,
      planned,
      rawBinary
    );
  }
  throw new Error(`unexpected release candidate schema: ${String(manifest && manifest.schema)}`);
}

function stageVerifiedReleaseCandidate(planned, testDependencies = null) {
  if (!/^[0-9]+\.[0-9]+\.[0-9]+(?:-[0-9A-Za-z.-]+)?$/.test(String(planned.target_version || ""))) {
    throw new Error(`planned release version is not an exact semantic version: ${String(planned.target_version)}`);
  }
  const cosign = resolveCosignBinary(testDependencies);
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), "m1nd-update-verified-"));
  try {
    const rawBinary = path.join(directory, planned.asset);
    const manifestPath = path.join(directory, "CANDIDATE.json");
    const bundlePath = path.join(directory, "CANDIDATE.json.sigstore.json");
    const transportSource = copyOrDownloadReleaseFile(
      planned,
      "CANDIDATE.json",
      manifestPath,
      testDependencies
    );
    copyOrDownloadReleaseFile(
      planned,
      "CANDIDATE.json.sigstore.json",
      bundlePath,
      testDependencies
    );
    copyOrDownloadReleaseFile(planned, planned.asset, rawBinary, testDependencies);

    const certificateIdentity = releaseCertificateIdentity(planned.target_version);
    const verified = runCommand(
      cosign.binary,
      [
        "verify-blob",
        "--bundle",
        bundlePath,
        "--certificate-identity",
        certificateIdentity,
        "--certificate-oidc-issuer",
        GITHUB_OIDC_ISSUER,
        manifestPath,
      ],
      { timeout: 120000 }
    );
    if (!verified.ok) {
      throw new Error((verified.stderr || verified.error || "cosign refused CANDIDATE.json").trim());
    }
    let manifest;
    try {
      manifest = parseIntegerJson(fs.readFileSync(manifestPath, "utf8"), "signed CANDIDATE.json");
    } catch (error) {
      throw new Error(`signed CANDIDATE.json is invalid JSON: ${error.message}`);
    }
    let compatibility = null;
    let compatibilityPath = null;
    if (manifest.schema === CANONICAL_RELEASE_CANDIDATE_SCHEMA) {
      compatibilityPath = path.join(directory, CANONICAL_COMPATIBILITY_FILE);
      const compatibilityTransport = copyOrDownloadReleaseFile(
        planned,
        CANONICAL_COMPATIBILITY_FILE,
        compatibilityPath,
        testDependencies
      );
      if (compatibilityTransport !== transportSource) {
        throw new Error("candidate and compatibility manifests arrived through different transports");
      }
      compatibility = parseIntegerJson(
        fs.readFileSync(compatibilityPath, "utf8"),
        CANONICAL_COMPATIBILITY_FILE
      );
    }
    const candidate = validateReleaseCandidateManifest(
      manifest,
      planned,
      rawBinary,
      compatibility,
      compatibilityPath
    );
    if (process.platform !== "win32") fs.chmodSync(rawBinary, 0o755);
    return {
      source_binary: rawBinary,
      staging_directory: directory,
      verification: {
        ...candidate,
        certificate_identity: certificateIdentity,
        certificate_oidc_issuer: GITHUB_OIDC_ISSUER,
        transport_source: transportSource,
        verifier_source: cosign.source,
      },
    };
  } catch (error) {
    fs.rmSync(directory, { recursive: true, force: true });
    throw error;
  }
}

function installRuntimeBinaryWithBackup(sourceBinary, targetBinary, verification) {
  const beforeVersion = runtimeVersion(targetBinary);
  const beforeSha256 = fileSha256OrNull(targetBinary);
  const candidateSha256 = sha256File(sourceBinary);
  if (!verification || verification.raw_sha256 !== candidateSha256) {
    throw new Error("verified candidate metadata is missing or does not bind the staged runtime bytes");
  }
  let backup = null;
  if (beforeSha256 !== null) {
    backup = updateBackupPath(targetBinary, beforeVersion);
    ensureDir(path.dirname(backup));
    fs.copyFileSync(targetBinary, backup);
    if (process.platform !== "win32") fs.chmodSync(backup, 0o755);
    fsyncFile(backup);
    fsyncDirectory(path.dirname(backup));
    if (sha256File(backup) !== beforeSha256) {
      throw new Error("runtime backup digest does not match the observed pre-update target");
    }
  }
  const state = {
    schema: UPDATE_STATE_SCHEMA,
    created_at: new Date().toISOString(),
    phase: "prepared",
    install_kind: "verified-github-release",
    rollback_available: true,
    target_binary: targetBinary,
    backup_binary: backup,
    backup_sha256: backup ? sha256File(backup) : null,
    before_version: beforeVersion,
    before_sha256: beforeSha256,
    candidate_sha256: candidateSha256,
    candidate_id: verification.candidate_id,
    candidate_manifest_sha256: verification.manifest_sha256,
    candidate_source_ref: verification.source_ref,
    candidate_target: verification.target,
    after_version: null,
    after_sha256: null,
  };
  writeJsonAtomic(updateStatePath(), state);
  const currentBeforeInstall = fileSha256OrNull(targetBinary);
  if (currentBeforeInstall !== beforeSha256) {
    throw new Error(
      `runtime target drifted after backup: expected ${String(beforeSha256)}, observed ${String(currentBeforeInstall)}`
    );
  }
  installRuntimeBinary(sourceBinary, targetBinary);
  state.after_version = runtimeVersion(targetBinary, RUNTIME_VERSION_PROBE_AFTER_INSTALL_MS);
  state.after_sha256 = sha256File(targetBinary);
  if (state.after_sha256 !== candidateSha256) {
    throw new Error(
      `installed runtime digest ${state.after_sha256} does not match candidate ${candidateSha256}`
    );
  }
  state.phase = "installed";
  state.installed_at = new Date().toISOString();
  writeJsonAtomic(updateStatePath(), state);
  return state;
}

function applySelfUpdate(args, testDependencies = null) {
  const proof = buildSelfUpdateProof(args, "apply", testDependencies);
  const yes = Boolean(args.yes);
  proof.dry_run = !yes;
  if (!yes) {
    proof.next_actions.push("Re-run with --yes to apply the planned update actions.");
    proof.next_actions.push("Use --no-npm, --no-runtime, --no-skills, or --no-kill to narrow the apply surface.");
    return proof;
  }

  for (const planned of proof.planned_actions) {
    if (planned.id === "runtime-install-github-release") {
      if (args["no-runtime"]) continue;
      let staged = null;
      try {
        staged = stageVerifiedReleaseCandidate(planned, testDependencies);
        const state = installRuntimeBinaryWithBackup(
          staged.source_binary,
          planned.target_binary,
          staged.verification
        );
        const versionVerified = Boolean(
          state.after_version && state.after_version.includes(planned.target_version)
        );
        proof.applied_actions.push({
          id: planned.id,
          kind: planned.kind,
          ok: versionVerified,
          installed: true,
          source: planned.url,
          target_binary: planned.target_binary,
          rollback_state: updateStatePath(),
          backup_binary: state.backup_binary,
          before_version: state.before_version,
          after_version: state.after_version,
          candidate_verification: staged.verification,
          version_verified: versionVerified,
        });
        const applied = proof.applied_actions[proof.applied_actions.length - 1];
        if (!applied.ok) {
          proof.blocked_actions.push(action("runtime-version-mismatch-after-install", "runtime", "installed runtime did not report the target version", {
            target_version: planned.target_version,
            after_version: state.after_version,
          }));
        }
      } catch (error) {
        proof.blocked_actions.push(action("runtime-install-failed", "runtime", "runtime release install failed", {
          error: error instanceof Error ? error.message : String(error),
        }));
      } finally {
        if (staged && staged.staging_directory) {
          fs.rmSync(staged.staging_directory, { recursive: true, force: true });
        }
      }
    }

    if (planned.id === "skills-refresh") {
      if (args["no-skills"]) continue;
      try {
        proof.applied_actions.push({
          id: planned.id,
          kind: planned.kind,
          ok: true,
          result: installSkills(planned.host || "codex", process.cwd()),
        });
      } catch (error) {
        proof.blocked_actions.push(action("skills-refresh-failed", "agent-pack", "agent pack refresh failed", {
          error: error instanceof Error ? error.message : String(error),
        }));
      }
    }

    if (planned.id === "stop-runtime-processes") {
      if (args["no-kill"]) continue;
      const runtimeApplied = proof.applied_actions.some(
        (applied) => applied.kind === "runtime" && (applied.ok || applied.installed === true)
      );
      if (!runtimeApplied) {
        proof.blocked_actions.push(
          action(
            "stop-runtime-processes-skipped",
            "process",
            "runtime processes were not stopped because no runtime install completed"
          )
        );
        continue;
      }
      proof.applied_actions.push({
        id: planned.id,
        kind: planned.kind,
        ok: true,
        stopped_processes: stopRuntimeProcesses(listRuntimeProcesses()),
      });
    }
  }

  proof.runtime_version_after = runtimeVersion(proof.runtime.target_binary);
  proof.requires_host_rebind = proof.applied_actions.some(
    (applied) =>
      ["npm", "runtime", "agent-pack", "process"].includes(applied.kind) &&
      (applied.ok === true || applied.installed === true)
  );
  if (proof.requires_host_rebind) {
    proof.next_actions.push("Restart or rebind each MCP host/client so it launches the updated runtime and refreshes its cached tool list.");
  }
  proof.next_actions.push("Then run m1nd update verify, trust_selftest, or session_handshake with the intended workspace scope.");
  return proof;
}

function verifySelfUpdate(args, testDependencies = null) {
  const proof = buildSelfUpdateProof(args, "verify", testDependencies);
  const repo = path.resolve(args.repo || process.cwd());
  const transport = args.transport || "stdio";
  const script = path.join(repo, "scripts", "m1nd_agent_demo.py");
  proof.verify = {
    repo,
    transport,
    doctor: doctor(),
    smoke: null,
  };
  if (!fs.existsSync(script)) {
    proof.blocked_actions.push(action("smoke-script-missing", "verify", "m1nd smoke harness not found in repo", {
      script,
    }));
    proof.next_actions.push("Run update verify from a m1nd source checkout or pass --repo /path/to/m1nd.");
    return proof;
  }
  const python = process.env.PYTHON || "python3";
  const result = runCommand(python, [script, "--repo", repo, "--transport", transport, "--json"], { timeout: 120000 });
  proof.verify.smoke = {
    ok: result.ok,
    status: result.status,
    stdout_json: safeJsonParse(result.stdout),
    stderr: result.stderr.trim(),
  };
  if (!result.ok) {
    proof.blocked_actions.push(action("smoke-failed", "verify", "m1nd smoke verify failed", {
      status: result.status,
      stderr: result.stderr.trim(),
    }));
  }
  return proof;
}

function buildSelfUpdateStatus(args, testDependencies = null) {
  const proof = buildSelfUpdateProof(args, "status", testDependencies);
  const doctorResult = doctor();
  const liveRuntimeProcesses = listRuntimeProcesses();
  const nonBlockingActionIds = new Set(["npm-registry-lag", "kill-disabled"]);
  const blockingActions = proof.blocked_actions.filter((blocked) => !nonBlockingActionIds.has(blocked.id));
  const packageRuntimeMatch = Boolean(
    proof.package_version &&
      proof.runtime_version &&
      proof.runtime_version.includes(proof.package_version)
  );
  const pathRuntimeMatch = Boolean(
    !proof.runtime.path_version ||
      !proof.target_version ||
      proof.runtime.path_version.includes(proof.target_version)
  );
  const agentPackOk = Boolean(proof.agent_pack.ok && doctorResult.pack_ok);
  const hasPlannedActions = proof.planned_actions.length > 0;
  const needsAttention =
    hasPlannedActions ||
    blockingActions.length > 0 ||
    !packageRuntimeMatch ||
    !pathRuntimeMatch ||
    !agentPackOk;

  proof.doctor = doctorResult;
  proof.live_runtime_processes = liveRuntimeProcesses;
  proof.status_summary = {
    readiness: needsAttention ? "attention" : "ready",
    install_current: proof.install_state === "current",
    package_runtime_match: packageRuntimeMatch,
    path_runtime_match: pathRuntimeMatch,
    agent_pack_ok: agentPackOk,
    planned_action_count: proof.planned_actions.length,
    blocked_action_count: proof.blocked_actions.length,
    blocking_action_count: blockingActions.length,
    active_runtime_process_count: liveRuntimeProcesses.length,
    host_rebind_state:
      liveRuntimeProcesses.length > 0
        ? "active_runtime_processes_present_rebind_not_proven"
        : "no_visible_runtime_processes",
    host_rebind_proven: false,
  };

  if (hasPlannedActions) {
    proof.next_actions.push("Run m1nd update plan --json, inspect actions, then apply with --yes when ready.");
  }
  if (blockingActions.length > 0) {
    proof.next_actions.push("Resolve blocking_actions before treating this install as agent-ready.");
  }
  if (!pathRuntimeMatch) {
    proof.next_actions.push("Align the m1nd-mcp found on PATH or pass --binary to target the runtime your host actually launches.");
  }
  if (liveRuntimeProcesses.length > 0) {
    proof.next_actions.push("Visible m1nd-mcp processes exist; after any update, restart/rebind host sessions before trusting cached tool lists.");
  }
  if (!hasPlannedActions && blockingActions.length === 0) {
    proof.next_actions.push("Run m1nd update verify --repo /path/to/m1nd --transport stdio --json for a live smoke proof.");
  }
  return proof;
}

function rollbackSelfUpdate(args, testDependencies = null) {
  const proof = buildSelfUpdateProof(args, "rollback", testDependencies);
  proof.requires_host_rebind = false;
  const statePath = updateStatePath();
  const refuse = (id, description, extra = {}) => {
    proof.blocked_actions.push(action(id, "rollback", description, { state_path: statePath, ...extra }));
    return proof;
  };
  if (!fs.existsSync(statePath)) {
    return refuse("rollback-state-missing", "no local update rollback state exists");
  }
  const state = safeJsonParse(fs.readFileSync(statePath, "utf8"));
  if (!state || state.schema !== UPDATE_STATE_SCHEMA) {
    return refuse("rollback-state-invalid", "local update rollback state has an invalid schema", {
      schema: state ? state.schema : null,
    });
  }
  if (!UPDATE_PHASES.has(state.phase)) {
    return refuse("rollback-state-phase-invalid", "rollback journal has an unknown or legacy phase and cannot be recovered automatically", {
      phase: state.phase || null,
      allowed_phases: Array.from(UPDATE_PHASES),
      suggested_action: "inspect the legacy journal and runtime bytes manually; automatic rollback will not guess",
    });
  }
  if (typeof state.target_binary !== "string" || !path.isAbsolute(state.target_binary)) {
    return refuse("rollback-state-target-invalid", "rollback journal target must be an absolute path", {
      state_target_binary: state.target_binary || null,
    });
  }
  if (
    args.binary &&
    path.resolve(args.binary) !== path.resolve(state.target_binary || "")
  ) {
    return refuse("rollback-target-mismatch", "requested runtime does not match the journaled update target", {
      requested_binary: path.resolve(args.binary),
      state_target_binary: state.target_binary || null,
    });
  }
  const cargoFallbackJournal =
    state.install_kind === "cargo-fallback-unverified" ||
    (state.rollback_available === false && !state.backup_binary && !state.candidate_sha256);
  if (cargoFallbackJournal) {
    return refuse("rollback-unavailable-cargo-fallback", "Cargo fallback installs are not verified release candidates and have no automatic rollback backup", {
      install_kind: state.install_kind || "legacy-cargo-fallback-unverified",
      rollback_available: false,
    });
  }
  if (!SHA256_RE.test(String(state.candidate_sha256 || ""))) {
    return refuse("rollback-state-digest-invalid", "rollback journal lacks a valid verified candidate digest", {
      field: "candidate_sha256",
    });
  }
  if (state.before_sha256 !== null && !SHA256_RE.test(String(state.before_sha256 || ""))) {
    return refuse("rollback-state-digest-invalid", "rollback journal has an invalid pre-update digest", {
      field: "before_sha256",
    });
  }
  if (
    state.phase === "installed" &&
    (!SHA256_RE.test(String(state.after_sha256 || "")) || state.after_sha256 !== state.candidate_sha256)
  ) {
    return refuse("rollback-state-digest-invalid", "installed journal must bind identical candidate and after digests", {
      candidate_sha256: state.candidate_sha256,
      after_sha256: state.after_sha256 || null,
    });
  }
  let currentTargetSha256;
  try {
    currentTargetSha256 = fileSha256OrNull(state.target_binary);
  } catch (error) {
    return refuse("rollback-target-unreadable", "rollback target cannot be safely inspected", {
      target_binary: state.target_binary,
      error: error instanceof Error ? error.message : String(error),
    });
  }
  const rollbackStartPhase = state.phase;

  if (state.phase === "rolled_back") {
    if (currentTargetSha256 !== state.before_sha256) {
      return refuse("rollback-target-digest-mismatch", "rolled-back target drifted after rollback; refusing to overwrite it", {
        phase: state.phase,
        target_binary: state.target_binary,
        expected_sha256: state.before_sha256,
        observed_sha256: currentTargetSha256,
      });
    }
    proof.applied_actions.push({
      id: "runtime-rollback",
      kind: "rollback",
      ok: true,
      idempotent: true,
      phase: "rolled_back",
      target_binary: state.target_binary,
      restored_sha256: currentTargetSha256,
      rollback_state: statePath,
    });
    proof.next_actions.push("Rollback was already complete; no runtime or journal bytes changed.");
    return proof;
  }

  if (
    (state.phase === "prepared" || state.phase === "installed") &&
    currentTargetSha256 === state.before_sha256
  ) {
    const recovery =
      state.phase === "prepared"
        ? "prepared-target-still-before"
        : "installed-target-already-before";
    state.phase = "rolled_back";
    state.rolled_back_at = new Date().toISOString();
    state.restored_sha256 = state.before_sha256;
    state.restored_version = runtimeVersion(state.target_binary);
    state.recovery = recovery;
    writeJsonAtomic(statePath, state);
    proof.applied_actions.push({
      id: "runtime-rollback",
      kind: "rollback",
      ok: true,
      idempotent: true,
      recovery: state.recovery,
      target_binary: state.target_binary,
      restored_sha256: state.restored_sha256,
      rollback_state: statePath,
    });
    proof.next_actions.push(
      recovery === "prepared-target-still-before"
        ? "Prepared update had not replaced the runtime; the journal was closed without rewriting the target."
        : "Rollback had already restored the pre-update runtime; crash recovery closed the journal without rewriting the target."
    );
    return proof;
  }

  if (currentTargetSha256 !== state.candidate_sha256) {
    return refuse("rollback-target-digest-mismatch", "current runtime bytes do not match the journal phase; refusing stale rollback overwrite", {
      phase: state.phase,
      target_binary: state.target_binary,
      expected_sha256: state.candidate_sha256,
      observed_sha256: currentTargetSha256,
    });
  }

  let backupSha256 = null;
  if (state.before_sha256 === null) {
    if (state.backup_binary !== null || state.backup_sha256 !== null) {
      return refuse("rollback-state-backup-invalid", "first-install journal must not claim pre-update backup bytes", {
        backup_binary: state.backup_binary || null,
        backup_sha256: state.backup_sha256 || null,
      });
    }
  } else {
    if (!state.backup_binary || !path.isAbsolute(state.backup_binary) || !fs.existsSync(state.backup_binary)) {
      return refuse("rollback-backup-missing", "rollback state has no usable runtime backup", {
        backup_binary: state.backup_binary || null,
      });
    }
    backupSha256 = sha256File(state.backup_binary);
    if (
      !SHA256_RE.test(String(state.backup_sha256 || "")) ||
      backupSha256 !== state.backup_sha256 ||
      backupSha256 !== state.before_sha256
    ) {
      return refuse("rollback-backup-digest-mismatch", "rollback backup bytes differ from the journaled pre-update digest", {
        backup_binary: state.backup_binary,
        expected_sha256: state.before_sha256,
        journaled_backup_sha256: state.backup_sha256 || null,
        observed_sha256: backupSha256,
      });
    }
  }

  if (state.before_sha256 === null) {
    fs.rmSync(state.target_binary, { force: true });
    fsyncDirectory(path.dirname(state.target_binary));
  } else {
    installRuntimeBinary(state.backup_binary, state.target_binary);
  }
  const restoredSha256 = fileSha256OrNull(state.target_binary);
  if (restoredSha256 !== state.before_sha256) {
    return refuse("rollback-restore-digest-mismatch", "restored runtime bytes differ from the pre-update digest", {
      target_binary: state.target_binary,
      expected_sha256: state.before_sha256,
      observed_sha256: restoredSha256,
    });
  }
  state.phase = "rolled_back";
  state.rolled_back_at = new Date().toISOString();
  state.restored_sha256 = restoredSha256;
  state.restored_version = runtimeVersion(state.target_binary);
  if (rollbackStartPhase === "prepared") state.recovery = "prepared-target-was-candidate";
  writeJsonAtomic(statePath, state);
  proof.applied_actions.push({
    id: "runtime-rollback",
    kind: "rollback",
    ok: true,
    target_binary: state.target_binary,
    backup_binary: state.backup_binary,
    restored_sha256: restoredSha256,
    restored_version: state.restored_version,
    rollback_state: statePath,
  });
  proof.requires_host_rebind = true;
  proof.next_actions.push("Restart or rebind each MCP host/client so it launches the restored runtime.");
  return proof;
}

function selfUpdateInternal(args, testDependencies = null) {
  const subcommand = args._[1] || "check";
  switch (subcommand) {
    case "check":
    case "plan":
      return buildSelfUpdateProof(args, subcommand, testDependencies);
    case "status":
      return buildSelfUpdateStatus(args, testDependencies);
    case "apply":
      return applySelfUpdate(args, testDependencies);
    case "verify":
      return verifySelfUpdate(args, testDependencies);
    case "rollback":
      return rollbackSelfUpdate(args, testDependencies);
    default:
      throw new Error(`unknown update subcommand '${subcommand}'`);
  }
}

// The ONLY sanctioned exception to the ambient-override refusal is the release
// `verified-update-smoke` job, which must drive the real `update` command against a
// local release directory. The seam opens only when BOTH fences hold: the explicit,
// dedicated marker the smoke sets in its own child env (`M1ND_RELEASE_SMOKE=1`), AND a
// source checkout — a packed/installed client has no `.git` at PACKAGE_ROOT (the same
// fence `createSelfUpdateTestHarness` relies on), so this can never open in production.
// Absent either fence, the ambient overrides remain firmly refused.
function releaseSmokeSeamOpen() {
  return (
    process.env.M1ND_RELEASE_SMOKE === "1" &&
    fs.existsSync(path.join(PACKAGE_ROOT, ".git"))
  );
}

// Mirror the harness's dependency shape, but sourced from the smoke's child env,
// because the public `update` command has no explicit dependency channel.
function releaseSmokeDependencies() {
  return Object.freeze({
    releaseDirectory: process.env.M1ND_TEST_RELEASE_DIR
      ? path.resolve(process.env.M1ND_TEST_RELEASE_DIR)
      : null,
    cosignPath: process.env.M1ND_TEST_COSIGN_PATH
      ? path.resolve(process.env.M1ND_TEST_COSIGN_PATH)
      : null,
  });
}

function selfUpdate(args) {
  const forbiddenAmbientOverrides = [
    "M1ND_TEST_RELEASE_DIR",
    "M1ND_TEST_COSIGN_PATH",
  ].filter((name) => Object.prototype.hasOwnProperty.call(process.env, name));
  if (forbiddenAmbientOverrides.length > 0) {
    if (!releaseSmokeSeamOpen()) {
      throw new Error(
        `unsafe self-update test overrides are not accepted by the production updater: ${forbiddenAmbientOverrides.join(
          ", "
        )}`
      );
    }
    return selfUpdateInternal(args, releaseSmokeDependencies());
  }
  return selfUpdateInternal(args, null);
}

// Tests need deterministic local release bytes and a fake verifier, but those
// capabilities must never be ambient production environment variables.  The
// harness is available only from a source checkout; packed/installed clients
// always execute `selfUpdate` with immutable production dependencies.
function createSelfUpdateTestHarness() {
  if (!fs.existsSync(path.join(PACKAGE_ROOT, ".git"))) {
    throw new Error("self-update test harness is unavailable outside a source checkout");
  }
  return (args, dependencies = {}) => {
    const releaseDirectory = dependencies.releaseDirectory
      ? path.resolve(dependencies.releaseDirectory)
      : null;
    const cosignPath = dependencies.cosignPath ? path.resolve(dependencies.cosignPath) : null;
    return selfUpdateInternal(args, Object.freeze({ releaseDirectory, cosignPath }));
  };
}

function sourceReleaseBinary(sourceDir) {
  return path.join(sourceDir, "target", "release", runtimeBinaryName());
}

function sourceLooksBuildable(sourceDir) {
  return (
    fs.existsSync(path.join(sourceDir, "Cargo.toml")) &&
    fs.existsSync(path.join(sourceDir, "m1nd-mcp", "Cargo.toml"))
  );
}

function listRuntimeProcesses() {
  if (process.platform === "win32") {
    const result = runCommand("tasklist", ["/FI", `IMAGENAME eq ${runtimeBinaryName()}`, "/FO", "CSV", "/NH"]);
    if (!result.ok) return [];
    return result.stdout
      .split(/\r?\n/)
      .map((line) => line.trim())
      .filter(Boolean)
      .map((line) => {
        const columns = line
          .split(/","/)
          .map((part) => part.replace(/^"|"$/g, ""));
        return { pid: Number(columns[1]), ppid: null, state: null, command: columns[0] };
      })
      .filter((processInfo) => Number.isFinite(processInfo.pid));
  }
  const result = runCommand("ps", ["-ax", "-o", "pid=,ppid=,state=,command="]);
  if (!result.ok) return [];
  return result.stdout
    .split(/\r?\n/)
    .map((line) => line.trim())
    .map((line) => {
      const match = line.match(/^(\d+)\s+(\d+)\s+(\S+)\s+(.+)$/);
      if (!match) return null;
      return { pid: Number(match[1]), ppid: Number(match[2]), state: match[3], command: match[4] };
    })
    .filter(Boolean)
    .filter((processInfo) => commandLooksLikeRuntime(processInfo.command));
}

function commandLooksLikeRuntime(command) {
  const firstToken = String(command || "").trim().split(/\s+/)[0] || "";
  const base = path.basename(firstToken).replace(/^\(|\)$/g, "");
  return base === runtimeBinaryName();
}

function stopRuntimeProcesses(processes) {
  const stopped = [];
  for (const processInfo of processes) {
    if (!processInfo.pid || processInfo.pid === process.pid) continue;
    const result =
      process.platform === "win32"
        ? runCommand("taskkill", ["/PID", String(processInfo.pid), "/T", "/F"])
        : runCommand("kill", ["-TERM", String(processInfo.pid)]);
    stopped.push({
      pid: processInfo.pid,
      ppid: processInfo.ppid || null,
      state: processInfo.state || null,
      command: processInfo.command,
      ok: result.ok,
      status: result.status,
      stderr: result.stderr.trim(),
    });
  }
  return stopped;
}

function installRuntimeBinary(sourceBinary, targetBinary) {
  ensureDir(path.dirname(targetBinary));
  const tempTarget = path.join(
    path.dirname(targetBinary),
    `.${path.basename(targetBinary)}.${process.pid}.${crypto.randomBytes(6).toString("hex")}.tmp`
  );
  try {
    fs.copyFileSync(sourceBinary, tempTarget);
    if (process.platform !== "win32") fs.chmodSync(tempTarget, 0o755);
    fsyncFile(tempTarget);
    fs.renameSync(tempTarget, targetBinary);
    fsyncDirectory(path.dirname(targetBinary));
  } finally {
    fs.rmSync(tempTarget, { force: true });
  }
}

// Ad-hoc codesign a freshly-installed binary on macOS. A binary written by a
// plain file copy inherits no signature, and Gatekeeper kills the unsigned
// replacement with OS_REASON_CODESIGNING the moment launchd re-execs it — the
// swap "succeeds" but the daemon never comes back. `codesign --sign -` applies
// an ad-hoc signature that satisfies the local policy. Returns a small result
// object; a MISSING codesign tool is a warning, never fatal (Linux/Windows and
// stripped-down macOS have no codesign — the install still stands).
function codesignAdHoc(targetBinary) {
  const result = runCommand("codesign", ["--force", "--sign", "-", targetBinary]);
  if (result.error && /ENOENT/.test(result.error)) {
    return { attempted: true, ok: false, missing: true, error: result.error };
  }
  return {
    attempted: true,
    ok: result.ok,
    missing: false,
    status: result.status,
    error: result.error,
    stderr: (result.stderr || "").trim() || undefined,
  };
}

// Parse the LABEL out of one `launchctl list` line. The format is
// `PID\tSTATUS\tLABEL`, and a launchd label MAY contain whitespace, so the label
// is everything from the third column on — `.split(/\s+/).slice(2).join(" ")`,
// NOT `.pop()` (which would keep only the last space-separated token of a label
// with spaces). Returns "" for a header/blank/malformed line.
function parseLaunchctlLabel(line) {
  const parts = String(line || "").trim().split(/\s+/);
  if (parts.length < 3) return "";
  return parts.slice(2).join(" ");
}

// Candidate m1nd-named labels from `launchctl list` (a label naming m1nd). These
// are only CANDIDATES: the fleet may run several m1nd services (worktrees, other
// installs), and a swap must reload ONLY the one whose managed binary is the
// target just installed — see `launchdLabelManagesTarget`. Returns [] when
// launchctl is absent / no m1nd service is loaded (restart keeps its kill -TERM
// fallback).
function candidateLaunchdLabels() {
  if (process.platform !== "darwin") return [];
  const result = runCommand("launchctl", ["list"]);
  if (!result.ok) return [];
  return result.stdout
    .split(/\r?\n/)
    .map(parseLaunchctlLabel)
    .filter(Boolean)
    .filter((label) => /m1nd/i.test(label));
}

// The program path a launchd label manages, from `launchctl print gui/<uid>/<label>`.
// The print output has a `program = /path/to/bin` line (and/or a `program-arguments`
// array whose first entry is the executable). Returns the resolved path or null
// when it cannot be determined (absent tool, unreadable output).
function launchdLabelProgramPath(uid, label) {
  const result = runCommand("launchctl", ["print", `gui/${uid}/${label}`]);
  if (!result.ok) return null;
  return parseLaunchctlProgramPath(result.stdout);
}

// Pure parse of a `launchctl print` block → the managed executable path. Prefers
// the explicit `program = …`; falls back to the first `program-arguments` entry.
// Returns null when neither is present.
function parseLaunchctlProgramPath(printOutput) {
  const text = String(printOutput || "");
  const program = text.match(/^\s*program\s*=\s*(.+?)\s*$/m);
  if (program && program[1]) return program[1].trim();
  // program-arguments = {\n\t\t0 => /path/to/exe\n ... }
  const firstArg = text.match(/program-arguments\s*=\s*\{[^}]*?\b0\s*=>\s*(.+?)\s*$/m);
  if (firstArg && firstArg[1]) return firstArg[1].trim();
  return null;
}

// Whether a launchd label's managed program IS the target binary just installed.
// Pure + path-normalized so `/var`→`/private/var` aliases and trailing separators
// do not cause a false miss (or, worse, a false MATCH that SIGKILLs an unrelated
// m1nd service). A null/empty program path is NOT a match (fail-closed): an
// undiscoverable program must never be kicked.
function launchdLabelManagesTarget(programPath, targetBinary) {
  if (!programPath || !targetBinary) return false;
  const norm = (p) => {
    try {
      return fs.realpathSync(p);
    } catch (_) {
      return path.resolve(String(p));
    }
  };
  return norm(programPath) === norm(targetBinary);
}

// Reload the managed launchd service that runs the JUST-INSTALLED binary so it
// re-execs it. `launchctl kickstart -k gui/<uid>/<label>` stops (SIGKILL) then
// restarts the service — the reliable "reload now" the plain kill -TERM could
// miss (KeepAlive races, the TERM never reaching the service).
//
// SCOPE (field hazard fix): only labels whose managed program path equals
// `targetBinary` are kicked. The old code kicked EVERY label containing "m1nd",
// so one swap SIGKILLed the whole fleet (worktrees, other installs). Each
// candidate is resolved via `launchctl print` and compared to the target;
// non-matches are recorded as skipped, never kicked. Returns one result per
// candidate (kicked or skipped); empty when no m1nd label is loaded.
function kickstartManagedServices(targetBinary) {
  const uid = typeof process.getuid === "function" ? process.getuid() : null;
  if (uid === null) return [];
  const labels = candidateLaunchdLabels();
  return labels.map((label) => {
    const domain = `gui/${uid}/${label}`;
    const programPath = launchdLabelProgramPath(uid, label);
    if (!launchdLabelManagesTarget(programPath, targetBinary)) {
      // A candidate that does NOT manage the target is left untouched — kicking
      // it would SIGKILL an unrelated m1nd service.
      return { label, domain, skipped: true, reason: "program path does not match target", program_path: programPath || null };
    }
    const result = runCommand("launchctl", ["kickstart", "-k", domain]);
    return { label, domain, ok: result.ok, status: result.status, program_path: programPath, stderr: (result.stderr || "").trim() || undefined };
  });
}

// Whether the reload (kickstart) may proceed after an install. On darwin a
// re-exec of an UNSIGNED binary is killed by the OS (OS_REASON_CODESIGNING), so
// if the ad-hoc codesign was attempted and FAILED, kickstarting would drive the
// service into a kill/respawn loop — refuse it and tell the operator to sign
// first. Non-darwin, or a successful/needless codesign, allows the reload.
// `codesign` is the `codesignAdHoc` result (or null when none was attempted).
function shouldKickstartAfterInstall(platform, codesign) {
  if (platform !== "darwin") return true;
  if (codesign && codesign.attempted && !codesign.ok) return false;
  return true;
}

function restart(args) {
  const sourceDir = path.resolve(args.source || args["build-from"] || process.cwd());
  const targetBinary = path.resolve(args.binary || defaultRuntimePath());
  const yes = Boolean(args.yes);
  const buildRequested = !args["no-build"] && (args.build || args.install || yes);
  const installRequested = !args["no-install"] && (args.install || yes);
  const killRequested = !args["no-kill"] && (args.kill || yes);
  const buildable = sourceLooksBuildable(sourceDir);
  const beforeVersion = runtimeVersion(targetBinary);
  const processes = listRuntimeProcesses();
  const result = {
    schema: "m1nd-npm-restart-v0",
    package_version: readPackageVersion(),
    source_dir: sourceDir,
    source_buildable: buildable,
    target_binary: targetBinary,
    before_version: beforeVersion,
    after_version: null,
    dry_run: !yes,
    actions: {
      build_requested: buildRequested,
      install_requested: installRequested,
      kill_requested: killRequested,
      built: false,
      installed: false,
      stopped_processes: [],
    },
    visible_runtime_processes: processes,
    next_actions: [],
    non_claims: [
      "m1nd restart does not refresh a host's cached MCP tool list by itself.",
      "m1nd restart does not repair graph contents, ingest roots, or semantic retrieval.",
      "m1nd restart does not select the correct workspace for the agent.",
    ],
  };

  if (buildRequested && !buildable) {
    result.next_actions.push("Run from a m1nd source checkout or pass --source /path/to/m1nd before building.");
  }

  if (yes && buildRequested && buildable) {
    const build = runCommand("cargo", ["build", "--release", "-p", "m1nd-mcp"], { cwd: sourceDir });
    result.actions.build = {
      ok: build.ok,
      status: build.status,
      stderr: build.stderr.trim(),
    };
    result.actions.built = build.ok;
    if (!build.ok) {
      result.next_actions.push("Fix the cargo build failure before installing or rebinding hosts.");
      result.after_version = runtimeVersion(targetBinary);
      return result;
    }
  }

  if (yes && installRequested) {
    const builtBinary = sourceReleaseBinary(sourceDir);
    if (!fs.existsSync(builtBinary)) {
      result.next_actions.push(`Built binary not found at ${builtBinary}; run cargo build --release -p m1nd-mcp first.`);
    } else {
      try {
        installRuntimeBinary(builtBinary, targetBinary);
        result.actions.install = { ok: true, source: builtBinary, target: targetBinary };
        result.actions.installed = true;
        // macOS: an unsigned copy is killed by Gatekeeper (OS_REASON_CODESIGNING)
        // the moment launchd re-execs it. Ad-hoc sign the installed binary so the
        // swap actually survives. A missing codesign tool is a loud warning, not
        // a failure — the install still stands.
        if (process.platform === "darwin") {
          const sign = codesignAdHoc(targetBinary);
          result.actions.codesign = sign;
          if (!sign.ok) {
            result.next_actions.push(
              sign.missing
                ? `codesign not found — the installed binary at ${targetBinary} is UNSIGNED and macOS may kill it (OS_REASON_CODESIGNING); sign it before relying on the swap.`
                : `codesign failed for ${targetBinary}; the binary may be unsigned and killed by macOS on launch — sign it manually before relying on the swap.`
            );
          }
        }
      } catch (error) {
        result.actions.install = {
          ok: false,
          source: builtBinary,
          target: targetBinary,
          error: error instanceof Error ? error.message : String(error),
        };
        result.next_actions.push("Install failed; close live runtimes or use --binary to install to an isolated target, then retry.");
        result.after_version = runtimeVersion(targetBinary);
        return result;
      }
    }
  }

  if (yes && killRequested) {
    // On macOS a m1nd daemon is a launchd-managed service: a plain kill -TERM can
    // race KeepAlive or never reach the service, so the OLD binary keeps running
    // after the swap. Kickstart the managed service that runs the JUST-INSTALLED
    // binary FIRST — it stops then re-execs it reliably. kill -TERM stays as the
    // fallback (and covers non-launchd processes + non-macOS hosts).
    //
    // codesign gate: if the ad-hoc codesign was attempted and FAILED, the
    // installed binary is unsigned and re-execing it would be killed by the OS
    // (OS_REASON_CODESIGNING) — an endless kill/respawn loop. Do NOT kickstart in
    // that case; leave the (old, signed) service running and tell the operator to
    // sign first. kill -TERM is likewise skipped so we don't drop the daemon onto
    // the unsigned binary via KeepAlive.
    if (!shouldKickstartAfterInstall(process.platform, result.actions.codesign)) {
      result.actions.kickstart_skipped = "codesign failed — not re-execing an unsigned binary";
      result.next_actions.push(
        `Skipped reloading launchd services: the installed binary at ${targetBinary} is unsigned (codesign failed) and macOS would kill it on re-exec. Sign it (codesign --force --sign - ${targetBinary}), then re-run.`
      );
    } else {
      const kicked = kickstartManagedServices(targetBinary);
      if (kicked.length > 0) {
        result.actions.kickstarted_services = kicked;
        const failedKicks = kicked.filter((k) => !k.skipped && !k.ok);
        if (failedKicks.length > 0) {
          result.next_actions.push(
            `Some managed m1nd launchd services did not reload (${failedKicks.map((k) => k.label).join(", ")}); check 'launchctl print ${failedKicks[0].domain}'.`
          );
        }
      }
      result.actions.stopped_processes = stopRuntimeProcesses(processes);
    }
    const failedStops = result.actions.stopped_processes.filter((processInfo) => !processInfo.ok);
    if (failedStops.length > 0) {
      result.next_actions.push("Some visible m1nd-mcp processes did not stop; restart the host session or OS if the process state is uninterruptible.");
    }
  }

  result.after_version = runtimeVersion(targetBinary);

  if (!yes) {
    // LOUD dry-run honesty (field bug): without --yes NOTHING is installed, yet
    // the version line reads X -> X and looks like a completed swap. When a
    // source IS installable, say so plainly and name the target, so the dry-run
    // can never be mistaken for the real thing.
    if (buildable && !args["no-install"]) {
      result.next_actions.push(`DRY RUN — nothing was installed; re-run with --yes to swap ${targetBinary}.`);
    }
    result.next_actions.push("Re-run with --yes to build/install/stop processes, or add --no-build/--no-install/--no-kill to narrow the repair.");
  }
  result.next_actions.push("Restart or rebind the MCP host/client so it launches the installed binary.");
  result.next_actions.push("Then run trust_selftest or session_handshake with the intended workspace scope.");
  return result;
}

function runDemo(args) {
  const repo = path.resolve(args.repo || process.cwd());
  const script = path.join(repo, "scripts", "m1nd_agent_demo.py");
  if (!fs.existsSync(script)) {
    throw new Error(`demo script not found at ${script}; run this from a m1nd source checkout`);
  }
  const python = process.env.PYTHON || "python3";
  const commandArgs = [script, "--repo", repo, "--transport", args.transport || "stdio"];
  // --binary names the runtime the demo must exercise, so it has to REACH the
  // smoke step. It used to be accepted and then dropped here, which made a run
  // against an installed runtime fail with the same "binary does not exist:
  // <repo>/target/debug/m1nd-mcp" as a run with no flag at all — two commands,
  // byte-identical errors, and no way to tell the flag had been ignored. A
  // named path that does not exist is refused BY NAME rather than replaced by
  // the default, because a silent fallback is the same lie in a smaller place.
  if (args.binary) {
    const binary = path.resolve(args.binary);
    if (!fs.existsSync(binary)) {
      throw new Error(
        `m1nd-mcp binary does not exist: ${binary}; --binary is never replaced by the default target/debug/m1nd-mcp`
      );
    }
    commandArgs.push("--binary", binary);
  }
  if (args.json) commandArgs.push("--json");
  const result = spawnSync(python, commandArgs, { stdio: "inherit" });
  if (result.error) {
    throw result.error;
  }
  process.exitCode = result.status || 0;
}

function print(value, asJson) {
  if (asJson) {
    console.log(JSON.stringify(value, null, 2));
    return;
  }
  if (Array.isArray(value)) {
    for (const item of value) {
      console.log(`${item.host}:`);
      for (const installed of item.installed) {
        console.log(`  installed ${installed}`);
      }
      if (item.note) console.log(`  ${item.note}`);
    }
    return;
  }
  if (value.schema === "m1nd-npm-doctor-v0") {
    console.log(`m1nd npm package ${value.package_version}`);
    console.log(`pack: ${value.pack_ok ? "ok" : "missing files"}`);
    console.log(`runtime: ${value.runtime.binary || "not found"}${value.runtime.version ? ` (${value.runtime.version})` : ""}`);
    console.log(`codex skills: ${value.codex.skills_installed ? "installed" : "not installed"}`);
    console.log("next:");
    for (const action of value.next_actions) console.log(`  - ${action}`);
    return;
  }
  if (value.schema === "m1nd-npm-restart-v0") {
    console.log(`m1nd restart ${value.dry_run ? "plan" : "result"}`);
    console.log(`source: ${value.source_dir}${value.source_buildable ? "" : " (not buildable)"}`);
    console.log(`target: ${value.target_binary}`);
    console.log(`version: ${value.before_version || "unknown"} -> ${value.after_version || "unknown"}`);
    console.log(`visible m1nd-mcp processes: ${value.visible_runtime_processes.length}`);
    console.log(`built: ${value.actions.built ? "yes" : "no"}`);
    console.log(`installed: ${value.actions.installed ? "yes" : "no"}`);
    console.log(`stopped: ${value.actions.stopped_processes.length}`);
    console.log("next:");
    for (const action of value.next_actions) console.log(`  - ${action}`);
    return;
  }
  if (value.schema === SELF_UPDATE_SCHEMA) {
    console.log(`m1nd update ${value.command}`);
    console.log(`state: ${value.install_state}`);
    if (value.status_summary) console.log(`readiness: ${value.status_summary.readiness}`);
    console.log(`package: ${value.package_version}`);
    console.log(`runtime: ${value.runtime.binary || "not found"}${value.runtime_version ? ` (${value.runtime_version})` : ""}`);
    if (value.runtime.path_binary) console.log(`path runtime: ${value.runtime.path_binary}${value.runtime.path_version ? ` (${value.runtime.path_version})` : ""}`);
    console.log(`channel: ${value.channel}${value.latest_version ? ` -> ${value.latest_version}` : ""}`);
    console.log(`target: ${value.target_version || "unknown"}`);
    console.log(`requires host rebind: ${value.requires_host_rebind ? "yes" : "no"}`);
    if (value.status_summary) {
      console.log(`visible m1nd-mcp processes: ${value.status_summary.active_runtime_process_count}`);
      console.log(`host rebind proven: ${value.status_summary.host_rebind_proven ? "yes" : "no"}`);
    }
    console.log(`planned actions: ${value.planned_actions.length}`);
    for (const planned of value.planned_actions) console.log(`  - ${planned.id}: ${planned.description}`);
    if (value.applied_actions.length > 0) {
      console.log(`applied actions: ${value.applied_actions.length}`);
      for (const applied of value.applied_actions) console.log(`  - ${applied.id}: ${applied.ok ? "ok" : "failed"}`);
    }
    if (value.blocked_actions.length > 0) {
      console.log(`blocked actions: ${value.blocked_actions.length}`);
      for (const blocked of value.blocked_actions) {
        // Print the CAUSE, not just the label. Measured on a stranger install
        // (2026-08-02): human mode said "runtime release install failed" while
        // the --json carried the actionable truth ("cosign not found; install
        // cosign, then retry…"). The person who needs the cause the most is
        // the one who did not think to ask for JSON.
        console.log(`  - ${blocked.id}: ${blocked.description}`);
        if (blocked.error) console.log(`      cause: ${blocked.error}`);
      }
    }
    if (value.next_actions.length > 0) {
      console.log("next:");
      for (const actionText of value.next_actions) console.log(`  - ${actionText}`);
    }
    return;
  }
  if (value.schema === HOST_READINESS_SCHEMA) {
    console.log("m1nd hosts status");
    console.log(`project: ${value.project_dir}`);
    console.log(`runtime: ${value.runtime.binary || "not found"}${value.runtime.version ? ` (${value.runtime.version})` : ""}`);
    console.log(`overall readiness: ${value.summary.overall_readiness}`);
    console.log(`host rebind proven: ${value.summary.host_rebind_proven ? "yes" : "no"}`);
    for (const host of value.hosts) {
      console.log(`${host.host}: ${host.readiness}`);
      console.log(`  agent pack: ${host.agent_pack.installed ? "installed" : "missing"}`);
      console.log(`  config: ${host.config.status}`);
    }
    if (value.next_actions.length > 0) {
      console.log("next:");
      for (const actionText of value.next_actions) console.log(`  - ${actionText}`);
    }
    return;
  }
  if (value.schema === HOST_REBIND_PLAN_SCHEMA) {
    console.log("m1nd hosts plan");
    console.log(`project: ${value.project_dir}`);
    console.log(`overall readiness: ${value.status_summary.overall_readiness}`);
    for (const plan of value.plans) {
      console.log(`${plan.host}: ${plan.readiness}`);
      console.log(`  install: ${plan.install_agent_pack.needed ? plan.install_agent_pack.command : "already installed"}`);
      console.log(`  config: ${plan.configure_mcp.command}`);
      console.log(`  workspace: M1ND_WORKSPACE_ROOT=${plan.workspace_binding.env.M1ND_WORKSPACE_ROOT}`);
      if (plan.hook && plan.hook.event) {
        console.log(`  hook: ${plan.hook.event} -> ${plan.hook.config_path}${plan.hook.os_supported === false ? " (unsupported on this OS)" : ""}`);
      } else if (plan.hook && plan.hook.reason) {
        console.log(`  hook: none (${plan.hook.reason})`);
      }
      if (plan.doctrine) console.log(`  doctrine: ${plan.doctrine.path}`);
      if (plan.settings_block) console.log("  settings block: (printed; paste into settings.json hooks)");
      console.log("  verify:");
      for (const check of plan.verification) console.log(`    - ${check}`);
    }
    return;
  }
  if (value.schema === HOST_APPLY_SCHEMA) {
    console.log(`m1nd hosts apply ${value.dry_run ? "plan" : "result"}`);
    console.log(`project: ${value.project_dir}`);
    console.log(`host selection: ${value.host_selection}`);
    console.log(`requires host rebind: ${value.requires_host_rebind ? "yes" : "no"}`);
    console.log(`host rebind proven: ${value.host_rebind_proven ? "yes" : "no"}`);
    console.log(`planned actions: ${value.planned_actions.length}`);
    console.log(`applied actions: ${value.applied_actions.length}`);
    console.log(`blocked actions: ${value.blocked_actions.length}`);
    if (value.changed_files.length > 0) {
      console.log("changed:");
      for (const file of value.changed_files) console.log(`  - ${file}`);
    }
    for (const h of value.hosts || []) {
      const doc = [...(h.applied_actions || []), ...(h.planned_actions || [])].find((a) => a.kind === "doctrine");
      if (doc) console.log(`  ${h.host} doctrine: ${doc.file}`);
      const hk = [...(h.applied_actions || []), ...(h.blocked_actions || []), ...(h.planned_actions || [])].find((a) => a.kind === "hook");
      if (hk) console.log(`  ${h.host} hook: ${hk.id}`);
    }
    if (value.next_actions.length > 0) {
      console.log("next:");
      for (const actionText of value.next_actions) console.log(`  - ${actionText}`);
    }
    return;
  }
  if (value.schema === KICKSTART_SCHEMA) {
    console.log(`m1nd kickstart ${value.ok ? "ok" : "NOT OK"}`);
    console.log(`trust: ${value.trust_verdict}`);
    console.log(`nodes: ${value.node_count}  edges: ${value.edge_count}`);
    console.log(`ingest: ${value.ingest.performed ? `yes (${value.ingest.files_parsed} files)` : "skipped"}`);
    console.log(`next_action: ${value.next_action}`);
    if (value.audit_summary) console.log(`audit: ${value.audit_summary}`);
    console.log(`timing: trust=${value.timing_ms.trust}ms ingest=${value.timing_ms.ingest}ms audit=${value.timing_ms.audit}ms total=${value.timing_ms.total}ms`);
    if (value.error) console.log(`error: ${value.error}`);
    return;
  }
  if (value.schema === AGENT_CLI_SCHEMA) {
    console.log(`m1nd agent ${value.command}`);
    console.log(`repo: ${value.repo}`);
    console.log(`scope: ${value.scope_alignment.binding_kind}`);
    console.log(`runtime: ${value.runtime.binary || "not found"}${value.runtime.version ? ` (${value.runtime.version})` : ""}`);
    if (value.trust) console.log(`trust: ${value.trust.verdict || "unknown"}`);
    if (value.action && value.action.route) {
      const routeTool = value.action.route.tool ? `/${value.action.route.tool}` : "";
      console.log(`route: ${value.action.route.kind}${routeTool}`);
    }
    if (value.switch_to_direct_proof) console.log("switch to direct proof: yes");
    if (value.action && value.action.action && value.action.action.command) {
      console.log(`action: ${value.action.action.command}`);
    }
    if (value.next_actions.length > 0) {
      console.log("next:");
      for (const actionText of value.next_actions) console.log(`  - ${actionText}`);
    }
    return;
  }
  console.log(String(value));
}

async function main(rawArgs) {
  const args = parseArgs(rawArgs);
  const command = args._[0] === "/restart" ? "restart" : args._[0] || "help";

  if (args.version || ["version", "-V", "--version"].includes(command)) {
    console.log(readPackageVersion());
    return;
  }

  if (args.help || ["help", "-h", "--help"].includes(command)) {
    console.log(usage());
    return;
  }

  if (["init", "install-skills"].includes(command)) {
    // `init --birth <repo>` is the P2 ceremony, not skill installation. The
    // origin stamp lives in the BINARY's own CLI flag (no MCP/REST payload can
    // forge it), so the npm side only relays the human's gesture to the binary
    // it resolves — inherit stdio so the ceremony speaks to the human directly.
    if (command === "init" && args.birth) {
      if (args.confirm !== undefined) {
        console.error("m1nd: --confirm is not supported; a birth accepts only an empty destination");
        process.exitCode = 1;
        return;
      }
      const targetBinary = path.resolve(args.binary || defaultRuntimePath());
      const repoArg = typeof args.birth === "string" ? args.birth : args._[1] || process.cwd();
      const result = spawnSync(targetBinary, ["--birth", path.resolve(repoArg)], {
        stdio: "inherit",
      });
      // The FIRST command this product teaches must never answer with silence.
      // Measured on a stranger install (2026-08-02): with no runtime on disk,
      // spawnSync returns status null with an ENOENT in `result.error`, which
      // this line used to swallow — stdout empty, stderr empty, exit 1. The
      // newcomer's very first step said nothing at all.
      if (result.error || result.status === null) {
        const missing = !fs.existsSync(targetBinary);
        console.error(
          missing
            ? `m1nd: the native runtime is not installed at ${targetBinary}\n` +
              `      install it first:  m1nd update apply --yes\n` +
              `      then run again:    m1nd init --birth ${path.resolve(repoArg)}`
            : `m1nd: could not run the ceremony binary ${targetBinary}: ${result.error ? result.error.message : "unknown spawn failure"}`
        );
        process.exitCode = 1;
        return;
      }
      process.exitCode = result.status;
      return;
    }
    const host = args._[1] || args.host || "generic";
    const projectDir = path.resolve(args.project || process.cwd());
    print(installSkills(host, projectDir), args.json);
    return;
  }

  if (command === "mcp-config") {
    const host = args._[1] || args.host || "generic";
    console.log(mcpConfig(host, args.binary, args.project));
    return;
  }

  if (["host", "hosts"].includes(command)) {
    const subcommand = args._[1] || "status";
    if (subcommand === "status") {
      print(hostStatus(args), args.json);
      return;
    }
    if (["plan", "recipes"].includes(subcommand)) {
      print(hostPlan(args), args.json);
      return;
    }
    if (subcommand === "apply") {
      print(hostApply(args), args.json);
      return;
    }
    throw new Error(`unknown hosts subcommand '${subcommand}'`);
  }

  if (command === "doctor") {
    print(doctor(), args.json);
    return;
  }

  if (command === "restart") {
    print(restart(args), args.json);
    return;
  }

  if (command === "update") {
    print(selfUpdate(args), args.json);
    return;
  }

  if (command === "kickstart") {
    if (!args.repo && !args.project) throw new Error("kickstart requires --repo <path>");
    const result = await agentKickstart(args, {
      defaultRuntimePath,
      findRuntimeBinary,
      runtimeVersion,
    });
    print(result, args.json || !process.stdout.isTTY);
    return;
  }

  if (command === "agent") {
    const result = await agentCommand(args, {
      assertPackShape,
      defaultRuntimePath,
      doctor,
      findRuntimeBinary,
      hostStatus,
      readPackageVersion,
      runtimeVersion,
      selfUpdate,
    });
    print(result, args.json || !process.stdout.isTTY);
    return;
  }

  if (command === "pack-check") {
    const result = assertPackShape();
    if (args.json) {
      console.log(JSON.stringify({ schema: "m1nd-agent-pack-check-v0", ...result }, null, 2));
    } else {
      console.log(result.ok ? "m1nd agent pack ok" : `m1nd agent pack missing: ${result.missing.join(", ")}`);
    }
    if (!result.ok) process.exitCode = 1;
    return;
  }

  if (command === "pack-routing-check") {
    const result = packRoutingCheck();
    if (args.json) {
      console.log(JSON.stringify(result, null, 2));
    } else {
      console.log(
        result.ok
          ? "m1nd agent pack routing ok"
          : `m1nd agent pack routing missing: ${result.missing
              .map((entry) => `${entry.file}:${entry.check}${entry.missing ? `:${entry.missing}` : ""}`)
              .join(", ")}`
      );
    }
    if (!result.ok) process.exitCode = 1;
    return;
  }

  if (["demo", "smoke"].includes(command)) {
    runDemo(args);
    return;
  }

  throw new Error(`unknown command '${command}'\n\n${usage()}`);
}

module.exports = {
  main,
  assertPackShape,
  defaultRuntimePath,
  doctor,
  findRuntimeBinary,
  hostPlan,
  hostApply,
  hostStatus,
  hostRecipe,
  osGateOk,
  renderHookSnippet,
  claudeSettingsBlock,
  installSkills,
  packRoutingCheck,
  restart,
  selfUpdate,
  createSelfUpdateTestHarness,
  agentCommand,
  agentKickstart,
  mcpConfig,
  runtimeBinaryName,
  commandLooksLikeRuntime,
  githubReleaseAssetName,
  githubReleaseAvailability,
  canonicalJson,
  canonicalJsonV1,
  parseIntegerJson,
  domainSeparatedDigest,
  validateCanonicalCandidate,
  validateCanonicalGateReceipt,
  validateCanonicalReviewReceipt,
  validateCanonicalEvidenceSet,
  validateCanonicalCompatibility,
  validateCanonicalRollback,
  verifyCanonicalReleaseVectors,
  versionFromText,
  compareSemver,
  parseLaunchctlLabel,
  parseLaunchctlProgramPath,
  launchdLabelManagesTarget,
  shouldKickstartAfterInstall,
  RUNTIME_VERSION_PROBE_MS,
  RUNTIME_VERSION_PROBE_AFTER_INSTALL_MS,
};
