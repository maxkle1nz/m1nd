"use strict";

const fs = require("fs");
const os = require("os");
const path = require("path");
const { spawnSync } = require("child_process");

const PACKAGE_ROOT = path.resolve(__dirname, "..", "..");
const SKILLS_ROOT = path.join(PACKAGE_ROOT, "skills");
const UNIVERSAL_PACK = path.join(SKILLS_ROOT, "m1nd-universal-agent-pack.md");

const HOSTS = new Set(["codex", "claude", "gemini", "antigravity", "generic", "all"]);

function usage() {
  return `m1nd installer

Usage:
  m1nd init [--host codex|claude|gemini|antigravity|generic|all] [--project <dir>]
  m1nd install-skills <host> [--project <dir>]
  m1nd mcp-config <host> [--binary <path>]
  m1nd doctor [--json]
  m1nd restart [--source <dir>] [--binary <path>] [--yes] [--json]
  m1nd demo [--repo <dir>] [--transport stdio|http] [--json]
  m1nd smoke [--repo <dir>] [--transport stdio|http] [--json]
  m1nd pack-check [--json]

This npm package installs the universal agent doctrine and host adapters. The
native runtime is still m1nd-mcp; doctor tells you whether it is visible.

restart is an external repair helper for stale host bindings. Without --yes it
prints the plan. With --yes it builds from source when available, installs the
native binary to the m1nd default path, and stops visible m1nd-mcp processes so
the host can relaunch/rebind.`;
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
    if (["build", "help", "install", "json", "kill", "no-build", "no-install", "no-kill", "yes"].includes(key)) {
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

function mcpConfig(host, binary) {
  const command = binary || findRuntimeBinary() || defaultRuntimePath();
  if (host === "codex") {
    const escapedCommand = command.replace(/\\/g, "\\\\");
    return `[mcp_servers.m1nd]
command = "${escapedCommand}"
args = ["--stdio", "--no-gui"]
`;
  }
  return JSON.stringify(
    {
      mcpServers: {
        m1nd: {
          command,
          args: ["--stdio", "--no-gui"],
        },
      },
    },
    null,
    2
  );
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
    result.next_actions.push(`Runtime version ${binaryVersion || "unknown"} does not match package ${packageVersion}; run m1nd restart --source /path/to/m1nd --yes, then rebind the host.`);
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
  const result = runCommand(binary, ["--version"], { timeout: 1500 });
  if (result.error && result.error.includes("ETIMEDOUT")) return "version-check-timeout";
  if (!result.ok) return null;
  return result.stdout.trim() || null;
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
    console.log(mcpConfig(host, args.binary));
    return;
  }

  if (command === "doctor") {
    print(doctor(), args.json);
    return;
  }

  if (command === "restart") {
    print(restart(args), args.json);
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
  installSkills,
  restart,
  mcpConfig,
  runtimeBinaryName,
  commandLooksLikeRuntime,
};
