#![allow(unused)]
#![recursion_limit = "512"]

#[path = "../ui_bundle_support.rs"]
pub mod ui_bundle_support;

pub mod action_consumers;
pub mod action_routes;
pub mod audit_handlers;
// M1ND-10 G6 — bounded, offline verification of runtime authorization receipts.
// This is a one-shot binary mode and never boots an owner or opens a transport.
pub mod authorization_receipt_verifier;
// M1ND-10 G2 — owner-serialized authority state and fail-closed bootstrap.
// Platform protected-key/epoch assurance remains an explicit adapter boundary.
pub mod authority_runtime;
// M1ND-10 G2→G3 — strict authorize ingress and shared production coordinator.
pub mod authority_transport;
// M1ND-10 G3 — isolated append-only AuthorityWALV1 backend. It is deliberately
// not wired into HTTP, MissionService, key verification, or safety activation.
pub mod authority_wal;
// M1ND-10 G9 -> G1 — read-only projection from the protected autonomy owner.
// Supported/proven/active modes remain three separate facts.
pub mod autonomy_manifest;
// M1ND-10 G2→G3 — durable one-shot authorization leases. The broker is the
// named linearization seam between an owner authorization decision and an
// AuthorityWAL terminal commit; transport wiring remains separately gated.
pub mod owner_authorization_broker;
// M1ND-10 G2 production assembly — protected, anti-rollback owner trust roots.
// No private key or implicit software signer is representable in this schema.
pub mod auto_ingest;
pub mod cli;
pub mod daemon_handlers;
/// macOS Secure Enclave custody floor (amendment G9-A1). On non-macOS targets it
/// is absent by construction, so the production authority assembly stays
/// NOT_INSTALLED and fails closed — never a software-assurance fallback.
#[cfg(target_os = "macos")]
pub mod enclave_authority;
pub mod help_guidance;
pub mod owner_security_config;
pub mod protected_journal_head;
pub mod protocol;
pub mod server;
pub mod session;
// M1ND-10 G6 — graph-bound, dual-matrix temporal restart state.
pub mod temporal_state;
pub mod tools;
pub mod util;

