"use strict";

const path = require("path");
const { callToolSafely } = require("./mcp-runtime-client");

function receipt(files, reason) {
  return {
    status: "not_proven",
    scope: "explicit_files_only",
    atomic_snapshot: false,
    files: [...files],
    checked: 0,
    stale: [],
    unknown: [],
    ...(reason ? { reason } : {}),
  };
}

// The caller owns canonicalization and containment within the selected repo.
// This component performs no filesystem reads, scans, refreshes or retries.
// A boundary observation cannot prove that source remains unchanged afterward.
async function checkAnchorFreshness(client, agentId, files) {
  if (typeof agentId !== "string" || !agentId.trim() || agentId.includes("\0") ||
      !Array.isArray(files) || files.length < 1 || files.length > 8 ||
      files.some((file) => typeof file !== "string" || file.includes("\0") || !path.isAbsolute(file)) ||
      new Set(files).size !== files.length) {
    return receipt([], "invalid_input");
  }
  const requested = [...files];
  const expected = new Set(requested);
  // Hidden registry verbs remain callable; tools/list is not an availability
  // gate. Errors (including unsupported tools) cannot become freshness proof.
  const response = await callToolSafely(client, "am_i_stale", { agent_id: agentId, files: [...requested] });
  if (response.isError) return receipt(requested, "tool_error");
  const payload = response.payload;
  if (!payload || typeof payload !== "object" || Array.isArray(payload) ||
      !Number.isInteger(payload.checked) || payload.checked < 0 || payload.checked > requested.length ||
      !Array.isArray(payload.fresh) || !Array.isArray(payload.stale) || !Array.isArray(payload.unknown)) {
    return receipt(requested, "malformed_response");
  }

  const seen = new Set();
  const accept = (file) => {
    if (typeof file !== "string" || !expected.has(file) || seen.has(file)) return false;
    seen.add(file);
    return true;
  };
  for (const file of payload.fresh) {
    if (!accept(file)) return receipt(requested, "malformed_response");
  }
  const stale = [];
  for (const item of payload.stale) {
    if (!item || typeof item !== "object" || Array.isArray(item) ||
        (item.reason !== "changed" && item.reason !== "missing") || !accept(item.path)) {
      return receipt(requested, "malformed_response");
    }
    stale.push({ path: item.path, reason: item.reason });
  }
  const unknown = [];
  for (const file of payload.unknown) {
    if (!accept(file)) return receipt(requested, "malformed_response");
    unknown.push(file);
  }
  if (seen.size !== payload.checked) return receipt(requested, "malformed_response");

  const result = { ...receipt(requested), checked: payload.checked, stale, unknown };
  if (stale.length > 0) return { ...result, status: "stale" };
  if (unknown.length > 0) return { ...result, reason: "unknown_files" };
  if (payload.checked !== requested.length || payload.fresh.length !== requested.length) {
    return { ...result, reason: "incomplete_check" };
  }
  return { ...result, status: "fresh_at_boundary" };
}

module.exports = { checkAnchorFreshness };
