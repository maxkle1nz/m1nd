"use strict";

const assert = require("node:assert/strict");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const test = require("node:test");

const { assertPackContents, dryRunPack, REQUIRED_PUBLISHED_PATHS } = require("../lib/pack-contents-check");

test("pack contents require the agent-autonomy guide", () => {
  const files = REQUIRED_PUBLISHED_PATHS
    .filter((entry) => entry !== "docs/AGENT-AUTONOMY.md")
    .map((path) => ({ path }));

  assert.throws(
    () => assertPackContents([{ name: "@maxkle1nz/m1nd", files }]),
    /docs\/AGENT-AUTONOMY\.md/,
  );
});

test("pack contents accept every required published path", () => {
  const files = REQUIRED_PUBLISHED_PATHS.map((path) => ({ path }));
  assert.doesNotThrow(() => assertPackContents([{ name: "@maxkle1nz/m1nd", files }]));
});

test("pack dry run invokes the npm JavaScript CLI through Node", () => {
  const fixture = fs.mkdtempSync(path.join(os.tmpdir(), "m1nd-pack-cli-"));
  const npmCli = path.join(fixture, "npm-cli.js");
  const previous = process.env.npm_execpath;
  try {
    fs.writeFileSync(npmCli, 'process.stdout.write(JSON.stringify([{ name: "fixture-npm-cli", files: [] }]));\n');
    process.env.npm_execpath = npmCli;
    assert.equal(dryRunPack()[0].name, "fixture-npm-cli");
  } finally {
    if (previous === undefined) delete process.env.npm_execpath;
    else process.env.npm_execpath = previous;
    fs.rmSync(fixture, { recursive: true, force: true });
  }
});
