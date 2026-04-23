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

// Perspective MCP — stateful navigation layer (12-PERSPECTIVE-SYNTHESIS)
pub mod boot_memory_handlers;
pub mod engine_ops;
pub mod instance_registry;
pub mod layer_handlers;
pub mod lock_handlers;
pub mod persist_handlers;
pub mod perspective;
pub mod perspective_handlers;
pub mod surgical_handlers;

// v0.4.0: new tool handlers + personality
pub mod personality;
pub mod report_handlers;
pub mod result_shaping;
pub mod scope;
pub mod search_handlers;
pub mod universal_docs;

// HTTP server + types (feature-gated behind "serve")
#[cfg(feature = "serve")]
pub mod http_server;
#[cfg(feature = "serve")]
pub mod http_types;