// Human View v2 F11-a — the candidate_edit engine (typed batch edits on a candidate).
pub mod candidate_edit;
// Human View v2 F11-b — the naming-runner engine (packets, the o5 sanitizer, the
// /name client, and the scan/in-screen application paths).
pub mod naming_runner;
// Human View v2 F12 — the curation-runner engine (the propose-apply curation lane:
// the /curate client, the block-view packet, and the validate→apply→summary
// transaction with the RUNNER seat under OCC).
pub mod curation_runner;
// Perspective MCP — stateful navigation layer (12-PERSPECTIVE-SYNTHESIS)
pub mod boot_memory_handlers;
// M1ND-10 G6 — conservative one-way retirement of arbitrary Boot KV.
pub mod boot_kv_migration;
// Upgrade-path repair: one-time boot adoption of a pre-1.5 legacy graph snapshot.
pub mod legacy_snapshot_adoption;
// ORGANISM R6 — the delegation layer (`delegate` / `debrief`).
pub mod delegation_handlers;
pub mod engine_ops;
// M1ND-10 G3 — durable execution owner-outbox/runner-inbox lifecycle. The
// MissionService is the only component allowed to turn its typed reconciliation
// actions into mission letters; REST/MCP ingress reaches it through the facade.
pub mod execution_dispatch;
// M1ND-10 G2->G3 — typed non-mission elevated mutations.  The generic
// dispatcher remains ORDINARY-only; this pair owns the exact journal witness
// consumed by the authorization broker.
pub mod external_mutation_journal;
pub mod external_mutation_service;
// M1ND-10 G5 — hash-chained correlation projection across receipts, MissionService
// letters, delegation packets, and Mission Control records. Domain authorities
// remain separate; this module only validates bindings and serves a read model.
pub mod evidence_spine;
// Owner adapters: canonical-source auto-projection plus a genuinely read-only,
// brain/workspace-isolated EvidenceQuery seam.
pub mod evidence_spine_owner;
// Human-layer voice slice 1 — the `human_view` card composed into the north
// packet (m1nd-human-view-v0): the m1nd voice for the human, server-mounted.
pub mod cockpit;
pub mod human_view;
pub mod instance_registry;
pub mod layer_handlers;
pub mod light_author_handlers;
pub mod lock_handlers;
pub mod mailbox;
pub mod medulla_migration;
pub mod mission_handlers;
// HUMAN VIEW v2 F2.5a — the mission-letter contract + laws, its owner-runtime-local
// side record, and the `mission_post` verb handler.
pub mod mission_letter;
pub mod mission_letter_handlers;
pub mod mission_local;
// M1ND-10 G3 — owner-side mission state machine and transactionally fenced
// landing. Transport integration is intentionally separate.
pub mod mission_service;
#[cfg(test)]
pub(crate) mod mission_service_tests;
// M1ND-10 G3 — strict external facade. REST/MCP ingress injects authenticated
// authority and owner time; raw mission writes stay closed.
#[cfg(all(test, feature = "serve"))]
mod evidence_spine_wire_tests;
pub mod mission_service_transport;
#[cfg(test)]
mod mission_service_transport_tests;
#[cfg(all(test, feature = "serve"))]
mod mission_service_wire_tests;
// M1ND-10 G1 — owner-composed OrganismManifestV1. The manifest projects
// authorities; it never becomes an authority or a writable store itself.
pub mod organism_manifest;
pub mod persist_handlers;
pub mod perspective;
pub mod perspective_handlers;
// ORGANISM-INSIDE P1 — durable session-presence sidecars (m1nd-presence-v0): the
// control room sees the team, collisions derived at read (askGOD verdict 2026-07-13).
pub mod presence;
// HUMAN VIEW v2 F2.5c — the owner's runnerd surface (announce liveness registry +
// the mission_spawn proxy that keeps the shared secret owner-side).
pub mod runnerd_owner;
// M1ND-10 G4 — durable supervision for blocking work. The per-brain actor/OCC
// integration is live; individual transport ingresses still opt in explicitly.
pub mod runtime_jobs;
// M1ND-10 G4 — content-addressed brain checkpoints and the per-brain
// actor/worker/OCC boundary layered over RuntimeJobRegistry.
pub mod brain_runtime;
pub mod checkpoint_store;
#[cfg(windows)]
pub(crate) mod windows_durable_fs;
// MEDULLA M6 — the `promote` verb (project-private claim → medulla, audited).
pub mod promote_handlers;
// ORGANISM R16 — the SOUL (`soul_check` / `soul_read`): PATHOS parsed into
// anchored claims with verification states + the freshness receipt.
pub mod skeleton_scan;
pub mod soul_handlers;
pub mod surgical_handlers;
// The `transplant` verb (graph-addressed cross-file move of a top-level `fn`):
// resolve via the graph, trichotomy from `calls` edges, atomic write through the
// apply_batch machinery. Design + proof addresses: `docs/TRANSPLANT-PRD.md`.
pub mod system_blocks;
pub mod system_blocks_handlers;
pub mod transplant;

// v0.4.0: new tool handlers + personality
pub mod personality;
// Two-Tier Brain (interim): owner-hosted per-project brain stores + registry.
pub mod project_brains;
pub mod report_handlers;
pub mod result_shaping;
pub mod scope;
pub mod search_handlers;
pub mod trust_envelope;
pub mod ui_attestation;
pub mod universal_docs;
pub mod xray_handlers;

// These suites exercise crate-internal actor, routing, and owner seams. Keep
// them as unit-test modules so those implementation details do not need to be
// exported solely for integration tests.
#[cfg(test)]
#[path = "internal_tests/project_brain_runtime.rs"]
mod project_brain_runtime_internal_tests;

#[cfg(all(test, feature = "serve"))]
#[path = "internal_tests/delegation_slices.rs"]
mod delegation_slices_internal_tests;
#[cfg(all(test, feature = "serve"))]
#[path = "internal_tests/file_view_brain_root.rs"]
mod file_view_brain_root_internal_tests;
#[cfg(all(test, feature = "serve"))]
#[path = "internal_tests/hall_brains_listing.rs"]
mod hall_brains_listing_internal_tests;
#[cfg(all(test, feature = "serve"))]
#[path = "internal_tests/http_compression.rs"]
mod http_compression_internal_tests;
#[cfg(all(test, feature = "serve"))]
#[path = "internal_tests/mailbox_m7b.rs"]
mod mailbox_m7b_internal_tests;
#[cfg(all(test, feature = "serve"))]
#[path = "internal_tests/per_brain_open.rs"]
mod per_brain_open_internal_tests;
#[cfg(all(test, feature = "serve"))]
#[path = "internal_tests/universe_endpoint.rs"]
mod universe_endpoint_internal_tests;

