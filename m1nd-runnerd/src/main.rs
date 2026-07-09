//! `m1nd-runnerd` binary (HUMAN-VIEW-V2-F2.5c §5) — the loopback runner daemon.
//!
//! Boot: resolve the runtime root (shared with the owner), load `runners.toml` (the
//! pinned capabilities, §5a), ensure the `0600` shared secret, ANNOUNCE liveness to
//! the owner (§5a), then serve `POST /run` on `127.0.0.1:<port>` (default 1339). Every
//! request carries the shared secret in `x-runnerd-secret`; a spawn runs in an
//! isolated worktree and streams phase letters through the owner. The daemon binds
//! loopback only and the same-UID threat is declared out of scope (§5d).

use std::path::PathBuf;
use std::sync::Arc;

use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::post,
    Json, Router,
};
use clap::Parser;

use m1nd_mcp::runnerd_owner::{secret_matches, RUNNERD_SECRET_HEADER};
use m1nd_runnerd::config::{self, RunnersConfig};
use m1nd_runnerd::mission::{self, EngineOpts, RunRequest};
use m1nd_runnerd::owner::HttpOwnerClient;

/// The runner daemon CLI (§5). `--runtime-dir` MUST match the owner's — the shared
/// secret + the side record live there.
#[derive(Parser, Debug)]
#[command(
    name = "m1nd-runnerd",
    about = "The m1nd runner daemon (F2.5c) — the only spawner"
)]
struct Cli {
    /// Loopback port to serve `/run` on (default 1339).
    #[arg(long, default_value = "1339")]
    port: u16,

    /// The runtime root shared with the owner (the secret + side record live here).
    /// Absent → the current directory (the owner's default rule).
    #[arg(long)]
    runtime_dir: Option<String>,

    /// The owner's base URL for announce + mission_post (default the served owner).
    #[arg(long, default_value = "http://127.0.0.1:1338")]
    owner_url: String,
}

/// Shared daemon state for the `/run` handler.
#[derive(Clone)]
struct DaemonState {
    config: Arc<RunnersConfig>,
    secret: String,
    owner_url: String,
    worktree_base: PathBuf,
    runtime_root: PathBuf,
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    let runtime_root: PathBuf = cli
        .runtime_dir
        .as_ref()
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

    // Load the pinned runners (§5a). No config → the daemon can spawn nothing; refuse
    // to boot with an honest message pointing at the example.
    let config = match config::load(&runtime_root) {
        Ok(c) => Arc::new(c),
        Err(e) => {
            eprintln!(
                "[m1nd-runnerd] cannot start: {e}\n  expected {}/{} — see runners.example.toml",
                runtime_root.display(),
                config::RUNNERS_CONFIG_FILE
            );
            std::process::exit(1);
        }
    };

    // The shared secret (§5a) — created 0600 on first boot, read thereafter.
    let secret = match config::ensure_secret(&runtime_root) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("[m1nd-runnerd] cannot create the shared secret: {e}");
            std::process::exit(1);
        }
    };

    let worktree_base = std::env::temp_dir().join("m1nd-runnerd-worktrees");
    if let Err(e) = std::fs::create_dir_all(&worktree_base) {
        eprintln!(
            "[m1nd-runnerd] cannot create the worktree base {}: {e}",
            worktree_base.display()
        );
        std::process::exit(1);
    }

    let runner_ids: Vec<String> = config.runners.iter().map(|r| r.id.clone()).collect();
    eprintln!(
        "[m1nd-runnerd] booting: {} pinned runner(s) {:?}, port {}, owner {}",
        runner_ids.len(),
        runner_ids,
        cli.port,
        cli.owner_url
    );

    // Announce liveness to the owner (§5a) + a periodic heartbeat so last_seen stays
    // fresh and the daemon re-registers if the owner restarts. Announce failure is a
    // WARNING, never fatal: the daemon still serves; the operator can retry the owner.
    spawn_announce_heartbeat(cli.owner_url.clone(), secret.clone(), runner_ids, cli.port);

    let state = DaemonState {
        config,
        secret,
        owner_url: cli.owner_url,
        worktree_base,
        runtime_root,
    };

    let app = Router::new()
        .route("/run", post(handle_run))
        .with_state(state);

    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], cli.port));
    let listener = match tokio::net::TcpListener::bind(addr).await {
        Ok(l) => l,
        Err(e) => {
            eprintln!("[m1nd-runnerd] cannot bind {addr}: {e}");
            std::process::exit(1);
        }
    };
    eprintln!("[m1nd-runnerd] serving /run on http://{addr}");
    if let Err(e) = axum::serve(listener, app).await {
        eprintln!("[m1nd-runnerd] server error: {e}");
        std::process::exit(1);
    }
}

