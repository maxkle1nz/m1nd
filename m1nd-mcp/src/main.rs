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
use m1nd_mcp::instance_registry::spawn_heartbeat;
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

/// Resolve `--attach auto` to a concrete owner base URL by discovering the live
/// serve ReadWrite owner for this client's runtime_root.
///
/// The runtime_root is computed with the SAME rule the owner uses
/// (`session.rs` runtime-root default): explicit `--runtime-dir` if given, else
/// the parent directory of `--graph` if given, else the current working dir. The
/// discovery itself is read-only and takes NO lease.
#[cfg(feature = "serve")]
fn resolve_attach_auto(cli: &Cli) -> Result<String, String> {
    let runtime_root: PathBuf = if let Some(dir) = &cli.runtime_dir {
        PathBuf::from(dir)
    } else if let Some(graph) = &cli.graph {
        PathBuf::from(graph)
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."))
    } else {
        std::env::current_dir().map_err(|e| format!("cannot read current dir: {e}"))?
    };

    let registry_dir = cli.registry_dir.as_ref().map(PathBuf::from);

    eprintln!(
        "[m1nd-mcp][attach] auto-discovery: runtime_root={}",
        runtime_root.display()
    );

    let url = m1nd_mcp::instance_registry::discover_serve_owner_base_url(
        &runtime_root,
        registry_dir.as_deref(),
    )?;
    eprintln!("[m1nd-mcp][attach] auto-discovery resolved owner: {url}");
    Ok(url)
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
    let graph_source = cli
        .graph
        .as_ref()
        .map(PathBuf::from)
        .or_else(|| std::env::var("M1ND_GRAPH_SOURCE").ok().map(PathBuf::from))
        .or_else(|| std::env::var("GRAPH_SNAPSHOT_PATH").ok().map(PathBuf::from))
        .map(resolve_graph_source)
        .unwrap_or_else(|| PathBuf::from("./graph_snapshot.json"));

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

    let runtime_dir = std::env::var("M1ND_RUNTIME_DIR").ok().map(PathBuf::from);
    let runtime_dir = cli.runtime_dir.as_ref().map(PathBuf::from).or(runtime_dir);
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
            std::process::exit(1);
        }
    };

    if let Err(e) = server.start() {
        eprintln!("[m1nd-mcp] Failed to start server: {}", e);
        std::process::exit(1);
    }

    let _heartbeat = spawn_heartbeat(server.instance_handle());

    // Spawn the serve loop in a blocking task (synchronous stdio I/O)
    let serve_handle = tokio::task::spawn_blocking(move || {
        let result = server.serve();
        let _ = server.shutdown();
        result
    });

    // Wait for either SIGINT or serve completion
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            eprintln!("[m1nd-mcp] SIGINT received.");
        }
        result = serve_handle => {
            match result {
                Ok(Ok(())) => {}
                Ok(Err(e)) => eprintln!("[m1nd-mcp] Server error: {}", e),
                Err(e) => eprintln!("[m1nd-mcp] Task error: {}", e),
            }
        }
    }
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

#[tokio::main]
async fn main() {
    #[cfg(unix)]
    ensure_bwrap_compat_wrapper();

    // Version-honesty strict gate — refuse a mismatched binary before doing any
    // work (see `enforce_strict_version`). No-op unless M1ND_STRICT_VERSION is set.
    enforce_strict_version();

    let cli = Cli::parse();

    // --attach: thin stdio↔HTTP bridge. This path loads NO graph, builds NO
    // engines, and takes NO lease — it must NEVER reach `McpServer::new`. It is
    // handled before `load_config_from_cli`/`--serve`/stdio so none of that
    // owner-side machinery runs.
    if let Some(attach_arg) = cli.attach.clone() {
        #[cfg(feature = "serve")]
        {
            // Resolve the owner base URL. Precedence:
            //   1. env M1ND_ATTACH_URL  — explicit override, always wins.
            //   2. `--attach auto`      — discover the live serve ReadWrite owner
            //                             for this runtime_root via the registry
            //                             (read-only, NO lease).
            //   3. `--attach <url>`     — use the literal URL verbatim.
            let base_url = match std::env::var("M1ND_ATTACH_URL") {
                Ok(url) if !url.trim().is_empty() => {
                    eprintln!(
                        "[m1nd-mcp][attach] using M1ND_ATTACH_URL override: {}",
                        url.trim()
                    );
                    url.trim().to_string()
                }
                _ if attach_arg.trim().eq_ignore_ascii_case("auto") => {
                    match resolve_attach_auto(&cli) {
                        Ok(url) => url,
                        Err(msg) => {
                            eprintln!("[m1nd-mcp][attach] auto-discovery failed: {msg}");
                            std::process::exit(1);
                        }
                    }
                }
                _ => attach_arg,
            };
            m1nd_mcp::attach_client::run_attach_client(base_url).await;
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

    let event_log = cli.event_log;
    let watch_events = cli.watch_events;

    if cli.serve {
        #[cfg(feature = "serve")]
        {
            m1nd_mcp::http_server::run(
                config,
                cli.port,
                cli.bind,
                cli.dev,
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
    use super::{resolve_graph_source, strict_version_verdict};
    use std::path::PathBuf;

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
