"use strict";

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
  m1nd agent first-minute --repo <dir> --query <text> [--mode short|normal|deep] [--json]
  m1nd agent orient --repo <dir> --query <text> [--mode short|normal|deep] [--tool auto|search|seek|activate|audit|glob] [--json]
  m1nd agent auto --repo <dir> [--query <text> | --from <error|payload|stdin>] [--mode short|normal|deep] [--tool auto|search|seek|activate|audit|glob] [--json]
  m1nd agent next --repo <dir> [--query <text> | --from <error|payload|stdin>] [--mode short|normal|deep] [--tool auto|search|seek|activate|audit|glob] [--json]
  m1nd agent recover --repo <dir> --from <error|payload|stdin> [--json]
  m1nd agent context --repo <dir> --query <text> [--anchor <file>] [--allow-discovery] [--tokens <n>] [--json]
  m1nd agent handoff --repo <dir> [--from last-run|mission] [--json]
  m1nd agent doctor --repo <dir> [--json]
  m1nd kickstart --repo <dir> [--audit-path <dir>] [--binary <path>] [--json]
  m1nd demo [--repo <dir>] [--transport stdio|http] [--json]
  m1nd smoke [--repo <dir>] [--transport stdio|http] [--json]
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

agent is the host-neutral operating layer for coding agents. It launches an
isolated m1nd-mcp runtime bound to --repo, emits deterministic JSON outside any
stale MCP host, and tells the agent when to switch back to direct proof.

agent auto/next is the deterministic route picker. It does not claim proof; it
chooses the next bounded agent step or hands control back to direct proof. For
deep architecture, hidden coupling, security/taint, duplication/refactor, or
runtime-heat tasks, it emits RETROBUILDER capability_suggestions.

agent first-minute is the safest first contact for a new repo. It scopes,
trusts, ingests when needed, runs one bounded orientation pass, returns anchors,
and then tells the agent to prove directly. It can also surface RETROBUILDER
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
    path.join(SKILLS_ROOT, "m1nd-operator", "SKILL.md"),
    path.join(SKILLS_ROOT, "m1nd-operator", "references", "routing-playbooks.md"),
    path.join(SKILLS_ROOT, "m1nd-operator", "references", "tool-families.md"),
    path.join(SKILLS_ROOT, "m1nd-operator", "references", "full-spec-agent-os.md"),
    path.join(SKILLS_ROOT, "m1nd-operator", "references", "runtime-and-refresh.md"),
    path.join(SKILLS_ROOT, "m1nd-operator", "references", "l1ght-and-docs.md"),
    path.join(SKILLS_ROOT, "m1nd-operator", "scripts", "probe_m1nd.py"),
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
      { id: "dext3r-continuity-only", needles: ["DEXT3R", "continuity"] },
      { id: "m1nd-agent-next-route", needles: ["m1nd agent next", "current task"] },
      { id: "m1nd-agent-first-minute-route", needles: ["m1nd agent first-minute", "first contact"] },
      { id: "retrobuilder-routing", needles: ["RETROBUILDER", "ghost_edges", "runtime_overlay", "direct source"] },
      { id: "no-companion-code-truth", needles: ["code truth"] },
      { id: "direct-proof-final-truth", needles: ["direct proof", "decides what is true"] },
    ],
  },
  {
    id: "m1nd-operator",
    relative_path: "skills/m1nd-operator/SKILL.md",
    checks: [
      { id: "session-companion-routing", needles: ["Session Companion Routing"] },
      { id: "dext3r-continuity", needles: ["DEXT3R", "conversation continuity"] },
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
      { id: "dext3r-continuity", needles: ["DEXT3R", "continuity"] },
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
      { id: "dext3r-continuity", needles: ["DEXT3R", "continuity"] },
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
      "pack-routing-check does not prove a session companion such as DEXT3R is installed.",
      "pack-routing-check does not prove m1nd retrieval correctness or code behavior.",
      "pack-routing-check does not refresh MCP host bindings or cached tool lists.",
    ],
  };
}

