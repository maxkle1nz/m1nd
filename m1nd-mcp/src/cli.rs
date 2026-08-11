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

/// The loopback port a served owner listens on when the operator names none —
/// the port the PRODUCT hands the world (`--serve` with no `--port`). It is the
/// SINGLE source of this default: `--port`'s clap default is derived from it, and
/// so is the offline migration's owner-alive guard
/// (`medulla_migration::MedullaMigration`). Keeping one constant is the fix for a
/// real miscalibration (2026-07-24): the guard used to probe only `1338` — the
/// port this maintainer's launchd owner happens to sit on — so a user serving on
/// the default port passed the guard unseen and the offline migration mutated the
/// stores under a live owner.
pub const DEFAULT_SERVED_OWNER_PORT: u16 = 1337;

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

    /// THE CUSTODY CEREMONY (amendment G9-A1, Path B —
    /// `docs/benchmarks/G9-CUSTODY-CEREMONY.md`). Run ONE step of the Secure
    /// Enclave custody ceremony, print the result as JSON, and exit. Offline and
    /// one-shot, exactly like `--verify-authorization-receipt`, `--inbox-sweep`
    /// and `--medulla-migrate`: it never boots an owner, opens a port, or takes a
    /// lease.
    ///
    /// THIS FLAG IS THE STAMP. Admission to a custody ceremony is a fact the OWNER
    /// observes about ITSELF, and this ingress is that fact: the human ran this
    /// command. It is the only place in the binary that constructs a
    /// `custody_ceremony::OwnerCeremonyIngressV1`, so no MCP or REST payload — no
    /// header, no field, no claim of any kind — can ever produce one.
    ///
    /// Verbs, in the order the owner runs them:
    ///   `preflight`       — report every prerequisite; provisions NOTHING. The
    ///                       only verb an agent may run.
    ///   `provision-seats` — Phase A, the four unattended verifier seats.
    ///   `owner-seat`      — Phase B, the owner's biometric seat. Refuses when no
    ///                       human is attached; Touch ID has no stand-in.
    ///   `seal`            — Phase C, seal the ceremony receipt.
    ///   `assemble`        — assemble the production owner authority from the
    ///                       sealed ceremony and print the pinned authority
    ///                       manifest the G6 formal run requires.
    ///
    /// No agent may perform, simulate or dry-run any step but `preflight`
    /// (`G9-CUSTODY-CEREMONY.md` §0).
    #[arg(long, value_name = "VERB")]
    pub custody_ceremony: Option<String>,

    /// The owner's `0700`, non-symlink protected root holding the enclave-sealed
    /// slots (prerequisite P5). Owner-held: this binary never derives or creates it.
    #[arg(long, value_name = "PATH")]
    pub custody_protected_root: Option<String>,

    /// Owner-held path to the immutable owner security config the production
    /// authority assembly loads through the enclave-sealed root.
    #[arg(long, value_name = "PATH")]
    pub custody_owner_security_config: Option<String>,

    /// Owner-held path to the MissionService config the production authority
    /// assembly binds.
    #[arg(long, value_name = "PATH")]
    pub custody_mission_config: Option<String>,

    /// Owner-held path to the JSON `IndependenceSpecV1` whose four voting seats
    /// the ceremony provisions (`provision-seats`) and seals (`seal`). Required
    /// by both: the seats belong to the owner's constitution, so this binary
    /// reads their principals, key ids and failure domains rather than inventing
    /// them — a ceremony that made up its own seats would be binding to itself.
    #[arg(long, value_name = "PATH")]
    pub custody_independence_spec: Option<String>,

    /// The owner's constitution digest (lowercase sha-256 hex), sealed into the
    /// ceremony receipt by `seal`. Recorded, never computed: the ceremony states
    /// which constitution its seats were minted under, it does not decide it.
    #[arg(long, value_name = "SHA256")]
    pub custody_constitution_digest: Option<String>,

    /// Seal a hand-authored `IndependenceSpecV1` (JSON): fill its
    /// `independence_spec_digest` from the digest of its own core, print the sealed
    /// document to stdout, and exit. Offline and one-shot, exactly like
    /// `--verify-authorization-receipt`, `--inbox-sweep` and `--custody-ceremony`:
    /// it never boots an owner, opens a port, or takes a lease.
    ///
    /// This is a DOCUMENT step, not a ceremony step. It reads one file and computes
    /// one digest — it never opens the Secure Enclave, the keychain, the protected
    /// root, or any ceremony state, and it runs on every platform. It is what makes
    /// the ceremony's prerequisite P9 reachable by hand: the owner writes the four
    /// voting seats himself, and every custody verb refuses a spec whose declared
    /// digest is not the digest of its core.
    ///
    /// The incoming digest is READ AND IGNORED — empty, placeholder or stale from
    /// an earlier draft, all three are overwritten — because sealing is the act
    /// that decides it. Everything else is carried through untouched.
    ///
    /// It refuses, by name and with exit 1, on an unreadable file, on JSON that is
    /// not this contract (an unknown field included), on the wrong schema, on a seat
    /// count other than the frozen four, on a quorum outside the kernel floor, on a
    /// lowered or unmet failure-domain minimum, and on either non-voting role marked
    /// voting. Every one of those floors is read from the constants in
    /// `m1nd-control`'s `autonomy` module, so this surface cannot drift from the
    /// kernel the ceremony checks the spec against.
    ///
    /// What it does NOT check is what needs the kernel, the constitution or
    /// cryptography — seat identity uniqueness, the sentinel's exclusion from the
    /// voting seats, the blind-isolation policy digest. `validate_against_kernel`
    /// owns those and still runs at the ceremony. A sealed spec is well-formed
    /// enough to present; it is not thereby ratified.
    ///
    /// The output is the document, so it pipes straight into the ceremony:
    ///   m1nd-mcp --seal-independence-spec draft.json > independence-spec.json
    #[arg(long, value_name = "PATH", exclusive = true)]
    pub seal_independence_spec: Option<String>,

    /// Start HTTP server with embedded web UI
    #[arg(long)]
    pub serve: bool,

    /// HTTP server port
    // The default is NOT a literal here: it comes from `DEFAULT_SERVED_OWNER_PORT`,
    // the one constant the owner-alive guard also reads, so the port the product
    // serves on and the port the guard watches for can never drift apart. (`--help`
    // still shows the concrete `[default: 1337]`, rendered by clap.)
    #[arg(long, default_value_t = DEFAULT_SERVED_OWNER_PORT)]
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
    /// `auto` to auto-discover a live serve owner via the instance registry
    /// (read-only, NO lease). `auto` asks TWO questions, in order: first "is
    /// there a live serve ReadWrite owner for this client's runtime_root?", and
    /// only failing that "is there a live serve owner whose declared ingest roots
    /// COVER this caller's repo?" — so an agent working inside a repo a served
    /// owner has already ingested reaches that owner instead of an empty local
    /// runtime. The second question resolves a git worktree to its main
    /// repository, REFUSES (naming every candidate) when two owners cover the
    /// same repo, and reads the bearer token from THAT owner's runtime root. The
    /// env var `M1ND_ATTACH_URL`, when set, overrides both and wins. The bridge
    /// loads NO
    /// graph, builds NO engines, and takes NO lease: it speaks stdio MCP to the
    /// host (Claude Code), forwards every JSON-RPC frame to the owner's
    /// `POST /mcp`, and relays the owner's server→client SSE push notifications
    /// (`notifications/m1nd/graph_changed`) back to stdout. Multiple `--attach`
    /// clients pointed at one owner share that owner's single live graph.
    /// Requires the `serve` feature.
    #[arg(long)]
    pub attach: Option<String>,

    /// Ask `--attach auto`'s TWO discovery questions, print the answer as one
    /// JSON object, and exit — without attaching, loading a graph, taking a
    /// lease or opening a port. Exit code 0 when an owner answered, 1 when none
    /// did (the refusal travels verbatim in the payload's `reason`).
    ///
    /// It exists so a client that is NOT Rust can make the same boot decision
    /// the bridge makes. The npm agent CLI (`m1nd agent first-minute`,
    /// `m1nd agent context`) used to boot an isolated runtime unconditionally
    /// and report `needs_authority` on a machine where a served owner already
    /// held the caller's repo; it now asks this probe first. Answering here
    /// instead of re-implementing the questions is the whole point: there is
    /// exactly ONE discovery (`instance_registry::discover_serve_owner`), and
    /// this is a projection of it. Requires the `serve` feature.
    #[arg(long)]
    pub discover_owner: bool,

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

    /// THE BIRTH CEREMONY (GENESIS-INGEST-CONSUMERS-SPEC.md §2, owner-ratified
    /// 2026-07-29). Birth a project brain for the named repo root, print the
    /// certificate as JSON, and exit. Offline and one-shot, exactly like
    /// `--inbox-sweep` and `--medulla-migrate`.
    ///
    /// THIS FLAG IS THE STAMP. Admission to `brain.bootstrap.birth` is an origin
    /// the OWNER applies from a fact it observes about ITSELF, and this ingress
    /// is that fact: the human ran this command. It is the only place in the
    /// binary that constructs a `brain_birth::HumanOrigin`, so no MCP or REST
    /// payload — no header, no `birth_via` field, no claim of any kind — can ever
    /// produce one. Over the wire the verb is refused for every client.
    ///
    /// The human-facing form is `m1nd init --birth <repo>`; the npm CLI runs this
    /// binary with this flag. An AGENT that finds a repo with no brain OFFERS
    /// that command and stops — running it is not the agent's to do.
    ///
    /// Refuses unless the destination is EMPTY on disk, unless the root resolves,
    /// on any overlap with an existing brain, and on the owner's own bound root.
    /// It is not the way to adopt an existing brain: that is migration, a
    /// boot-time fact with no verb.
    #[arg(long, value_name = "REPO")]
    pub birth: Option<String>,

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
