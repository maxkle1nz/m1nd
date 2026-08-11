// === m1nd-mcp binary entry point ===
//
// Modes:
//   m1nd-mcp                     → JSON-RPC stdio only (default runtime path)
//   m1nd-mcp --no-gui            → JSON-RPC stdio only (explicit CI/headless intent)
//   m1nd-mcp --serve             → HTTP server + embedded UI on :1337
//   m1nd-mcp --serve --stdio     → Both transports simultaneously (SSE cross-process bridge)
//   m1nd-mcp --serve --dev       → HTTP with frontend served from disk (Vite HMR)
//   m1nd-mcp --serve --open      → HTTP + auto-open browser
//
// Cross-process SSE (new):
//   m1nd-mcp --serve --stdio                           → Option A: same process, shared state + broadcast
//   m1nd-mcp --serve --stdio --event-log /tmp/e.jsonl  → Option A + B: same process + file event bus
//   m1nd-mcp --serve --watch-events /tmp/e.jsonl       → Option B consumer: watch file, broadcast SSE

use clap::Parser;
use m1nd_mcp::cli::Cli;
use m1nd_mcp::server::{McpConfig, McpServer};
use std::path::PathBuf;

#[cfg(unix)]
fn ensure_bwrap_compat_wrapper() {
    if let Ok(home) = std::env::var("HOME") {
        let bwrap_path = std::path::PathBuf::from(home).join(".local/bin/bwrap");
        if !bwrap_path.exists() {
            let wrapper = r#"#!/bin/bash
args=()
skip_next=0
for arg in "$@"; do
    if [ "$skip_next" -eq 1 ]; then
        skip_next=0
        continue
    fi
    if [ "$arg" = "--argv0" ]; then
        skip_next=1
        continue
    fi
    args+=("$arg")
done
exec /usr/bin/bwrap "${args[@]}"
"#;
            if let Some(parent) = bwrap_path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            if std::fs::write(&bwrap_path, wrapper).is_ok() {
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    if let Ok(mut perms) = std::fs::metadata(&bwrap_path).map(|m| m.permissions()) {
                        perms.set_mode(0o755);
                        let _ = std::fs::set_permissions(&bwrap_path, perms);
                    }
                }
            }
        }
    }
}

/// Resolve the runtime_root `--attach auto`'s FIRST discovery question asks about.
///
/// Computed with the SAME rule the owner uses (`session.rs` runtime-root
/// default): explicit `--runtime-dir` if given, else the parent directory of
/// `--graph` if given, else the current working dir.
#[cfg(feature = "serve")]
fn resolve_attach_runtime_root(cli: &Cli) -> Result<PathBuf, String> {
    if let Some(dir) = &cli.runtime_dir {
        return Ok(PathBuf::from(dir));
    }
    if let Ok(dir) = std::env::var("M1ND_RUNTIME_DIR") {
        if !dir.trim().is_empty() {
            return Ok(PathBuf::from(dir));
        }
    }
    if let Some(config_path) = &cli.config {
        if let Ok(contents) = std::fs::read_to_string(config_path) {
            if let Ok(config) = serde_json::from_str::<McpConfig>(&contents) {
                if let Some(runtime_dir) = config.runtime_dir {
                    return Ok(runtime_dir);
                }
                if let Some(parent) = config.graph_source.parent() {
                    return Ok(parent.to_path_buf());
                }
            }
        }
    }
    let graph = cli
        .graph
        .as_ref()
        .map(PathBuf::from)
        .or_else(|| std::env::var("M1ND_GRAPH_SOURCE").ok().map(PathBuf::from));
    if let Some(graph) = graph {
        return Ok(graph
            .parent()
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(".")));
    }
    std::env::current_dir().map_err(|error| format!("cannot read current dir: {error}"))
}

/// Resolve `--attach auto` to a concrete owner: first the live serve ReadWrite
/// owner for this client's runtime_root, and failing that the live serve owner
/// whose declared ingest roots COVER this caller's repo. Both passes are pure
/// read-only registry inspection and take NO lease.
///
/// The caller root comes from `attach_client::resolve_caller_root` — the same
/// resolution the bridge will stamp as the hop-2 `M1nd-Caller-Root` header, so
/// the client cannot pick an owner by one root and introduce itself with another.
#[cfg(feature = "serve")]
fn resolve_attach_auto(cli: &Cli) -> Result<m1nd_mcp::instance_registry::DiscoveredOwner, String> {
    use m1nd_mcp::instance_registry::OwnerDiscovery;

    let runtime_root = resolve_attach_runtime_root(cli)?;
    let registry_dir = cli.registry_dir.as_ref().map(PathBuf::from);
    let caller_root = m1nd_mcp::attach_client::resolve_caller_root().map(PathBuf::from);

    eprintln!(
        "[m1nd-mcp][attach] auto-discovery: runtime_root={} caller_root={}",
        runtime_root.display(),
        caller_root
            .as_deref()
            .map(|root| root.display().to_string())
            .unwrap_or_else(|| "<unresolved>".to_string())
    );

    let owner = m1nd_mcp::instance_registry::discover_serve_owner(
        &runtime_root,
        caller_root.as_deref(),
        registry_dir.as_deref(),
    )?;
    match &owner.discovery {
        OwnerDiscovery::RuntimeRoot => eprintln!(
            "[m1nd-mcp][attach] auto-discovery resolved owner: {} (owns this runtime_root)",
            owner.base_url
        ),
        OwnerDiscovery::IngestCoverage { declared_root, .. } => eprintln!(
            "[m1nd-mcp][attach] auto-discovery resolved owner: {} (declares ingest root {declared_root}, runtime_root {})",
            owner.base_url,
            owner.runtime_root.display()
        ),
    }
    Ok(owner)
}

