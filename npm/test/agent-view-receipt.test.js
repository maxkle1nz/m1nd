"use strict";

const assert = require("node:assert/strict");
const crypto = require("node:crypto");
const path = require("node:path");
const test = require("node:test");
const { agentViewReceipt } = require("../lib/agent-view-receipt");

const root = path.parse(__filename).root;
function target(repo = "repo-alpha", branch = "main", head = "a".repeat(40)) {
  const identity = { schema: "m1nd-agent-runtime-cache-v1", canonical_root: path.join(root, repo),
    source_revision: { kind: "git", head, branch } };
  const key = crypto.createHash("sha256").update(JSON.stringify(identity)).digest("hex");
  return { runtimeDir: path.join(root, "cache", key), identity };
}
function graphVersion() {
  return { status: "persisted_bytes_observed", update_status: "refreshed",
    code_snapshot: { schema: "m1nd-refresh-code-snapshot-v1", source_projection_digest: "b".repeat(64),
      semantic_completeness: "not_proven", details: { inputs: ["source"] } } };
}

test("view receipt keeps the existing cache key and separates roots, branches and commits", () => {
  const inputs = [target(), target("repo-alpha", "feature"), target("repo-beta"), target("repo-alpha", "main", "c".repeat(40))];
  const views = inputs.map((input) => agentViewReceipt(input, graphVersion()));
  assert.equal(new Set(views.map((view) => view.view_id)).size, 4);
  for (const [index, view] of views.entries()) {
    assert.equal(view.schema, "m1nd-agent-view-v1");
    assert.equal(view.status, "selected");
    assert.equal(view.view_id, path.basename(inputs[index].runtimeDir));
    assert.equal(view.canonical_root, inputs[index].identity.canonical_root);
    assert.deepEqual(view.source_revision, inputs[index].identity.source_revision);
    assert.equal(view.memory.scope, "cache_view");
    assert.equal(view.memory.view_id, view.view_id);
    assert.equal(view.memory.cross_view_sharing, false);
    assert.equal(view.memory.claim_origin, "not_attested");
    assert.equal(view.memory.claims_freshness, "not_revalidated");
  }
});

test("manual, missing and malformed cache targets do not assert branch or memory scope", () => {
  const malformed = target();
  malformed.runtimeDir = path.join(root, "cache", "not-a-cache-key");
  const unknownSchema = target();
  unknownSchema.identity.schema = "unknown";
  for (const input of [null, undefined, { runtimeDir: path.join(root, "manual-runtime") }, malformed, unknownSchema]) {
    const view = agentViewReceipt(input, graphVersion());
    assert.equal(view.status, "not_checked");
    assert.equal(view.view_id, null);
    assert.equal(view.canonical_root, null);
    assert.equal(view.source_revision, null);
    assert.equal(view.memory.scope, "not_checked");
    assert.equal(view.memory.view_id, null);
    assert.equal(view.memory.cross_view_sharing, null);
    assert.equal(view.code_version.status, "not_observed");
  }
});

test("code provenance is observed only from a successfully refreshed persisted graph receipt", () => {
  const graph = graphVersion();
  assert.deepEqual(agentViewReceipt(target(), graph).code_version,
    { status: "observed_refreshed", provenance: graph.code_snapshot });
  for (const version of [null, {}, { ...graph, status: "unavailable" },
    { ...graph, update_status: "refused" }, { ...graph, update_status: "not_observed" },
    { ...graph, code_snapshot: null }, { ...graph, code_snapshot: {} }]) {
    assert.deepEqual(agentViewReceipt(target(), version).code_version,
      { status: "not_observed", provenance: null });
  }
});

test("returned provenance does not alias target identity or source receipts", () => {
  const input = target(), graph = graphVersion();
  const view = agentViewReceipt(input, graph);
  view.source_revision.branch = "changed";
  view.code_version.provenance.details.inputs.push("changed");
  view.memory.view_id = "changed";
  assert.equal(input.identity.source_revision.branch, "main");
  assert.deepEqual(graph.code_snapshot.details.inputs, ["source"]);
  assert.equal(agentViewReceipt(input, graph).memory.view_id, path.basename(input.runtimeDir));
  assert.equal(view.content_certified, false);
});

test("observed memory telemetry never certifies claim origin or current truth", () => {
  const refresh = { attempted: true, ok: true, agent_memory_restored: { loaded: true,
    loaded_count: 2, file_count: 4, load_cap: 2, capped_out_count: 2,
    light_evidence_resolved: 3, light_evidence_unresolved: 1, stale_evidence_count: 1,
    dir: "not-forwarded", capped_out: ["not-forwarded"], stale_claims: ["not-forwarded"],
    freshness_note: "not-forwarded", error: "not-forwarded" } };
  const memory = agentViewReceipt(target(), graphVersion(), refresh).memory;
  assert.deepEqual(memory.restoration, { status: "reported", loaded: true, loaded_count: 2,
    file_count: 4, load_cap: 2, capped_out_count: 2, light_evidence_resolved: 3,
    light_evidence_unresolved: 1, stale_evidence_count: 1 });
  assert.equal(memory.claim_origin, "not_attested");
  assert.equal(memory.claims_freshness, "not_revalidated");
  assert.equal(memory.cross_view_sharing, false);
});

test("missing or refused memory telemetry never invents loaded claims", () => {
  for (const refresh of [null, {}, { attempted: true, ok: false, agent_memory_restored: { loaded: true, loaded_count: 4 } },
    { attempted: true, ok: true, agent_memory_restored: { loaded: true } },
    { attempted: true, ok: true, agent_memory_restored: { loaded: true, loaded_count: -1 } }]) {
    assert.deepEqual(agentViewReceipt(target(), graphVersion(), refresh).memory.restoration, { status: "not_observed" });
  }
  const refresh = { attempted: true, ok: true, agent_memory_restored: { loaded: false,
    file_count: 0, loaded_count: -1, capped_out_count: Infinity, light_evidence_resolved: 1.5, load_cap: null } };
  assert.deepEqual(agentViewReceipt(target(), null, refresh).memory.restoration,
    { status: "reported", loaded: false, file_count: 0, load_cap: null });
  assert.deepEqual(agentViewReceipt(null, graphVersion(), refresh).memory.restoration, { status: "not_observed" });
});