/// A random hex challenge for the announce liveness round-trip (§5a).
fn boot_challenge() -> String {
    use rand::RngCore;
    let mut b = [0u8; 8];
    rand::rng().fill_bytes(&mut b);
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// Announce once, then re-announce every 30s (liveness heartbeat, §5a). Detached.
fn spawn_announce_heartbeat(owner_url: String, secret: String, runner_ids: Vec<String>, port: u16) {
    tokio::spawn(async move {
        let client = HttpOwnerClient::new(owner_url);
        loop {
            let challenge = boot_challenge();
            match client
                .announce(&secret, &runner_ids, port, &challenge)
                .await
            {
                Ok(()) => {}
                Err(e) => eprintln!("[m1nd-runnerd] announce to owner failed (will retry): {e}"),
            }
            tokio::time::sleep(std::time::Duration::from_secs(30)).await;
        }
    });
}

/// `POST /run` (§B) — the mission entry. Authenticates the shared secret (401 bare),
/// gates the pinned runner + workspace allowlist (403), then ACCEPTS: mints the
/// mission id, spawns the engine in the background, and returns immediately. The
/// letters (`judging → executing → merge_wait|failed`) stream via the owner; the tray
/// watches them. The owner never spawns — this daemon is the only spawner (§5d).
async fn handle_run(
    State(state): State<DaemonState>,
    headers: HeaderMap,
    Json(req): Json<RunRequest>,
) -> axum::response::Response {
    // Secret (§5a): a missing/wrong header is a BARE 401.
    let provided = headers
        .get(RUNNERD_SECRET_HEADER)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if !secret_matches(&state.secret, provided) {
        return StatusCode::UNAUTHORIZED.into_response();
    }

    // Pinned runner + workspace allowlist (§B.1/§B.2) → 403 with the daemon's keyword.
    let runner = match mission::validate_run(&state.config, &req) {
        Ok(r) => r.clone(),
        Err(refusal) => {
            let status = StatusCode::from_u16(refusal.status()).unwrap_or(StatusCode::FORBIDDEN);
            return (
                status,
                Json(serde_json::json!({
                    "error": refusal.keyword(),
                    "detail": refusal.detail(),
                })),
            )
                .into_response();
        }
    };

    let mission_id = req
        .mission_id
        .clone()
        .filter(|m| !m.is_empty())
        .unwrap_or_else(mission::mint_mission_id);
    let runner_id_echo = req.runner_id.clone();

    // Accept + run in the background. The owner client is built inside the task; the
    // engine posts letters and cleans the worktree (§B.7) on its own.
    let opts = EngineOpts {
        worktree_base: state.worktree_base.clone(),
        runtime_root: state.runtime_root.clone(),
    };
    let owner_url = state.owner_url.clone();
    let mid = mission_id.clone();
    tokio::spawn(async move {
        let client = HttpOwnerClient::new(owner_url);
        mission::run_mission(&client, &runner, &req, &mid, &opts).await;
    });

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "mission_id": mission_id,
            "accepted": true,
            "runner_id": runner_id_echo,
        })),
    )
        .into_response()
}
