"use strict";

const path = require("path");

function cloneRecord(value) {
  if (!value || typeof value !== "object" || Array.isArray(value)) return null;
  try {
    const copy = JSON.parse(JSON.stringify(value));
    return copy && typeof copy === "object" && !Array.isArray(copy) ? copy : null;
  } catch (_) { return null; }
}

function validRevision(revision) {
  if (!revision) return false;
  if (revision.kind === "non_git") return revision.head === null && revision.branch === null;
  if (typeof revision.branch !== "string" || !revision.branch) return false;
  if (revision.kind === "git_unborn") return revision.head === null;
  return revision.kind === "git" && typeof revision.head === "string"
    && /^(?:[a-f0-9]{40}|[a-f0-9]{64})$/i.test(revision.head);
}

function memoryRestoration(refresh) {
  const missing = { status: "not_observed" };
  if (refresh?.attempted !== true || refresh.ok !== true) return missing;
  const report = refresh.agent_memory_restored;
  if (!report || typeof report.loaded !== "boolean") return missing;
  const count = (value) => Number.isSafeInteger(value) && value >= 0;
  if (report.loaded && !count(report.loaded_count)) return missing;
  const observed = { status: "reported", loaded: report.loaded };
  for (const key of ["file_count", "loaded_count", "capped_out_count",
    "light_evidence_resolved", "light_evidence_unresolved", "stale_evidence_count"]) {
    if (count(report[key])) observed[key] = report[key];
  }
  if (report.load_cap === null || count(report.load_cap)) observed.load_cap = report.load_cap;
  return observed;
}

// Describes the supplied automatic selector and observed telemetry only. No
// filesystem inspection, dirty-state fingerprinting or claim certification.
// Explicit/manual launchers must pass null rather than invent a cache target.
function agentViewReceipt(cacheTarget, graphVersion, refreshReceipt) {
  const unknownCode = () => ({ status: "not_observed", provenance: null });
  const output = {
    schema: "m1nd-agent-view-v1",
    status: "not_checked",
    view_id: null,
    canonical_root: null,
    source_revision: null,
    content_certified: false,
    code_version: unknownCode(),
    memory: { scope: "not_checked", view_id: null, cross_view_sharing: null,
      claim_origin: "not_attested", claims_freshness: "not_revalidated",
      restoration: { status: "not_observed" } },
  };
  const identity = cacheTarget?.identity;
  const revision = cloneRecord(identity?.source_revision);
  if (typeof cacheTarget?.runtimeDir !== "string" || !path.isAbsolute(cacheTarget.runtimeDir)
    || identity?.schema !== "m1nd-agent-runtime-cache-v1"
    || typeof identity.canonical_root !== "string" || !path.isAbsolute(identity.canonical_root)
    || !validRevision(revision)) return output;
  const key = path.basename(cacheTarget.runtimeDir);
  if (!/^[a-f0-9]{64}$/i.test(key)) return output;
  output.status = "selected";
  output.view_id = key;
  output.canonical_root = identity.canonical_root;
  output.source_revision = revision;
  output.memory.scope = "cache_view";
  output.memory.view_id = key;
  output.memory.cross_view_sharing = false;
  output.memory.restoration = memoryRestoration(refreshReceipt);
  if (graphVersion?.status === "persisted_bytes_observed" && graphVersion.update_status === "refreshed") {
    const provenance = cloneRecord(graphVersion.code_snapshot);
    if (provenance && Object.keys(provenance).length) {
      output.code_version = { status: "observed_refreshed", provenance };
    }
  }
  return output;
}

module.exports = { agentViewReceipt };