/// Resolve the owner-local HTTP transport credential without creating or
/// rotating it. Explicit raw/file overrides support managed launchers; otherwise
/// attach reads the token beside the runtime root of the owner it is about to
/// speak to.
///
/// `owner_runtime_root` is THE OWNER's, handed back by discovery — not the
/// client's. The second discovery question resolves owners whose runtime root
/// differs from the client's, and each owner's token lives in its OWN runtime
/// root (`LocalHttpSecurity::load_or_create`), so falling back to the client's
/// would read the wrong credential or none at all. It is derived at runtime from
/// the registry entry; no personal path is ever compiled in. Falls back to the
/// client's runtime root only when no owner was discovered (an explicit
/// `--attach <url>` or `M1ND_ATTACH_URL`), which is the historical behavior.
#[cfg(feature = "serve")]
fn resolve_attach_bearer_token(
    cli: &Cli,
    owner_runtime_root: Option<&std::path::Path>,
) -> Result<String, String> {
    if let Ok(token) = std::env::var("M1ND_HTTP_BEARER_TOKEN") {
        let token = token.trim();
        if token.len() == 64 && token.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Ok(token.to_string());
        }
        return Err("M1ND_HTTP_BEARER_TOKEN must be exactly 64 hexadecimal characters".into());
    }
    let token_path = if let Ok(path) = std::env::var("M1ND_HTTP_BEARER_TOKEN_FILE") {
        if path.trim().is_empty() {
            return Err("M1ND_HTTP_BEARER_TOKEN_FILE is empty".into());
        }
        PathBuf::from(path)
    } else {
        let root = match owner_runtime_root {
            Some(root) => root.to_path_buf(),
            None => resolve_attach_runtime_root(cli)?,
        };
        root.join(m1nd_mcp::http_security::HTTP_AUTH_TOKEN_FILE_NAME)
    };
    m1nd_mcp::http_security::read_existing_bearer_token(&token_path).map_err(|error| {
        format!(
            "cannot read owner HTTP bearer token at {}: {error}",
            token_path.display()
        )
    })
}

/// Resolve the graph-source path, honoring the `temp` sentinel.
///
/// The bare value `temp` means "ephemeral graph, I never read the snapshot
/// back" — it is NOT a relative path. Treating it literally made
/// `save_graph` write a multi-MB file named `temp` into the current
/// directory (see field-triage #2). We resolve it to a per-process file
/// under the OS temp dir so persistence keeps working without ever
/// littering the CWD / repo root.
fn resolve_graph_source(path: PathBuf) -> PathBuf {
    if path.as_os_str() == "temp" {
        std::env::temp_dir().join(format!("m1nd-graph-{}.snapshot", std::process::id()))
    } else {
        path
    }
}

/// Anchor a persist target against the runtime root when it is relative.
///
/// BUG (field-triage batch B): the launchd-spawned `serve` owner runs with
/// `cwd=/` (the plist has no `WorkingDirectory`; `/` is a sealed, read-only
/// volume). The default `graph_source` / `plasticity_state` are RELATIVE
/// (`./graph_snapshot.json`, `./plasticity_state.json`), so every persist —
/// the graph snapshot, the plasticity state, and `ingest_roots.json` (written
/// next to the snapshot) — resolved against `/` and failed with
/// `Read-only file system (os error 30)`. The medulla therefore re-ingested
/// the whole repo on every boot and warm-boot never worked (`graph_path_exists:
/// false`, "No graph snapshot found, starting fresh").
///
/// When a `runtime_dir` is configured, a RELATIVE persist target must resolve
/// against it (the runtime dir is always writable and process-independent of
/// cwd — the embedding cache, boot memory, and daemon state already anchor
/// there). An EXPLICIT absolute override (a real `--graph /abs/path`) is left
/// exactly as given. With no runtime_dir, behavior is unchanged (relative to
/// cwd), preserving the plain `m1nd-mcp` stdio-in-a-repo workflow.
fn anchor_persist_target(path: PathBuf, runtime_dir: Option<&std::path::Path>) -> PathBuf {
    match runtime_dir {
        Some(root) if path.is_relative() => root.join(path),
        _ => path,
    }
}

fn load_config_from_cli(cli: &Cli) -> McpConfig {
    // Priority: --config file > --graph/--plasticity/--domain flags > env vars > defaults

    // Read-only attach: --read-only flag OR M1ND_READ_ONLY=1 (any non-"0"/"false").
    // Resolved up-front so it can be forced on top of a config file too.
    let force_read_only = cli.read_only
        || std::env::var("M1ND_READ_ONLY")
            .map(|v| v != "0" && v != "false" && !v.is_empty())
            .unwrap_or(false);

    // 1. Try config file
    if let Some(ref path) = cli.config {
        if let Ok(contents) = std::fs::read_to_string(path) {
            if let Ok(mut config) = serde_json::from_str::<McpConfig>(&contents) {
                eprintln!("[m1nd-mcp] Config loaded from {}", path);
                // --read-only / env always wins over a config file (safety opt-in).
                config.read_only = config.read_only || force_read_only;
                return config;
            }
        }
    }

    // 2. Build from CLI flags + env vars

    // Resolve the runtime dir FIRST: relative persist targets anchor against it
    // so they never resolve against cwd (field-triage batch B: launchd cwd=/ is
    // read-only). `--runtime-dir` wins over `M1ND_RUNTIME_DIR`.
    let runtime_dir = cli
        .runtime_dir
        .as_ref()
        .map(PathBuf::from)
        .or_else(|| std::env::var("M1ND_RUNTIME_DIR").ok().map(PathBuf::from));

    let graph_source = cli
        .graph
        .as_ref()
        .map(PathBuf::from)
        .or_else(|| std::env::var("M1ND_GRAPH_SOURCE").ok().map(PathBuf::from))
        .or_else(|| std::env::var("GRAPH_SNAPSHOT_PATH").ok().map(PathBuf::from))
        .map(resolve_graph_source)
        .unwrap_or_else(|| PathBuf::from("./graph_snapshot.json"));
    let graph_source = anchor_persist_target(graph_source, runtime_dir.as_deref());

    let plasticity_state = cli
        .plasticity
        .as_ref()
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var("M1ND_PLASTICITY_STATE")
                .ok()
                .map(PathBuf::from)
        })
        .or_else(|| {
            std::env::var("PLASTICITY_STATE_PATH")
                .ok()
                .map(PathBuf::from)
        })
        .unwrap_or_else(|| PathBuf::from("./plasticity_state.json"));
    let plasticity_state = anchor_persist_target(plasticity_state, runtime_dir.as_deref());

    let registry_dir = cli
        .registry_dir
        .as_ref()
        .map(PathBuf::from)
        .or_else(|| std::env::var("M1ND_REGISTRY_DIR").ok().map(PathBuf::from));

    let xlr_enabled = std::env::var("M1ND_XLR_ENABLED")
        .map(|v| v != "0" && v != "false")
        .unwrap_or(true);

    let domain = match cli.domain.as_str() {
        "code" | "music" | "memory" | "generic" => Some(cli.domain.clone()),
        _ => None,
    };

    McpConfig {
        graph_source,
        plasticity_state,
        runtime_dir,
        registry_dir,
        xlr_enabled,
        domain,
        read_only: force_read_only,
        ..McpConfig::default()
    }
}

