#![allow(unused)]
#![recursion_limit = "512"]

pub mod audit_handlers;
pub mod auto_ingest;
pub mod cli;
pub mod daemon_handlers;
pub mod help_guidance;
pub mod protocol;
pub mod server;
pub mod session;
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
// ORGANISM R6 — the delegation layer (`delegate` / `debrief`).
pub mod delegation_handlers;
pub mod engine_ops;
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
pub mod persist_handlers;
pub mod perspective;
pub mod perspective_handlers;
// HUMAN VIEW v2 F2.5c — the owner's runnerd surface (announce liveness registry +
// the mission_spawn proxy that keeps the shared secret owner-side).
pub mod runnerd_owner;
// MEDULLA M6 — the `promote` verb (project-private claim → medulla, audited).
pub mod promote_handlers;
// ORGANISM R16 — the SOUL (`soul_check` / `soul_read`): PATHOS parsed into
// anchored claims with verification states + the freshness receipt.
pub mod skeleton_scan;
pub mod soul_handlers;
pub mod surgical_handlers;
pub mod system_blocks;
pub mod system_blocks_handlers;

// v0.4.0: new tool handlers + personality
pub mod personality;
// Two-Tier Brain (interim): owner-hosted per-project brain stores + registry.
pub mod project_brains;
pub mod report_handlers;
pub mod result_shaping;
pub mod scope;
pub mod search_handlers;
pub mod trust_envelope;
pub mod universal_docs;
pub mod xray_handlers;

// HTTP server + types (feature-gated behind "serve")
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
