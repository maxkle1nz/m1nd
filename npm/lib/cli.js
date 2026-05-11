"use strict";

const fs = require("fs");
const os = require("os");
const path = require("path");
const { spawnSync } = require("child_process");

const PACKAGE_ROOT = path.resolve(__dirname, "..", "..");
const SKILLS_ROOT = path.join(PACKAGE_ROOT, "skills");
const UNIVERSAL_PACK = path.join(SKILLS_ROOT, "m1nd-universal-agent-pack.md");
const NPM_PACKAGE = "@maxkle1nz/m1nd";
const SELF_UPDATE_SCHEMA = "m1nd-self-update-v0";
const HOST_READINESS_SCHEMA = "m1nd-host-readiness-v0";
const HOST_REBIND_PLAN_SCHEMA = "m1nd-host-rebind-plan-v0";

const HOST_LIST = ["codex", "claude", "gemini", "antigravity", "generic"];
const HOSTS = new Set([...HOST_LIST, "all"]);

function usage() {
  return `m1nd installer

Usage:
  m1nd init [--host codex|claude|gemini|antigravity|generic|all] [--project <dir>]
  m1nd install-skills <host> [--project <dir>]
  m1nd mcp-config <host> [--binary <path>] [--project <dir>]
  m1nd hosts status [--host codex|claude|gemini|antigravity|generic|all] [--project <dir>] [--binary <path>] [--json]
  m1nd hosts plan [--host codex|claude|gemini|antigravity|generic|all] [--project <dir>] [--binary <path>] [--json]
  m1nd doctor [--json]
  m1nd restart [--source <dir>] [--binary <path>] [--yes] [--json]
  m1nd update check [--channel beta|latest] [--json]
  m1nd update status [--channel beta|latest] [--json]
  m1nd update plan [--channel beta|latest] [--json]
  m1nd update apply [--channel beta|latest] [--yes] [--no-npm] [--no-runtime] [--no-skills] [--no-kill] [--json]
  m1nd update verify [--repo <dir>] [--transport stdio|http] [--json]
  m1nd update rollback [--json]
  m1nd demo [--repo <dir>] [--transport stdio|http] [--json]
  m1nd smoke [--repo <dir>] [--transport stdio|http] [--json]
  m1nd pack-check [--json]

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
workspace, rebind, and verification recipes without editing any host files.`;
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
        "help",
        "install",
        "json",
        "kill",
        "no-build",
        "no-install",
        "no-kill",
        "no-npm",
        "no-runtime",
        "no-skills",
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

function copyDir(source, target) {
  ensureDir(target);
  for (const entry of fs.readdirSync(source, { withFileTypes: true })) {
    const sourcePath = path.join(source, entry.name);
    const targetPath = path.join(target, entry.name);
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

function defaultRuntimePath(platform = process.platform, homeDir = os.homedir()) {
  const pathModule = platform === "win32" ? path.win32 : path;
  return pathModule.join(homeDir, ".m1nd", "bin", runtimeBinaryName(platform));
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
    path.join(SKILLS_ROOT, "m1nd-operator", "references", "runtime-and-refresh.md"),
    path.join(SKILLS_ROOT, "m1nd-operator", "references", "l1ght-and-docs.md"),
    path.join(SKILLS_ROOT, "m1nd-operator", "scripts", "probe_m1nd.py"),
    UNIVERSAL_PACK,
  ];
  const missing = required.filter((file) => !fs.existsSync(file));
  return { ok: missing.length === 0, missing, required };
}

function installCodex() {
  const targetRoot = path.join(os.homedir(), ".codex", "skills");
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
    const skillRoot = path.join(os.homedir(), ".codex", "skills");
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
      return [path.join(os.homedir(), ".codex", "config.toml")];
    case "claude":
      return [path.join(projectDir, ".claude", "mcp.json"), path.join(projectDir, "claude_mcp.json")];
    case "gemini":
      return [path.join(projectDir, ".gemini", "settings.json"), path.join(projectDir, "gemini_mcp.json")];
    case "antigravity":
      return [path.join(projectDir, "mcp_config.json")];
    default:
      return [];
  }
}

function hostConfigStatus(host, projectDir, binary) {
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

  const checked = candidates.map((file) => ({
    file,
    exists: fs.existsSync(file),
    mentions_m1nd: fileContains(file, "m1nd"),
    mentions_workspace_root: fileContains(file, "M1ND_WORKSPACE_ROOT"),
    mentions_project_dir: fileContains(file, projectDir),
  }));
  const configured = checked.some((candidate) => candidate.exists && candidate.mentions_m1nd);
  const workspaceConfigured = checked.some(
    (candidate) =>
      candidate.exists &&
      candidate.mentions_m1nd &&
      candidate.mentions_workspace_root &&
      candidate.mentions_project_dir
  );
  const anyPresent = checked.some((candidate) => candidate.exists);
  return {
    status: configured ? "configured" : anyPresent ? "present_without_m1nd" : "missing",
    workspace_configured: workspaceConfigured,
    candidates: checked,
    expected_command: binary || defaultRuntimePath(),
    snippet_command: snippetCommand,
    note: configured
      ? "A config candidate mentions m1nd; workspace env and host rebind are still checked separately."
      : "No config candidate currently proves that this host is wired to m1nd.",
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
  ];
}

function hostInstallCommand(host, projectDir) {
  if (host === "codex") return "m1nd install-skills codex";
  return `m1nd install-skills ${host} --project ${projectDir}`;
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
    const config = hostConfigStatus(host, projectDir, binary);
    const workspaceReady = workspace.status === "aligned" || Boolean(config.workspace_configured);
    const configReady = config.status === "configured" && workspaceReady;
    const readiness = runtimeCurrent && pathRuntimeCurrent && agentPack.installed && configReady ? "ready" : "attention";
    const nextActions = [];
    if (!runtimeCurrent) {
      nextActions.push("Run m1nd update status --channel beta --json, then m1nd update plan/apply if the runtime is stale or missing.");
    }
    if (!pathRuntimeCurrent) {
      nextActions.push("Align the m1nd-mcp binary found on PATH or pass --binary to target the runtime this host launches.");
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
    if (workspace.status !== "aligned") {
      nextActions.push(workspace.recommendation);
    }
    nextActions.push("Restart/rebind the host, then call trust_selftest or session_handshake before retrieval.");

    return {
      host,
      readiness,
      agent_pack: agentPack,
      config,
      workspace,
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
    return {
      host: host.host,
      readiness: host.readiness,
      read_only: true,
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

function doctor() {
  const pack = assertPackShape();
  const binary = findRuntimeBinary();
  const binaryVersion = runtimeVersion(binary);
  const packageVersion = readPackageVersion();
  const codexSkillRoot = path.join(os.homedir(), ".codex", "skills");
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
    result.next_actions.push(`Runtime version ${binaryVersion || "unknown"} does not match package ${packageVersion}; run m1nd update plan --channel beta, then m1nd update apply --channel beta --yes and rebind the host.`);
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
  const normalized = channel || "beta";
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
  return process.env.M1ND_UPDATE_STATE_PATH || path.join(os.homedir(), ".m1nd", "update-state.json");
}

function updateBackupPath(targetBinary, beforeVersion) {
  const safeVersion = versionFromText(beforeVersion) || "unknown";
  const stamp = new Date().toISOString().replace(/[:.]/g, "-");
  const backupRoot = process.env.M1ND_UPDATE_BACKUP_DIR || path.join(os.homedir(), ".m1nd", "backups");
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
    proof.next_actions.push("Run m1nd update plan --channel beta --json, inspect actions, then apply with --yes when ready.");
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
      console.log("  verify:");
      for (const check of plan.verification) console.log(`    - ${check}`);
    }
    return;
  }
  console.log(String(value));
}

async function main(rawArgs) {
  const args = parseArgs(rawArgs);
  const command = args._[0] === "/restart" ? "restart" : args._[0] || "help";

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
  hostStatus,
  installSkills,
  restart,
  selfUpdate,
  mcpConfig,
  runtimeBinaryName,
  commandLooksLikeRuntime,
  githubReleaseAssetName,
  versionFromText,
  compareSemver,
};