/// One-shot mailbox triage (MEDULLA-PRD §9.2). Boots a `SessionState` to recover
/// the canonical runtime root + the known project brains, distributes the spool
/// into per-project boxes + the medulla box (unless `no_distribute`), then prints
/// the cross-box sweep as JSON. Pure operator convenience — OFF the MCP surface.
fn run_inbox_sweep(config: McpConfig, no_distribute: bool) {
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    let server = match McpServer::new(config) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("[m1nd-mcp][inbox_sweep] failed to boot session: {e}");
            std::process::exit(1);
        }
    };
    let (runtime_root, project_root) = server.offline_operator_context();
    let worktree_base = project_root
        .as_deref()
        .map(m1nd_mcp::mailbox::project_basename)
        .unwrap_or_default();

    // Known project roots: the bound root + every disk-roster brain store.
    let mut roots: Vec<String> = Vec::new();
    if let Some(bound) = project_root {
        roots.push(bound);
    }
    let registry = m1nd_mcp::project_brains::ProjectBrainRegistry::new(
        runtime_root.join(m1nd_mcp::project_brains::PROJECT_BRAINS_DIR),
        None,
    );
    for (_key, facts, _dir) in registry.disk_roster() {
        roots.push(facts.project_root);
    }

    let roots = match m1nd_mcp::mailbox::project_roots_for_runtime(&runtime_root, &roots) {
        Ok(roots) => roots,
        Err(e) => {
            eprintln!("[m1nd-mcp][inbox_sweep] failed to scope project boxes: {e}");
            std::process::exit(1);
        }
    };
    let (known, mut boxes) = m1nd_mcp::mailbox::boxes_from_roots(&roots, &worktree_base);
    boxes.push(m1nd_mcp::mailbox::KnownBox {
        label: "medulla".into(),
        path: m1nd_mcp::mailbox::medulla_box_path(&runtime_root),
        reachable: true,
    });

    let spool = m1nd_mcp::mailbox::spool_path_for_runtime(&runtime_root);
    let foreign: std::collections::BTreeSet<String> =
        ["context7", "browseros", "playwright", "semgrep"]
            .iter()
            .map(|s| s.to_string())
            .collect();

    let distribution = if no_distribute {
        None
    } else {
        match m1nd_mcp::mailbox::distribute(&spool, &runtime_root, &worktree_base, &known) {
            Ok(r) => Some(r),
            Err(e) => {
                eprintln!("[m1nd-mcp][inbox_sweep] distribution failed: {e}");
                std::process::exit(1);
            }
        }
    };

    // Re-derive boxes AFTER distribution so a box born this run is swept.
    let (_known2, mut boxes2) = m1nd_mcp::mailbox::boxes_from_roots(&roots, &worktree_base);
    boxes2.push(m1nd_mcp::mailbox::KnownBox {
        label: "medulla".into(),
        path: m1nd_mcp::mailbox::medulla_box_path(&runtime_root),
        reachable: true,
    });
    let _ = boxes; // the pre-distribution list is superseded by boxes2

    let sweep = match m1nd_mcp::mailbox::inbox_sweep(&spool, &boxes2, &foreign) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("[m1nd-mcp][inbox_sweep] sweep failed: {e}");
            std::process::exit(1);
        }
    };

    let out = serde_json::json!({
        "schema": "m1nd-inbox-sweep-v0",
        "spool_path": spool.to_string_lossy(),
        "distribution": distribution.map(|d| serde_json::json!({
            "spool_total": d.spool_total,
            "appended": d.appended,
            "to_project": d.to_project,
            "to_medulla": d.to_medulla,
            "pending": d.pending,
        })),
        "total": sweep.total,
        "open": sweep.open,
        "misdelivery": sweep.misdelivery,
        "unreachable": sweep.unreachable,
        "letters": sweep.letters,
    });
    println!("{}", serde_json::to_string_pretty(&out).unwrap_or_default());
    let _: BTreeMap<String, PathBuf> = known; // keep the type explicit for clarity
}

