"use strict";
const assert = require("node:assert/strict");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const test = require("node:test");
const { capsuleFiles } = require("../lib/agent-capsule-files");

test("selects canonical anchor and returned connections without silently truncating", () => {
  const root = path.resolve("repo-alpha");
  const bound = value => { const p = path.resolve(root, value); if (!p.startsWith(root + path.sep)) throw Error("outside"); return p; };
  const anchor = path.join(root, "a.js");
  assert.deepEqual(capsuleFiles(anchor, {}, bound), { ok: true, scope: "selected_anchor_only", files: [anchor] });
  assert.deepEqual(capsuleFiles(anchor, { connected_files: [{ file_path: "b.js" }, { file_path: "b.js" }] }, bound),
    { ok: true, scope: "anchor_and_returned_connected_files", files: [anchor, path.join(root, "b.js")] });
  for (const connected_files of [null, {}, [{ file_path: "../escape" }], [{}], [{ file_path: "x\0y" }], Array(6).fill({file_path:"b.js"})]) {
    assert.equal(capsuleFiles(anchor, { connected_files }, bound).ok, false);
  }
});

test("containment callback rejects canonical symlink escapes", () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "m1nd-capsule-"));
  try {
    const repo = path.join(root, "repo-alpha"); fs.mkdirSync(repo);
    const outside = path.join(root, "outside.js"); fs.writeFileSync(outside, "source");
    fs.symlinkSync(outside, path.join(repo, "link.js"));
    const bound = value => { const p = fs.realpathSync(path.resolve(repo, value)); if (!p.startsWith(repo + path.sep)) throw Error("outside"); return p; };
    fs.writeFileSync(path.join(repo, "a.js"), "anchor");
    assert.equal(capsuleFiles(path.join(repo,"a.js"), {connected_files:[{file_path:"link.js"}]}, bound).ok, false);
  } finally { fs.rmSync(root, {recursive:true,force:true}); }
});

test("every declared primary source must canonically match the selected anchor", () => {
  const root = path.resolve("repo-alpha"), anchor = path.join(root, "a.js");
  const bound = value => { const p = path.resolve(root, value); if (!p.startsWith(root + path.sep)) throw Error("outside"); return p; };
  for (const payload of [
    { file_path: "../outside.js", connected_files: [] },
    { file_path: "b.js", connected_files: [] },
    { file_path: null },
    { primary_file: { file_path: "../outside.js" }, connected_files: [] },
    { primary_file: { file_path: "b.js" }, connected_files: [] },
    { primary_file: null },
    { file_path: "a.js", primary_file: { file_path: "b.js" } },
  ]) assert.equal(capsuleFiles(anchor, payload, bound).ok, false, JSON.stringify(payload));
  assert.equal(capsuleFiles(anchor, { file_path: "./a.js", primary_file: { file_path: anchor }, connected_files: [] }, bound).ok, true);
});
