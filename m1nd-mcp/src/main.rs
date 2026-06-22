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

#[tokio::main]
async fn main() {
    #[cfg(unix)]
    ensure_bwrap_compat_wrapper();

    let cli = Cli::parse();

    // --attach: thin stdio↔HTTP bridge. This path loads NO graph, builds NO
    // engines, and takes NO lease — it must NEVER reach `McpServer::new`. It is
    // handled before `load_config_from_cli`/`--serve`/stdio so none of that
    // owner-side machinery runs.
    if let Some(base_url) = cli.attach.clone() {
        #[cfg(feature = "serve")]
        {
            m1nd_mcp::attach_client::run_attach_client(base_url).await;
            return;
        }
        #[cfg(not(feature = "serve"))]
        {
            let _ = base_url;
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