/// One-shot MEDULLA storage-split migration (MEDULLA-PRD §4.2, slice M5a). Boots a
/// `SessionState` to recover the canonical runtime root + the bound project root,
/// derives the two store dirs + `ingest_roots.json` from them exactly like
/// `run_inbox_sweep` does, then runs the requested verb and prints JSON. `plan` is
/// the pure dry-run (mutates nothing); `apply` is the gated backup-first split
/// (mutates the store); `rollback` restores the medulla store from its most recent
/// `apply` backup. Operator convenience — OFF the MCP surface, offline, no server
/// transport.
fn run_medulla_migrate(
    config: McpConfig,
    mode: m1nd_mcp::cli::MedullaMigrateMode,
    migrate_project_root: Option<String>,
) {
    use m1nd_mcp::cli::MedullaMigrateMode;
    use m1nd_mcp::medulla_migration::MedullaMigration;
    use m1nd_mcp::project_brains::{ProjectBrainRegistry, PROJECT_BRAINS_DIR};

    let server = match McpServer::new(config) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("[m1nd-mcp][medulla_migrate] failed to boot session: {e}");
            std::process::exit(1);
        }
    };
    let (runtime_root, ambient_project_root) = server.offline_operator_context();

    // The medulla store IS the owner runtime root's `agent-memory/` (the tier is
    // the directory, §4.1 — no move). The project brain's store is the destination
    // repo's per-project `agent-memory/` under `project-brains/<fingerprint>/`.
    //
    // The destination repo MUST be named explicitly (`--migrate-project-root`), and
    // is NEVER derived from the ambient session binding: a second agent that bound
    // the owner to an unrelated repo once caused the migration to move legacy
    // memories into the wrong brain's store, a silent cross-brain contamination
    // (field bug 2026-07-05, MED-INV-1). `apply`/`rollback` therefore REQUIRE the
    // flag; `plan` may fall back to the ambient binding but flags it as unsafe.
    let medulla_dir = runtime_root.join("agent-memory");
    let ingest_roots_path = runtime_root.join("ingest_roots.json");

    // Resolve the destination origin + whether it was explicit. For `apply` and
    // `rollback` an absent flag is a hard refusal (exit != 0) — the store must
    // never be mutated against a destination guessed from the environment.
    let (project_origin, destination_source) = match (&migrate_project_root, mode) {
        (Some(root), _) => (root.clone(), "explicit"),
        (None, MedullaMigrateMode::Apply | MedullaMigrateMode::Rollback) => {
            eprintln!(
                "[m1nd-mcp][medulla_migrate] {} requires --migrate-project-root <repo>: the \
                 destination brain must be named explicitly, never derived from the ambient \
                 session binding (field bug 2026-07-05)",
                match mode {
                    MedullaMigrateMode::Apply => "apply",
                    MedullaMigrateMode::Rollback => "rollback",
                    MedullaMigrateMode::Plan => unreachable!(),
                }
            );
            std::process::exit(2);
        }
        (None, MedullaMigrateMode::Plan) => {
            eprintln!(
                "[m1nd-mcp][medulla_migrate] WARNING: no --migrate-project-root given; plan \
                 falls back to the ambient session binding, which is UNSAFE as a destination \
                 (field bug 2026-07-05). Pass --migrate-project-root <repo> to target a brain \
                 explicitly."
            );
            (
                ambient_project_root.unwrap_or_default(),
                "ambient-binding (unsafe — pass --migrate-project-root)",
            )
        }
    };

    let registry = ProjectBrainRegistry::new(runtime_root.join(PROJECT_BRAINS_DIR), None);
    let project_dir = registry
        .store_dir_for(&ProjectBrainRegistry::canonical_key(&project_origin))
        .join("agent-memory");

    let mut mig = MedullaMigration::new(
        &medulla_dir,
        &project_dir,
        &ingest_roots_path,
        project_origin.clone(),
    );
    // Operational override for the owner-alive guard. By default the guard probes a
    // SET of loopback ports — the product's default served-owner port AND the pinned
    // medulla port — and refuses if any answers. This env var REPLACES that set with
    // one explicit port: a maintainer serving outside the set points the guard at the
    // right listener, and the CLI integration tests point it at a closed port so they
    // exercise `apply`/`rollback` without racing the machine's real owner.
    if let Ok(p) = std::env::var("M1ND_MEDULLA_GUARD_PORT") {
        if let Ok(port) = p.parse::<u16>() {
            mig = mig.with_owner_guard_port(port);
        }
    }

    let (mode_str, payload) = match mode {
        MedullaMigrateMode::Plan => match mig.plan() {
            Ok(plan) => ("plan", serde_json::to_value(&plan).unwrap_or_default()),
            Err(e) => {
                eprintln!("[m1nd-mcp][medulla_migrate] plan failed: {e}");
                std::process::exit(1);
            }
        },
        MedullaMigrateMode::Apply => match mig.apply() {
            Ok(receipt) => {
                // Register the destination brain so the owner can MOUNT the moved
                // memories. `apply` is pure-filesystem (no SessionState) and cannot
                // register itself; without this the store is an orphan `resolve`/
                // `knows` never return (field report 2026-07-05T22:31). Reuses the
                // SAME manifest birth path a bootstrap uses — never a fork.
                if let Err(e) = registry.ensure_registered(&project_origin) {
                    eprintln!(
                        "[m1nd-mcp][medulla_migrate] apply moved the memories but FAILED to \
                         register the destination brain ({e}); the store may be unmountable — \
                         resolve this before relying on the migrated memories"
                    );
                    std::process::exit(1);
                }
                ("apply", serde_json::to_value(&receipt).unwrap_or_default())
            }
            Err(e) => {
                eprintln!("[m1nd-mcp][medulla_migrate] apply failed: {e}");
                std::process::exit(1);
            }
        },
        MedullaMigrateMode::Rollback => {
            // Find the most recent `.m5a-backup-*` dir written by a prior apply.
            let backup = match most_recent_backup(&medulla_dir) {
                Some(b) => b,
                None => {
                    eprintln!(
                        "[m1nd-mcp][medulla_migrate] rollback: no backup found under {}",
                        medulla_dir.display()
                    );
                    std::process::exit(1);
                }
            };
            // The moved files come from the AUTHORITATIVE manifest `apply` wrote in
            // the backup dir — `rollback` reads it and returns exactly what it
            // removed. We do NOT scan the project store to derive this list: that
            // scan would sweep up (and delete) claims that already lived in the
            // destination brain before the migration, a silent data-loss vector.
            // The empty slice is only a legacy fallback for pre-manifest backups.
            match mig.rollback(&backup.to_string_lossy(), &[]) {
                Ok(removed) => (
                    "rollback",
                    serde_json::json!({
                        "restored_from": backup.to_string_lossy(),
                        "removed_from_project": removed,
                    }),
                ),
                Err(e) => {
                    eprintln!("[m1nd-mcp][medulla_migrate] rollback failed: {e}");
                    std::process::exit(1);
                }
            }
        }
    };

    let out = serde_json::json!({
        "schema": "m1nd-medulla-migrate-v0",
        "mode": mode_str,
        "medulla_dir": medulla_dir.to_string_lossy(),
        "project_dir": project_dir.to_string_lossy(),
        "project_origin": project_origin,
        "destination_source": destination_source,
        "plan": payload,
    });
    println!("{}", serde_json::to_string_pretty(&out).unwrap_or_default());
}

