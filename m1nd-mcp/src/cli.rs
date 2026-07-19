// === m1nd-mcp CLI argument parsing ===
//
// Clap derive struct for m1nd-mcp binary modes.
// Replaces manual std::env::args() parsing in main.rs.

use clap::Parser;

/// `--version` string: semantic version + embedded git sha, e.g. `1.1.0 (50385cd)`.
/// The sha is `unknown` on builds without a `.git` (crates.io / vendored). This
/// makes the binary declare exactly what it is — the first layer of the
/// version-honesty moat (see `build.rs` / `session::binary_version_info`).
const LONG_VERSION: &str = concat!(env!("CARGO_PKG_VERSION"), " (", env!("M1ND_GIT_SHA"), ")");

#[derive(Parser, Debug)]
#[command(
    name = "m1nd-mcp",
    about = "Neuro-symbolic connectome engine",
    version,
    long_version = LONG_VERSION
)]
pub struct Cli {
    /// Read one bounded authorization-receipt verification request from stdin,
    /// emit one closed proof JSON object, and exit without booting an owner,
    /// opening a port, or touching runtime/home state.
    #[arg(long, exclusive = true)]
    pub verify_authorization_receipt: bool,

    /// Start HTTP server with embedded web UI
    #[arg(long)]
    pub serve: bool,

    /// HTTP server port
    #[arg(long, default_value = "1337")]
    pub port: u16,

    /// Bind address override (default: 127.0.0.1). Every non-loopback bind (for
    /// example 0.0.0.0 or a concrete LAN IP) is refused because authenticated
    /// TLS remote transport and scoped authorization are not implemented.
    #[arg(long, default_value = "127.0.0.1")]
    pub bind: String,

    /// Legacy compatibility flag. It cannot override the fail-closed refusal of
    /// non-loopback binds; retained so older launch commands fail honestly at the
    /// network gate instead of at argument parsing.
    #[arg(long)]
    pub allow_remote: bool,

    /// Serve frontend from disk instead of embedded (dev mode)
    #[arg(long)]
    pub dev: bool,

    /// Serve this UI directory verbatim instead of the bundled UI. Its runtime
    /// tree digest is attested on every manifest read; drift never reuses the
    /// binary's build-time digest.
    #[arg(long, value_name = "PATH")]
    pub ui_dir: Option<String>,

    /// Also run JSON-RPC stdio server alongside HTTP
    #[arg(long)]
    pub stdio: bool,

    /// Auto-open browser on startup
    #[arg(long)]
    pub open: bool,

    /// Path to config JSON file
    #[arg(long)]
    pub config: Option<String>,

    /// Graph source path override
    #[arg(long)]
    pub graph: Option<String>,

    /// Plasticity state path override
    #[arg(long)]
    pub plasticity: Option<String>,

    /// Runtime directory override for instance sidecar state
    #[arg(long)]
    pub runtime_dir: Option<String>,

    /// Global registry directory override
    #[arg(long)]
    pub registry_dir: Option<String>,

    /// Domain: code, music, memory, generic
    #[arg(long, default_value = "code")]
    pub domain: String,

    /// Disable auto-launching the HTTP GUI in stdio mode (for CI, headless servers)
    #[arg(long)]
    pub no_gui: bool,

    /// Path to event log file (append-only JSON lines). Enables cross-process SSE via file bus.
    #[arg(long)]
    pub event_log: Option<String>,

    /// Watch an event log file and broadcast new events via SSE (HTTP-only mode).
    /// Use when a separate stdio process writes events to this file.
    #[arg(long)]
    pub watch_events: Option<String>,

    /// Attach read-only: load the snapshot and serve queries, but never write to
    /// disk and never take an exclusive lease. Mutation tools are disabled.
    /// Also honored via env `M1ND_READ_ONLY=1`.
    #[arg(long)]
    pub read_only: bool,

