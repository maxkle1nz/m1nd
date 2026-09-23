"use strict";

const { spawnSync } = require("node:child_process");
const path = require("node:path");

const PACKAGE_ROOT = path.resolve(__dirname, "..", "..");
const REQUIRED_PUBLISHED_PATHS = Object.freeze([
  "LICENSE",
  "README.md",
  "EXAMPLES.md",
  "package.json",
  "docs/AGENT-PACKS.md",
  "docs/AGENT-FIRST-DEMO.md",
  "docs/AGENT-AUTONOMY.md",
  "docs/M1ND-GUARDIAN-METHOD.md",
  "docs/MCP-HOST-REFRESH.md",
  "docs/IDE-INTEGRATIONS.md",
  "npm/bin/m1nd.js",
  "npm/lib/agent-cli.js",
  "npm/lib/agent-runtime-cache.js",
  "npm/lib/mcp-runtime-client.js",
  "npm/lib/cli.js",
]);

function assertPackContents(records) {
  if (!Array.isArray(records) || records.length !== 1 || !records[0] || !Array.isArray(records[0].files)) {
    throw new Error("npm pack did not return exactly one file manifest");
  }
  const published = new Set(
    records[0].files.map((entry) => (typeof entry === "string" ? entry : entry && entry.path)).filter(Boolean),
  );
  const missing = REQUIRED_PUBLISHED_PATHS.filter((entry) => !published.has(entry));
  if (missing.length > 0) {
    throw new Error(`npm pack missing required published paths: ${missing.join(", ")}`);
  }
}

function dryRunPack() {
  // npm.cmd is not directly spawnable on Windows without a shell. npm scripts
  // provide the JavaScript CLI path; use Node to invoke it on every platform.
  const npmCli = process.env.npm_execpath ||
    (process.platform === "win32"
      ? path.join(path.dirname(process.execPath), "node_modules", "npm", "bin", "npm-cli.js")
      : null);
  if (process.platform === "win32" && (!npmCli || !npmCli.endsWith(".js"))) {
    throw new Error("npm pack check requires the npm JavaScript CLI on Windows");
  }
  const result = spawnSync(npmCli ? process.execPath : "npm",
    [...(npmCli ? [npmCli] : []), "pack", "--dry-run", "--json"], {
      cwd: PACKAGE_ROOT,
      encoding: "utf8",
    });
  if (result.error) {
    throw result.error;
  }
  if (result.status !== 0) {
    throw new Error(`npm pack --dry-run failed (${result.status}): ${result.stderr || result.stdout}`);
  }
  try {
    return JSON.parse(result.stdout);
  } catch (error) {
    throw new Error(`npm pack --dry-run returned invalid JSON: ${error.message}`);
  }
}

function main() {
  const records = dryRunPack();
  assertPackContents(records);
  process.stdout.write(
    `${JSON.stringify({
      status: "ok",
      package: records[0].name,
      version: records[0].version,
      required_paths: REQUIRED_PUBLISHED_PATHS,
    })}\n`,
  );
}

if (require.main === module) {
  try {
    main();
  } catch (error) {
    process.stderr.write(`${error.message}\n`);
    process.exitCode = 1;
  }
}

module.exports = { REQUIRED_PUBLISHED_PATHS, assertPackContents, dryRunPack };