/// THE BIRTH CEREMONY (`docs/GENESIS-INGEST-CONSUMERS-SPEC.md` §2, owner-ratified
/// 2026-07-29) — the P2 human gesture, and the ONLY place in this binary that
/// stamps a `HumanOrigin`.
///
/// This function IS the admission §2 requires. It is not that the CLI presents a
/// token the owner then trusts: the owner is this process, and the fact it
/// observes is its own ingress — the human ran `m1nd init --birth <repo>`, which
/// runs this binary with this flag. Nothing that arrives over a transport can
/// reach here, because this runs before any transport is opened, so no header,
/// field, or claimed origin can ever produce the stamp. That is the whole
/// mechanism, and its honest limit is stated in `brain_birth`: it closes the
/// reflex vector, not a hostile same-UID process.
///
/// Offline and one-shot, exactly like `--inbox-sweep` and `--medulla-migrate`:
/// boot a session to recover the runtime root and the owner's own binding, run
/// the verb, print JSON, exit. Exit code follows the answer — `0` for a
/// certificate, `1` for a refusal — so a script or a human sees the difference
/// without parsing.
fn run_birth_ceremony(config: McpConfig, root: &str) {
    eprintln!(
        "m1nd: creating the graph for {root} — reading the repo, this can take a moment on a large one…"
    );
    let server = match McpServer::new(config) {
        Ok(server) => server,
        Err(e) => {
            eprintln!("[m1nd-mcp][birth] failed to boot the owner session: {e}");
            std::process::exit(1);
        }
    };
    // The ORCHESTRATION lives in the library, not here. The ceremony needs the
    // owner's own binding in order to REFUSE a root the bound dev graph already
    // covers, and that binding is a crate-internal capability by design
    // (`McpServer::into_session_state` is `pub(crate)`, guarded by a
    // `compile_fail` doctest). The binary contributes the INGRESS — the fact
    // that a human ran this command — and nothing else.
    let payload = match m1nd_mcp::brain_birth::run_ceremony(server, root, "m1nd-init-birth") {
        Ok(payload) => payload,
        Err(e) => {
            eprintln!("[m1nd-mcp][birth] the ceremony failed: {e}");
            std::process::exit(1);
        }
    };
    let ok = payload
        .get("ok")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    println!(
        "{}",
        serde_json::to_string_pretty(&payload).unwrap_or_default()
    );
    if ok {
        eprintln!(
            "{}",
            m1nd_mcp::brain_birth::birth_receipt_human_line(&payload)
        );
    }
    if !ok {
        std::process::exit(1);
    }
}

/// The most recent `.m5a-backup-<ms>` dir under a medulla store, if any. The
/// suffix is `now_ms()` at `apply` time, so lexical max over the numeric suffix
/// is the newest backup (the rollback anchor).
fn most_recent_backup(medulla_dir: &std::path::Path) -> Option<PathBuf> {
    const PREFIX: &str = ".m5a-backup-";
    std::fs::read_dir(medulla_dir)
        .ok()?
        .flatten()
        .filter_map(|e| {
            let path = e.path();
            let name = path.file_name()?.to_str()?.to_string();
            let suffix = name.strip_prefix(PREFIX)?;
            let stamp: u128 = suffix.parse().ok()?;
            path.is_dir().then_some((stamp, path))
        })
        .max_by_key(|(stamp, _)| *stamp)
        .map(|(_, path)| path)
}

async fn run_stdio_server(config: McpConfig, event_log: Option<String>, no_gui: bool, _port: u16) {
    if event_log.is_some() {
        eprintln!(
            "[m1nd-mcp] NOTE: --event-log in stdio-only mode writes events for external consumers."
        );
        eprintln!(
            "[m1nd-mcp]       For cross-process SSE, use --serve --stdio --event-log <path>."
        );
    }

    // Spawn background HTTP GUI server (unless --no-gui or serve feature disabled)
    #[cfg(feature = "serve")]
    let _gui_handle: Option<tokio::task::JoinHandle<()>> = if !no_gui {
        eprintln!(
            "[m1nd-mcp] Auto GUI disabled in stdio mode while multi-instance runtime leases are active."
        );
        eprintln!(
            "[m1nd-mcp] Use `m1nd-mcp --serve --stdio` when you want one shared HTTP + stdio instance."
        );
        None
    } else {
        None
    };

    #[cfg(not(feature = "serve"))]
    let _ = (no_gui, _port); // suppress unused warnings

    let mut server = match McpServer::new(config) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("[m1nd-mcp] Failed to create server: {}", e);
            return;
        }
    };

    if let Err(e) = server.start() {
        eprintln!("[m1nd-mcp] Failed to start server: {}", e);
        if let Err(shutdown_error) = server.shutdown() {
            eprintln!(
                "[m1nd-mcp] Failed to shut down after startup refusal: {}",
                shutdown_error
            );
        }
        return;
    }

    let heartbeat = match server.spawn_instance_heartbeat() {
        Ok(heartbeat) => heartbeat,
        Err(error) => {
            eprintln!("[m1nd-mcp] Failed to start owner heartbeat: {error}");
            if let Err(shutdown_error) = server.shutdown() {
                eprintln!(
                    "[m1nd-mcp] Failed to shut down after heartbeat refusal: {}",
                    shutdown_error
                );
            }
            return;
        }
    };
    let shutdown = server.shutdown_handle();

    // Spawn the serve loop in a blocking task (synchronous stdio I/O)
    let mut serve_handle = tokio::task::spawn_blocking(move || {
        let serve_result = server.serve();
        let shutdown_result = server.shutdown();
        match shutdown_result {
            Err(error) => Err(error),
            Ok(()) => serve_result,
        }
    });

    // A signal requests cooperative return from the blocking loop, then awaits
    // the same serve+checkpoint+release task. The heartbeat is revoked only
    // after that lifecycle transaction has completed.
    let result = tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            eprintln!("[m1nd-mcp] SIGINT received.");
            shutdown.request_shutdown();
            (&mut serve_handle).await
        }
        result = &mut serve_handle => {
            result
        }
    };
    match result {
        Ok(Ok(())) => {}
        Ok(Err(e)) => eprintln!("[m1nd-mcp] Server error: {}", e),
        Err(e) => eprintln!("[m1nd-mcp] Task error: {}", e),
    }
    heartbeat.abort();
    let _ = heartbeat.await;
}

