"use strict";

const path = require("path");

// Select only the bounded files actually returned by the context protocol.
// Canonicalization/containment belongs to the caller; no scanning or hashing.
function capsuleFiles(anchor, payload, ensureWithinRepo) {
  const invalid = () => ({ ok: false, scope: "not_proven", files: [], reason: "invalid_capsule_scope" });
  const validPath = value => typeof value === "string" && value.trim().length > 0 && !value.includes("\0");
  if (!validPath(anchor) || !payload || typeof payload !== "object" || Array.isArray(payload) || typeof ensureWithinRepo !== "function") return invalid();
  const present = Object.prototype.hasOwnProperty.call(payload, "connected_files");
  if (present && (!Array.isArray(payload.connected_files) || payload.connected_files.length > 5)) return invalid();
  try {
    const files = [ensureWithinRepo(anchor)];
    if (Object.prototype.hasOwnProperty.call(payload, "file_path") &&
        (!validPath(payload.file_path) || ensureWithinRepo(payload.file_path) !== files[0])) return invalid();
    if (Object.prototype.hasOwnProperty.call(payload, "primary_file")) {
      const primary = payload.primary_file;
      if (!primary || typeof primary !== "object" || Array.isArray(primary) || !validPath(primary.file_path) ||
          ensureWithinRepo(primary.file_path) !== files[0]) return invalid();
    }
    for (const item of present ? payload.connected_files : []) {
      if (!item || typeof item !== "object" || Array.isArray(item) || !validPath(item.file_path)) return invalid();
      files.push(ensureWithinRepo(item.file_path));
    }
    if (files.some(file => !validPath(file) || !path.isAbsolute(file))) return invalid();
    return { ok: true, scope: present ? "anchor_and_returned_connected_files" : "selected_anchor_only", files: [...new Set(files)] };
  } catch (_) { return invalid(); }
}

module.exports = { capsuleFiles };