function installCodex() {
  const targetRoot = path.join(homeDir(), ".codex", "skills");
  copyDir(path.join(SKILLS_ROOT, "m1nd-first"), path.join(targetRoot, "m1nd-first"));
  copyDir(path.join(SKILLS_ROOT, "m1nd-operator"), path.join(targetRoot, "m1nd-operator"));
  return {
    host: "codex",
    installed: [
      path.join(targetRoot, "m1nd-first"),
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
    operator_skill: path.join(skillsRoot, "m1nd-operator", "SKILL.md"),
    rule_file: path.join(targetRoot, hostRuleFilename(host)),
  };
}

function agentPackStatusForHost(host, projectDir) {
  if (host === "codex") {
    const skillRoot = path.join(homeDir(), ".codex", "skills");
    const required = [
      path.join(skillRoot, "m1nd-first", "SKILL.md"),
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
  const required = [paths.first_skill, paths.operator_skill, paths.rule_file];
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
      mentions_project_dir: scopedContent.includes(projectDir),
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
    result.next_actions.push(`From a source checkout: cargo build --release -p m1nd-mcp, then copy ${runtimeBinaryName()} to ${defaultRuntimePath()}`);
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

function runtimeVersion(binary) {
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
  const result = runCommand(binary, ["--version"], { timeout: 1500 });
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

  const tagsResult = runCommand("npm", ["view", NPM_PACKAGE, "dist-tags", "--json"], { timeout: 7000 });
  const versionResult = runCommand("npm", ["view", NPM_PACKAGE, "version", "--json"], { timeout: 7000 });
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
  const result = runCommand("cargo", ["search", crateName, "--limit", "1"], { timeout: 10000 });
  const match = result.stdout.match(new RegExp(`^${crateName}\\s*=\\s*"([^"]+)"`, "m"));
  return {
    ok: result.ok && Boolean(match),
    crate: crateName,
    version: match ? match[1] : null,
    source: "cargo-search",
    error: result.ok ? null : (result.stderr || result.error || "").trim(),
  };
}

function githubReleaseAssetName(platform = process.platform, arch = process.arch) {
  if (platform === "darwin" && arch === "arm64") return "m1nd-mcp-macos-aarch64";
  if (platform === "darwin" && arch === "x64") return "m1nd-mcp-macos-x86_64";
  if (platform === "linux" && arch === "x64") return "m1nd-mcp-linux-x86_64";
  return null;
}

function githubReleaseAssetUrl(version, platform = process.platform, arch = process.arch) {
  const asset = githubReleaseAssetName(platform, arch);
  if (!asset || !version) return null;
  return `https://github.com/maxkle1nz/m1nd/releases/download/v${version}/${asset}`;
}

function githubReleaseAvailability(version, platform = process.platform, arch = process.arch) {
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
  if (process.env.M1ND_TEST_RELEASE_ASSET_PATH) {
    return {
      ok: true,
      available: true,
      asset,
      url,
      source: "M1ND_TEST_RELEASE_ASSET_PATH",
      error: null,
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
  const curl = which("curl");
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
  const result = runCommand(curl, ["-fsI", "-L", url], { timeout: 8000 });
  return {
    ok: result.ok,
    available: result.ok,
    asset,
    url,
    source: "github-release-head",
    error: result.ok ? null : (result.stderr || result.error || "").trim(),
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

function buildSelfUpdateProof(args, command = "check") {
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
  const release = githubReleaseAvailability(targetVersion);
  const plannedActions = [];
  const blockedActions = [];
  const staleSurfaces = [];
  let unknown = false;

  if (registry.latest_version) {
    const npmComparison = compareSemver(packageVersion, registry.latest_version);
    if (npmComparison !== null && npmComparison < 0 && !args["no-npm"]) {
      staleSurfaces.push("npm-package");
      plannedActions.push(action("npm-install", "npm", `install ${NPM_PACKAGE}@${channel}`, {
        package: NPM_PACKAGE,
        channel,
        target_version: registry.latest_version,
        command: `npm install -g ${NPM_PACKAGE}@${channel}`,
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
      plannedActions.push(runtimeInstallAction(release, crate, targetVersion, targetBinary, "runtime missing"));
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
      plannedActions.push(runtimeInstallAction(release, crate, targetVersion, targetBinary, "runtime stale or unknown"));
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
    non_claims: selfUpdateNonClaims(),
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
  return action("runtime-install-cargo", "runtime", `install native runtime ${targetVersion} with cargo fallback`, {
    reason,
    source: "cargo-install",
    crate: "m1nd-mcp",
    crate_version: crate.version,
    target_binary: targetBinary,
    target_version: targetVersion,
    release_error: release.error,
  });
}

function installRuntimeBinaryWithBackup(sourceBinary, targetBinary) {
  const beforeVersion = runtimeVersion(targetBinary);
  let backup = null;
  if (fs.existsSync(targetBinary)) {
    backup = updateBackupPath(targetBinary, beforeVersion);
    ensureDir(path.dirname(backup));
    fs.copyFileSync(targetBinary, backup);
    if (process.platform !== "win32") fs.chmodSync(backup, 0o755);
  }
  installRuntimeBinary(sourceBinary, targetBinary);
  const state = {
    schema: "m1nd-self-update-rollback-state-v0",
    created_at: new Date().toISOString(),
    target_binary: targetBinary,
    backup_binary: backup,
    before_version: beforeVersion,
    after_version: runtimeVersion(targetBinary),
  };
  ensureDir(path.dirname(updateStatePath()));
  fs.writeFileSync(updateStatePath(), `${JSON.stringify(state, null, 2)}\n`);
  return state;
}

function stageReleaseAsset(planned) {
  if (process.env.M1ND_TEST_RELEASE_ASSET_PATH) {
    return process.env.M1ND_TEST_RELEASE_ASSET_PATH;
  }
  const curl = which("curl");
  if (!curl) throw new Error("curl not found; cannot download GitHub release asset");
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "m1nd-update-"));
  const target = path.join(dir, runtimeBinaryName());
  const result = runCommand(curl, ["-fL", planned.url, "-o", target], { timeout: 120000 });
  if (!result.ok) {
    throw new Error((result.stderr || result.error || "GitHub release download failed").trim());
  }
  if (process.platform !== "win32") fs.chmodSync(target, 0o755);
  return target;
}

function cargoInstallRuntime(planned) {
  const targetBinary = path.resolve(planned.target_binary);
  const rootDir = path.dirname(path.dirname(targetBinary));
  const binDir = path.basename(path.dirname(targetBinary));
  if (binDir !== "bin") {
    return {
      ok: false,
      error: "cargo fallback only supports targets inside a <root>/bin directory",
    };
  }
  const args = ["install", "m1nd-mcp", "--version", planned.target_version, "--force", "--root", rootDir];
  const result = runCommand("cargo", args, { timeout: 300000 });
  return {
    ok: result.ok,
    status: result.status,
    stderr: result.stderr.trim(),
    stdout: result.stdout.trim(),
    command: `cargo ${args.join(" ")}`,
  };
}

function applySelfUpdate(args) {
  const proof = buildSelfUpdateProof(args, "apply");
  const yes = Boolean(args.yes);
  proof.dry_run = !yes;
  if (!yes) {
    proof.next_actions.push("Re-run with --yes to apply the planned update actions.");
    proof.next_actions.push("Use --no-npm, --no-runtime, --no-skills, or --no-kill to narrow the apply surface.");
    return proof;
  }

  for (const planned of proof.planned_actions) {
    if (planned.id === "npm-install") {
      if (args["no-npm"]) continue;
      const result = runCommand("npm", ["install", "-g", `${NPM_PACKAGE}@${proof.channel}`], { timeout: 180000 });
      const applied = {
        id: planned.id,
        kind: planned.kind,
        ok: result.ok,
        status: result.status,
        stderr: result.stderr.trim(),
      };
      proof.applied_actions.push(applied);
      if (!result.ok) proof.blocked_actions.push(action("npm-install-failed", "npm", "npm global package update failed", applied));
    }

    if (planned.id === "runtime-install-github-release") {
      if (args["no-runtime"]) continue;
      try {
        const source = stageReleaseAsset(planned);
        const state = installRuntimeBinaryWithBackup(source, planned.target_binary);
        proof.applied_actions.push({
          id: planned.id,
          kind: planned.kind,
          ok: Boolean(process.env.M1ND_TEST_RELEASE_ASSET_PATH) || Boolean(state.after_version && state.after_version.includes(planned.target_version)),
          source,
          target_binary: planned.target_binary,
          rollback_state: updateStatePath(),
          backup_binary: state.backup_binary,
          before_version: state.before_version,
          after_version: state.after_version,
          version_verified: Boolean(state.after_version && state.after_version.includes(planned.target_version)),
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
      }
    }

    if (planned.id === "runtime-install-cargo") {
      if (args["no-runtime"]) continue;
      const result = cargoInstallRuntime(planned);
      if (result.ok && fs.existsSync(planned.target_binary)) {
        const state = {
          schema: "m1nd-self-update-rollback-state-v0",
          created_at: new Date().toISOString(),
          target_binary: planned.target_binary,
          backup_binary: null,
          before_version: proof.runtime_version,
          after_version: runtimeVersion(planned.target_binary),
        };
        ensureDir(path.dirname(updateStatePath()));
        fs.writeFileSync(updateStatePath(), `${JSON.stringify(state, null, 2)}\n`);
      }
      proof.applied_actions.push({
        id: planned.id,
        kind: planned.kind,
        ...result,
        version_verified: result.ok ? Boolean(runtimeVersion(planned.target_binary) && runtimeVersion(planned.target_binary).includes(planned.target_version)) : false,
      });
      if (!result.ok) proof.blocked_actions.push(action("runtime-cargo-install-failed", "runtime", "cargo runtime install failed", result));
      if (result.ok && !proof.applied_actions[proof.applied_actions.length - 1].version_verified) {
        proof.blocked_actions.push(action("runtime-version-mismatch-after-install", "runtime", "installed cargo runtime did not report the target version", {
          target_version: planned.target_version,
          after_version: runtimeVersion(planned.target_binary),
        }));
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
      proof.applied_actions.push({
        id: planned.id,
        kind: planned.kind,
        ok: true,
        stopped_processes: stopRuntimeProcesses(listRuntimeProcesses()),
      });
    }
  }

  proof.runtime_version_after = runtimeVersion(proof.runtime.target_binary);
  proof.requires_host_rebind =
    proof.requires_host_rebind ||
    proof.applied_actions.some((applied) => ["npm", "runtime", "agent-pack", "process"].includes(applied.kind));
  if (proof.requires_host_rebind) {
    proof.next_actions.push("Restart or rebind each MCP host/client so it launches the updated runtime and refreshes its cached tool list.");
  }
  proof.next_actions.push("Then run m1nd update verify, trust_selftest, or session_handshake with the intended workspace scope.");
  return proof;
}

function verifySelfUpdate(args) {
  const proof = buildSelfUpdateProof(args, "verify");
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

function buildSelfUpdateStatus(args) {
  const proof = buildSelfUpdateProof(args, "status");
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

function rollbackSelfUpdate(args) {
  const proof = buildSelfUpdateProof(args, "rollback");
  proof.requires_host_rebind = true;
  const statePath = updateStatePath();
  if (!fs.existsSync(statePath)) {
    proof.blocked_actions.push(action("rollback-state-missing", "rollback", "no local update rollback state exists", {
      state_path: statePath,
    }));
    return proof;
  }
  const state = safeJsonParse(fs.readFileSync(statePath, "utf8"));
  if (!state || !state.backup_binary || !fs.existsSync(state.backup_binary)) {
    proof.blocked_actions.push(action("rollback-backup-missing", "rollback", "rollback state has no usable runtime backup", {
      state_path: statePath,
      backup_binary: state ? state.backup_binary : null,
    }));
    return proof;
  }
  installRuntimeBinary(state.backup_binary, state.target_binary);
  proof.applied_actions.push({
    id: "runtime-rollback",
    kind: "rollback",
    ok: true,
    target_binary: state.target_binary,
    backup_binary: state.backup_binary,
    restored_version: runtimeVersion(state.target_binary),
  });
  proof.next_actions.push("Restart or rebind each MCP host/client so it launches the restored runtime.");
  return proof;
}

function selfUpdate(args) {
  const subcommand = args._[1] || "check";
  switch (subcommand) {
    case "check":
    case "plan":
      return buildSelfUpdateProof(args, subcommand);
    case "status":
      return buildSelfUpdateStatus(args);
    case "apply":
      return applySelfUpdate(args);
    case "verify":
      return verifySelfUpdate(args);
    case "rollback":
      return rollbackSelfUpdate(args);
    default:
      throw new Error(`unknown update subcommand '${subcommand}'`);
  }
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
    `.${path.basename(targetBinary)}.${process.pid}.tmp`
  );
  fs.copyFileSync(sourceBinary, tempTarget);
  if (process.platform !== "win32") fs.chmodSync(tempTarget, 0o755);
  fs.renameSync(tempTarget, targetBinary);
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
    result.actions.stopped_processes = stopRuntimeProcesses(processes);
    const failedStops = result.actions.stopped_processes.filter((processInfo) => !processInfo.ok);
    if (failedStops.length > 0) {
      result.next_actions.push("Some visible m1nd-mcp processes did not stop; restart the host session or OS if the process state is uninterruptible.");
    }
  }

  result.after_version = runtimeVersion(targetBinary);

  if (!yes) {
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
      for (const blocked of value.blocked_actions) console.log(`  - ${blocked.id}: ${blocked.description}`);
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
  agentCommand,
  agentKickstart,
  mcpConfig,
  runtimeBinaryName,
  commandLooksLikeRuntime,
  githubReleaseAssetName,
  versionFromText,
  compareSemver,
};