    /// Attach to a running `--serve` owner as a thin stdio↔HTTP MCP bridge.
    /// Takes the owner's base URL (e.g. `http://127.0.0.1:1337`), or the literal
    /// `auto` to auto-discover the live serve ReadWrite owner for this client's
    /// runtime_root via the instance registry (read-only, NO lease). The env var
    /// `M1ND_ATTACH_URL`, when set, overrides both and wins. The bridge loads NO
    /// graph, builds NO engines, and takes NO lease: it speaks stdio MCP to the
    /// host (Claude Code), forwards every JSON-RPC frame to the owner's
    /// `POST /mcp`, and relays the owner's server→client SSE push notifications
    /// (`notifications/m1nd/graph_changed`) back to stdout. Multiple `--attach`
    /// clients pointed at one owner share that owner's single live graph.
    /// Requires the `serve` feature.
    #[arg(long)]
    pub attach: Option<String>,

    /// One-shot triage: distribute the field-report spool into per-project boxes
    /// (`<repo>/.m1nd/inbox.jsonl`) + the medulla box, then print the cross-box
    /// sweep (spool ∪ every known box, de-duplicated by content id) as JSON and
    /// exit. Idempotent (append-with-dedup), LOCAL, safe to re-run — it is
    /// telemetry, not memory (MEDULLA-PRD §9.2). Add `--no-distribute` to sweep
    /// the EXISTING boxes without filing anything new first.
    #[arg(long)]
    pub inbox_sweep: bool,

    /// With `--inbox-sweep`: skip the distribution pass and only read the current
    /// spool + boxes (a pure, read-only view).
    #[arg(long)]
    pub no_distribute: bool,

    /// One-shot MEDULLA storage-split migration (MEDULLA-PRD §4.2, slice M5a).
    /// Takes one required verb (no default):
    ///   `plan`     — print the dry-run plan JSON (enumerate + classify + the
    ///                count-conservation gate) WITHOUT mutating anything;
    ///   `apply`    — backup-first, then move repo-fact claims into the project
    ///                brain store, stamp `Origin-Brain`, prune ghost ingest-root
    ///                pointers, and verify count- AND content-conservation; prints
    ///                the receipt (incl. the authoritative `moved_files` list). It
    ///                REFUSES on any destination name collision (never overwrites)
    ///                and writes a `manifest.json` + an `ingest_roots.json` copy
    ///                into the backup dir;
    ///   `rollback` — restore the medulla store (and `ingest_roots.json`) from the
    ///                most recent backup, removing exactly the files named in that
    ///                backup's manifest (never scanning the destination store), and
    ///                snapshotting the live state first so a mid-restore failure
    ///                stays recoverable.
    /// Derives every path from the runtime root exactly like `--inbox-sweep`,
    /// runs offline, prints JSON, and exits. `apply`/`rollback` mutate the store —
    /// intended for the maintainer, never an agent (the CODE-LAND-ONLY posture) —
    /// and REFUSE while a served owner is up (stop the owner first: the offline
    /// migration must not race a live owner).
    #[arg(long, value_name = "plan|apply|rollback")]
    pub medulla_migrate: Option<MedullaMigrateMode>,

    /// The destination project brain for `--medulla-migrate`, named EXPLICITLY
    /// as a repo root path — the brain that repo-fact claims move into and whose
    /// root is stamped as their `Origin-Brain`. It is NEVER derived from the
    /// ambient session binding: a second agent that bound the owner to an
    /// unrelated repo once caused the migration to move legacy memories into the
    /// wrong brain's store (field bug 2026-07-05). REQUIRED for `apply` (and for
    /// `rollback`, which must locate the same store); recommended for `plan`,
    /// which otherwise falls back to the ambient binding and loudly flags that
    /// destination as unsafe in its JSON.
    #[arg(long, value_name = "PATH")]
    pub migrate_project_root: Option<String>,
}

/// The verb for `--medulla-migrate` (MEDULLA-PRD §4.2). No default: the flag
/// requires one of these values, so a bare `--medulla-migrate` is a usage error.
#[derive(clap::ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[value(rename_all = "lower")]
pub enum MedullaMigrateMode {
    /// Pure dry-run: print the plan, mutate nothing (§11 M5a default).
    Plan,
    /// The gated executor: backup-first split + stamp + prune (mutates).
    Apply,
    /// Restore the medulla store from the most recent backup (mutates).
    Rollback,
}
