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
  m1nd demo [--repo <dir>] [--transport stdio|http] [--json]
  m1nd pack-check [--json]

This npm package installs the universal agent doctrine and host adapters. The
native runtime is still m1nd-mcp; doctor tells you whether it is visible.`;
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
    if (["json", "yes"].includes(key)) {
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
  return process.env.M1ND_MCP_BINARY || which("m1nd-mcp") || (fs.existsSync(defaultRuntimePath()) ? defaultRuntimePath() : null);
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
  const codexSkillRoot = path.join(os.homedir(), ".codex", "skills");
  const codexSkillsInstalled =
    fs.existsSync(path.join(codexSkillRoot, "m1nd-first", "SKILL.md")) &&
    fs.existsSync(path.join(codexSkillRoot, "m1nd-operator", "SKILL.md"));

  const result = {
    schema: "m1nd-npm-doctor-v0",
    package_version: readPackageVersion(),
    package_root: PACKAGE_ROOT,
    pack_ok: pack.ok,
    missing_pack_files: pack.missing,
    runtime: {
      platform: process.platform,
      arch: process.arch,
      binary: binary || null,
      default_install_path: defaultRuntimePath(),
      visible_on_path_or_env: Boolean(binary),
      hint: binary
        ? "m1nd-mcp is visible. Use m1nd demo from a repo checkout for a live MCP smoke."
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
  if (!codexSkillsInstalled) {
    result.next_actions.push("For Codex: m1nd install-skills codex");
  }
  if (result.next_actions.length === 0) {
    result.next_actions.push("Run trust_selftest in your MCP host, then ingest your repo.");
  }
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
    console.log(`runtime: ${value.runtime.binary || "not found"}`);
    console.log(`codex skills: ${value.codex.skills_installed ? "installed" : "not installed"}`);
    console.log("next:");
    for (const action of value.next_actions) console.log(`  - ${action}`);
    return;
  }
  console.log(String(value));
}

async function main(rawArgs) {
  const args = parseArgs(rawArgs);
  const command = args._[0] || "help";

  if (["help", "-h", "--help"].includes(command)) {
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

  if (command === "demo") {
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
  mcpConfig,
  runtimeBinaryName,
};