/// Pure strict-version decision (no I/O, no exit) so it is unit-testable in
/// both directions. Returns `Some(one_line_error)` when the process MUST refuse
/// to start, else `None`.
///
/// Refuse iff strict mode is on AND an explicit expectation
/// (`expected_version` / `expected_sha`) is set and differs from the running
/// identity. Only the explicit env expectations are consulted here — no graph,
/// no bound repo — because this runs before any session exists.
fn strict_version_verdict(
    strict: bool,
    running_version: &str,
    running_sha: &str,
    expected_version: Option<&str>,
    expected_sha: Option<&str>,
) -> Option<String> {
    if !strict {
        return None;
    }
    let version_mismatch = expected_version
        .map(|expected| expected.trim() != running_version)
        .unwrap_or(false);
    let sha_mismatch = expected_sha
        .map(|expected| expected.trim() != running_sha)
        .unwrap_or(false);
    if version_mismatch || sha_mismatch {
        Some(format!(
            "[m1nd-mcp] STRICT VERSION REFUSAL: running {running_version} ({running_sha}) but expected version={} sha={} (M1ND_STRICT_VERSION=1). Refusing to start against a mismatched binary.",
            expected_version.unwrap_or("<unset>"),
            expected_sha.unwrap_or("<unset>"),
        ))
    } else {
        None
    }
}

/// Strict version-honesty gate (the hardest layer of the honesty moat).
///
/// When `M1ND_STRICT_VERSION` is truthy AND an explicit expectation
/// (`M1ND_EXPECTED_VERSION` / `M1ND_EXPECTED_SHA`) does not match this running
/// binary, REFUSE to start with a one-line error and a nonzero exit. This is for
/// harnesses/experiments that must NEVER run the wrong binary (the exact
/// incident: an old beta.8 binary silently used in an experiment). Uses only the
/// compile-time identity — no graph, no session, no lease — so it is safe to run
/// before anything else. Non-strict callers get warnings instead (in the
/// handshake/selftest honest surface); this only bites when opted in.
fn enforce_strict_version() {
    let strict = std::env::var("M1ND_STRICT_VERSION")
        .map(|v| v != "0" && v != "false" && !v.trim().is_empty())
        .unwrap_or(false);
    let expected_version = std::env::var("M1ND_EXPECTED_VERSION")
        .ok()
        .filter(|v| !v.trim().is_empty());
    let expected_sha = std::env::var("M1ND_EXPECTED_SHA")
        .ok()
        .filter(|v| !v.trim().is_empty());

    if let Some(error) = strict_version_verdict(
        strict,
        m1nd_mcp::session::BINARY_VERSION,
        m1nd_mcp::session::BINARY_GIT_SHA,
        expected_version.as_deref(),
        expected_sha.as_deref(),
    ) {
        eprintln!("{error}");
        std::process::exit(2);
    }
}

/// Run ONE custody-ceremony step and exit. Never returns.
///
/// The ceremony's admission is this ingress itself (`--custody-ceremony`), which
/// is why the stamp is constructed here and only here. Everything else the step
/// needs is owner-held and passed explicitly: this binary derives no path and
/// creates no directory, so the owner's own prerequisites stay visible to them.
fn run_custody_ceremony_mode(cli: &Cli, verb: &str) -> ! {
    use m1nd_mcp::custody_ceremony::{
        CeremonyAttendanceV1, CeremonyRequestV1, CustodyCeremonyVerbV1, OwnerCeremonyIngressV1,
        CUSTODY_CEREMONY_VERBS,
    };

    let parsed: CustodyCeremonyVerbV1 = match verb.parse() {
        Ok(parsed) => parsed,
        Err(refusal) => {
            println!(
                "{}",
                serde_json::to_string_pretty(&refusal.to_json()).unwrap_or_default()
            );
            eprintln!(
                "[m1nd-mcp][custody] expected one of: {}",
                CUSTODY_CEREMONY_VERBS.join(", ")
            );
            std::process::exit(1);
        }
    };

    let protected_root = match cli.custody_protected_root.as_deref() {
        Some(root) => std::path::PathBuf::from(root),
        None => {
            eprintln!(
                "[m1nd-mcp][custody] --custody-protected-root is required: the ceremony's \
                 protected root is owner-held and this binary never derives or creates it"
            );
            std::process::exit(1);
        }
    };

    let (payload, code) = m1nd_mcp::custody_ceremony::run_custody_ceremony(
        OwnerCeremonyIngressV1::from_cli_ingress(),
        CeremonyRequestV1 {
            verb: parsed,
            protected_root,
            owner_security_config: cli
                .custody_owner_security_config
                .as_deref()
                .map(std::path::PathBuf::from),
            mission_config: cli
                .custody_mission_config
                .as_deref()
                .map(std::path::PathBuf::from),
            independence_spec: cli
                .custody_independence_spec
                .as_deref()
                .map(std::path::PathBuf::from),
            constitution_digest: cli.custody_constitution_digest.clone(),
        },
        CeremonyAttendanceV1::detect(),
    );
    println!(
        "{}",
        serde_json::to_string_pretty(&payload).unwrap_or_default()
    );
    std::process::exit(code);
}

