"use strict";

const crypto = require("crypto");
const fs = require("fs");
const path = require("path");

// Called only after confirmed native shutdown, while the automatic cache lease
// is still held. This versions persisted BYTES, not semantic completeness or
// an atomic source-tree snapshot. No source contents leave this process.
function graphSnapshotVersion(runtimeDir, refresh) {
  const receipt = {
    schema: "m1nd-agent-graph-version-v1",
    status: "unavailable",
    update_status: refresh?.attempted ? (refresh.ok ? "refreshed" : "refused") : "not_observed",
    refresh_refusal: refresh?.refused || null,
    code_snapshot: refresh?.code_snapshot || null,
    semantic_completeness: "not_proven",
    atomic_source_snapshot: false,
  };
  let fd;
  try {
    const root = fs.lstatSync(runtimeDir);
    if (!root.isDirectory() || root.isSymbolicLink()) throw new Error("unsafe_runtime_directory");
    const file = path.join(runtimeDir, "graph_snapshot.json");
    const before = fs.lstatSync(file);
    if (!before.isFile() || before.isSymbolicLink()) throw new Error("unsafe_snapshot_file");
    fd = fs.openSync(file, fs.constants.O_RDONLY | (fs.constants.O_NOFOLLOW || 0));
    const opened = fs.fstatSync(fd);
    if (!opened.isFile() || opened.dev !== before.dev || opened.ino !== before.ino) throw new Error("snapshot_changed");
    const hash = crypto.createHash("sha256");
    const buffer = Buffer.alloc(64 * 1024);
    let size = 0;
    for (let bytes; (bytes = fs.readSync(fd, buffer, 0, buffer.length, null)) > 0;) {
      hash.update(buffer.subarray(0, bytes));
      size += bytes;
    }
    const after = fs.fstatSync(fd);
    if (size !== opened.size || after.size !== opened.size || after.mtimeMs !== opened.mtimeMs || after.ctimeMs !== opened.ctimeMs) {
      throw new Error("snapshot_changed");
    }
    return { ...receipt, status: "persisted_bytes_observed", sha256: hash.digest("hex"), bytes: size };
  } catch (error) {
    return { ...receipt, reason: error.code || error.message };
  } finally {
    if (fd !== undefined) fs.closeSync(fd);
  }
}

module.exports = { graphSnapshotVersion };