#[cfg(test)]
#[path = "internal_tests/antibody_lifecycle_behavior.rs"]
mod antibody_lifecycle_behavior_internal_tests;
#[cfg(test)]
#[path = "internal_tests/counterfactual_behavior.rs"]
mod counterfactual_behavior_internal_tests;
#[cfg(test)]
#[path = "internal_tests/evidence_spine.rs"]
mod evidence_spine_internal_tests;
#[cfg(test)]
#[path = "internal_tests/lock_diff_ownership_and_not_found_errors.rs"]
mod lock_diff_ownership_and_not_found_errors_internal_tests;
#[cfg(test)]
#[path = "internal_tests/metrics_behavior.rs"]
mod metrics_behavior_internal_tests;
#[cfg(test)]
#[path = "internal_tests/perspective_inspect_stale_route_set_version_behavior.rs"]
mod perspective_inspect_stale_route_set_version_behavior_internal_tests;
#[cfg(test)]
#[path = "internal_tests/perspective_lifecycle_behavior.rs"]
mod perspective_lifecycle_behavior_internal_tests;
#[cfg(test)]
#[path = "internal_tests/perspective_suggest_affinity.rs"]
mod perspective_suggest_affinity_internal_tests;
#[cfg(test)]
#[path = "internal_tests/soul_check_behavior.rs"]
mod soul_check_behavior_internal_tests;
#[cfg(test)]
#[path = "internal_tests/taint_trace_behavior.rs"]
mod taint_trace_behavior_internal_tests;
#[cfg(test)]
#[path = "internal_tests/test_auto_ingest.rs"]
mod test_auto_ingest_internal_tests;
#[cfg(test)]
#[path = "internal_tests/test_edit_preview.rs"]
mod test_edit_preview_internal_tests;
#[cfg(test)]
#[path = "internal_tests/trail_save_list_resume_dispatch_behavior.rs"]
mod trail_save_list_resume_dispatch_behavior_internal_tests;
#[cfg(test)]
#[path = "internal_tests/trust_cold_start_no_lie.rs"]
mod trust_cold_start_no_lie_internal_tests;
#[cfg(test)]
#[path = "internal_tests/type_trace_behavior.rs"]
mod type_trace_behavior_internal_tests;
#[cfg(test)]
#[path = "internal_tests/why_closure_verdict_behavior.rs"]
mod why_closure_verdict_behavior_internal_tests;
#[cfg(test)]
#[path = "internal_tests/why_path_reconstruction_behavior.rs"]
mod why_path_reconstruction_behavior_internal_tests;

#[cfg(all(test, feature = "serve"))]
#[path = "internal_tests/medulla_m5b_tier_recall.rs"]
mod medulla_m5b_tier_recall_internal_tests;
#[cfg(all(test, feature = "serve"))]
#[path = "internal_tests/medulla_m6_promote.rs"]
mod medulla_m6_promote_internal_tests;
#[cfg(all(test, feature = "serve"))]
#[path = "internal_tests/retrieval_battery.rs"]
mod retrieval_battery_internal_tests;
#[cfg(all(test, feature = "serve"))]
#[path = "internal_tests/two_tier_project_brains.rs"]
mod two_tier_project_brains_internal_tests;
#[cfg(all(test, feature = "serve"))]
#[path = "internal_tests/ui_bundle_attestation.rs"]
mod ui_bundle_attestation_internal_tests;
// SPEC-1 — the freshness door's acceptance battery
// (`docs/GENESIS-INGEST-CONSUMERS-SPEC.md` §5), written before its implementation.
#[cfg(test)]
#[path = "internal_tests/spec1_refresh_declared_root.rs"]
mod spec1_refresh_declared_root_internal_tests;
// `--attach auto`'s SECOND discovery question ("is there a live owner that has
// INGESTED my repo?"), written before the second pass existed.
#[cfg(test)]
#[path = "internal_tests/attach_auto_ingest_coverage.rs"]
mod attach_auto_ingest_coverage_internal_tests;

// HTTP server + types (feature-gated behind "serve")
#[cfg(feature = "serve")]
pub mod http_security;
#[cfg(feature = "serve")]
pub mod http_server;
#[cfg(feature = "serve")]
pub mod http_types;
// Streamable-HTTP MCP transport (POST /mcp) — Wave 4, Slice 1.
#[cfg(feature = "serve")]
pub mod mcp_http;
// stdio↔HTTP bridge for `--attach` — Wave 4, Slice 3. Lets multiple stdio MCP
// hosts share one running `--serve` owner's live graph.
#[cfg(feature = "serve")]
pub mod attach_client;