#[tokio::main]
async fn main() {
    // Parse before every side-effecting compatibility/runtime path. The offline
    // receipt verifier is intentionally an early mode: it reads stdin, consults
    // the system clock, writes one JSON proof, and exits.
    let cli = Cli::parse();

    // Version-honesty strict gate — refuse a mismatched binary before doing any
    // work (see `enforce_strict_version`). No-op unless M1ND_STRICT_VERSION is set.
    enforce_strict_version();

    if cli.verify_authorization_receipt {
        std::process::exit(
            m1nd_mcp::authorization_receipt_verifier::run_authorization_receipt_verifier_stdio(),
        );
    }

    // --custody-ceremony <verb>: THE CUSTODY CEREMONY (amendment G9-A1, Path B —
    // docs/benchmarks/G9-CUSTODY-CEREMONY.md §2). One bounded step, offline, one
    // closed JSON object, exit — the same early-mode shape as the receipt verifier
    // above and --inbox-sweep/--medulla-migrate below. Dispatched HERE, before any
    // config load or owner machinery, because the ceremony must never boot an
    // owner, open a port or take a lease.
    //
    // THIS is the stamp's only construction site. Admission to the ceremony is a
    // fact the owner observes about ITSELF — the human ran this command — so the
    // ingress, not a payload, is what mints it. A test holds this line: a second
    // `from_cli_ingress` anywhere in the crate fails the battery.
    if let Some(verb) = cli.custody_ceremony.clone() {
        run_custody_ceremony_mode(&cli, &verb);
    }

    // --seal-independence-spec <path>: fill a hand-authored IndependenceSpecV1's
    // digest from the digest of its own core and print the sealed document. One
    // bounded offline step, one closed JSON object, exit — the same early-mode
    // shape as the custody family above, and dispatched here for the same reason:
    // it must never boot an owner, open a port or take a lease.
    //
    // It is NOT a ceremony step and carries no ingress stamp, because it needs
    // none: it reads a file and computes a digest. It never touches the enclave,
    // the keychain, the protected root or any ceremony state, so nothing an agent
    // is forbidden to perform is reachable through it.
    if let Some(path) = cli.seal_independence_spec.clone() {
        let (payload, code) = m1nd_mcp::seal_independence_spec::run_seal_independence_spec(
            std::path::Path::new(&path),
        );
        println!(
            "{}",
            serde_json::to_string_pretty(&payload).unwrap_or_default()
        );
        std::process::exit(code);
    }

    #[cfg(unix)]
    ensure_bwrap_compat_wrapper();

    // --discover-owner: ask `--attach auto`'s two questions, print the answer,
    // exit. One bounded read-only step — no bridge, no graph, no lease, no port
    // — so it is dispatched here, beside `--attach` and before any config load.
    //
    // The runtime root and caller root are resolved by the SAME helpers the
    // bridge uses, so the probe can never answer about one identity while the
    // attach that follows it presents another.
    if cli.discover_owner {
        #[cfg(feature = "serve")]
        {
            let runtime_root = match resolve_attach_runtime_root(&cli) {
                Ok(root) => root,
                Err(message) => {
                    eprintln!("[m1nd-mcp][discover-owner] {message}");
                    std::process::exit(1);
                }
            };
            let caller_root = m1nd_mcp::attach_client::resolve_caller_root().map(PathBuf::from);
            let registry_dir = cli.registry_dir.as_ref().map(PathBuf::from);
            let probe = m1nd_mcp::instance_registry::probe_serve_owner(
                &runtime_root,
                caller_root.as_deref(),
                registry_dir.as_deref(),
            );
            let found = probe.found;
            println!(
                "{}",
                serde_json::to_string_pretty(&probe).unwrap_or_default()
            );
            std::process::exit(if found { 0 } else { 1 });
        }
        #[cfg(not(feature = "serve"))]
        {
            eprintln!("[m1nd-mcp] --discover-owner requires the 'serve' feature.");
            eprintln!("  Rebuild with: cargo build --release --features serve");
            std::process::exit(1);
        }
    }

    // --attach: thin stdio↔HTTP bridge. This path loads NO graph, builds NO
    // engines, and takes NO lease — it must NEVER reach `McpServer::new`. It is
    // handled before `load_config_from_cli`/`--serve`/stdio so none of that
    // owner-side machinery runs.
    if let Some(attach_arg) = cli.attach.clone() {
        #[cfg(feature = "serve")]
        {
            // Resolve the owner base URL. Precedence:
            //   1. env M1ND_ATTACH_URL  — explicit override, always wins.
            //   2. `--attach auto`      — discover the live serve owner via the
            //                             registry (read-only, NO lease): first by
            //                             this runtime_root, then by which owner
            //                             has ingested this caller's repo.
            //   3. `--attach <url>`     — use the literal URL verbatim.
            //
            // Only the auto path knows the owner's OWN runtime root, which is
            // where that owner's bearer token lives; the two explicit paths name a
            // URL and nothing more, so they keep the historical token fallback.
            let (base_url, owner_runtime_root) = match std::env::var("M1ND_ATTACH_URL") {
                Ok(url) if !url.trim().is_empty() => {
                    eprintln!(
                        "[m1nd-mcp][attach] using M1ND_ATTACH_URL override: {}",
                        url.trim()
                    );
                    (url.trim().to_string(), None)
                }
                _ if attach_arg.trim().eq_ignore_ascii_case("auto") => {
                    match resolve_attach_auto(&cli) {
                        Ok(owner) => (owner.base_url, Some(owner.runtime_root)),
                        Err(msg) => {
                            eprintln!("[m1nd-mcp][attach] auto-discovery failed: {msg}");
                            std::process::exit(1);
                        }
                    }
                }
                _ => (attach_arg, None),
            };
            let bearer_token =
                match resolve_attach_bearer_token(&cli, owner_runtime_root.as_deref()) {
                    Ok(token) => token,
                    Err(message) => {
                        eprintln!("[m1nd-mcp][attach] authentication setup failed: {message}");
                        std::process::exit(1);
                    }
                };
            m1nd_mcp::attach_client::run_attach_client(base_url, bearer_token).await;
            return;
        }
        #[cfg(not(feature = "serve"))]
        {
            let _ = attach_arg;
            eprintln!("[m1nd-mcp] --attach requires the 'serve' feature (HTTP client).");
            eprintln!("  Rebuild with: cargo build --release --features serve");
            std::process::exit(1);
        }
    }

    let config = load_config_from_cli(&cli);

    // --inbox-sweep: the one-shot triage hand (MEDULLA-PRD §9.2, §C6.2 — CLI/REST
    // only, OFF the MCP surface). Distribute the spool into per-project boxes +
    // the medulla box (idempotent, LOCAL, safe to re-run — telemetry not memory),
    // then print the cross-box sweep and exit. Runs BEFORE --serve/stdio so it
    // never boots a server transport.
    if cli.inbox_sweep {
        run_inbox_sweep(config, cli.no_distribute);
        return;
    }

    // --medulla-migrate plan|apply|rollback: the one-shot MEDULLA storage-split
    // migration (MEDULLA-PRD §4.2, slice M5a). Derives every path from the runtime
    // root like --inbox-sweep, runs offline, prints JSON, and exits. `plan` is a
    // pure dry-run; `apply`/`rollback` mutate the store (CODE-LAND-ONLY posture —
    // for the maintainer, never an agent). Runs BEFORE --serve/stdio.
    if let Some(mode) = cli.medulla_migrate {
        run_medulla_migrate(config, mode, cli.migrate_project_root);
        return;
    }

    // --birth <repo>: THE BIRTH CEREMONY (GENESIS-INGEST-CONSUMERS-SPEC.md §2,
    // owner-ratified 2026-07-29). Runs BEFORE --serve/stdio, so it never opens a
    // transport — which is also why the stamp it applies can never be reached
    // through one.
    if let Some(root) = cli.birth {
        run_birth_ceremony(config, &root);
        return;
    }

    let event_log = cli.event_log;
    let watch_events = cli.watch_events;

    if cli.serve {
        #[cfg(feature = "serve")]
        {
            m1nd_mcp::http_server::run(
                config,
                cli.port,
                cli.bind,
                cli.allow_remote,
                cli.dev,
                cli.ui_dir,
                cli.open,
                cli.stdio,
                event_log,
                watch_events,
            )
            .await;
        }
        #[cfg(not(feature = "serve"))]
        {
            let _ = (event_log, watch_events); // suppress unused warnings
            eprintln!("[m1nd-mcp] --serve requires the 'serve' feature.");
            eprintln!("  Rebuild with: cargo build --release --features serve");
            std::process::exit(1);
        }
    } else {
        run_stdio_server(config, event_log, cli.no_gui, cli.port).await;
    }
}

#[cfg(test)]
mod tests {
    use super::{anchor_persist_target, resolve_graph_source, strict_version_verdict};
    use std::path::PathBuf;

    // --- field-triage batch B: relative persist targets must anchor on the
    // runtime dir, never resolve against cwd (launchd cwd=/ is read-only) ---

    #[test]
    fn relative_persist_target_anchors_on_runtime_dir() {
        // BUG (field report L27/L29): the plist has no WorkingDirectory, so the
        // owner runs with cwd=/. The default `./graph_snapshot.json` then resolved
        // against `/` and every persist failed with os error 30 (read-only fs).
        let runtime = PathBuf::from("/Users/<name>/.m1nd/runtimes/claude");

        let graph = anchor_persist_target(
            PathBuf::from("./graph_snapshot.json"),
            Some(runtime.as_path()),
        );
        assert_eq!(
            graph,
            runtime.join("./graph_snapshot.json"),
            "relative graph snapshot must land under the runtime dir"
        );
        assert!(
            graph.starts_with(&runtime),
            "anchored graph path must be under the runtime dir, got {graph:?}"
        );

        // A bare relative filename anchors too (the ingest_roots.json neighbor
        // is derived from graph_source.parent(), so this fixes it transitively).
        let plas = anchor_persist_target(
            PathBuf::from("plasticity_state.json"),
            Some(runtime.as_path()),
        );
        assert_eq!(plas, runtime.join("plasticity_state.json"));
    }

    #[test]
    fn explicit_absolute_persist_target_is_never_rewritten() {
        // A real `--graph /abs/path` override must pass through untouched even
        // when a runtime dir is set: the operator asked for that exact location.
        let runtime = PathBuf::from("/Users/<name>/.m1nd/runtimes/claude");
        let explicit = PathBuf::from("/data/snapshots/graph_snapshot.json");
        assert_eq!(
            anchor_persist_target(explicit.clone(), Some(runtime.as_path())),
            explicit,
            "explicit absolute path must not be re-anchored"
        );
    }

    #[test]
    fn no_runtime_dir_leaves_relative_target_unchanged() {
        // With no runtime dir (plain `m1nd-mcp` stdio-in-a-repo), the historical
        // cwd-relative behavior is preserved.
        let rel = PathBuf::from("./graph_snapshot.json");
        assert_eq!(anchor_persist_target(rel.clone(), None), rel);
    }

    // --- field-triage #2: the `temp` graph-source sentinel must not litter CWD ---

    #[test]
    fn temp_sentinel_never_resolves_to_a_cwd_relative_temp_path() {
        // BUG (field report): `M1ND_GRAPH_SOURCE=temp` was taken literally, so
        // save_graph wrote an ~8.5MB file named `temp` into the CWD/repo root.
        let resolved = resolve_graph_source(PathBuf::from("temp"));

        // Must NOT be the bare relative `temp` (which lands in the CWD).
        assert_ne!(
            resolved,
            PathBuf::from("temp"),
            "temp sentinel still resolves to CWD `temp`"
        );
        assert!(
            resolved.is_absolute(),
            "resolved temp graph path must be absolute, got {resolved:?}"
        );

        // It must live under the OS temp dir, not the working directory.
        assert!(
            resolved.starts_with(std::env::temp_dir()),
            "temp graph snapshot must live under the OS temp dir, got {resolved:?}"
        );
        // Sanity: it keeps a snapshot-ish name so persistence still works.
        assert!(
            resolved
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("m1nd-graph-")),
            "temp graph snapshot filename should be process-scoped, got {resolved:?}"
        );
    }

    #[test]
    fn non_sentinel_graph_source_passes_through_unchanged() {
        // Any other value is a real path and must be left exactly as given.
        let explicit = PathBuf::from("/some/where/graph_snapshot.json");
        assert_eq!(resolve_graph_source(explicit.clone()), explicit);
        // A literal relative path that merely contains "temp" is NOT the sentinel.
        let looks_like = PathBuf::from("temp.json");
        assert_eq!(resolve_graph_source(looks_like.clone()), looks_like);
        let nested = PathBuf::from("./temp/graph.json");
        assert_eq!(resolve_graph_source(nested.clone()), nested);
    }

    #[test]
    fn strict_off_never_refuses_even_on_mismatch() {
        // Strict disabled => warn-only elsewhere, never a startup refusal.
        assert!(strict_version_verdict(false, "1.1.0", "abc", Some("0.0.1"), None).is_none());
    }

    #[test]
    fn strict_on_refuses_version_mismatch() {
        let verdict = strict_version_verdict(true, "1.1.0", "abc123", Some("0.0.0-beta.8"), None);
        let msg = verdict.expect("strict + mismatch must refuse");
        assert!(msg.contains("STRICT VERSION REFUSAL"));
        assert!(msg.contains("1.1.0"));
        assert!(msg.contains("0.0.0-beta.8"));
    }

    #[test]
    fn strict_on_refuses_sha_mismatch() {
        let verdict = strict_version_verdict(true, "1.1.0", "abc123", None, Some("deadbee"));
        assert!(verdict.expect("sha mismatch refuses").contains("deadbee"));
    }

    #[test]
    fn strict_on_allows_exact_match() {
        // Strict but everything matches => no refusal.
        assert!(
            strict_version_verdict(true, "1.1.0", "abc123", Some("1.1.0"), Some("abc123"))
                .is_none()
        );
    }

    #[test]
    fn strict_on_with_no_expectation_allows() {
        // Strict on but no expectation set => nothing to compare, allow start.
        assert!(strict_version_verdict(true, "1.1.0", "abc123", None, None).is_none());
    }
}
