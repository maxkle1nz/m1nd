use clap::Parser;
use m1nd_mcp::server::{dispatch_tool, McpConfig, McpServer};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};

#[derive(Parser, Debug)]
#[command(
    name = "m1nd-openclaw",
    about = "Native low-latency OpenClaw bridge for m1nd"
)]
struct Cli {
    #[arg(long, default_value = "/tmp/m1nd-openclaw.sock")]
    socket: String,

    #[arg(long)]
    config: Option<String>,

    #[arg(long)]
    graph: Option<String>,

    #[arg(long)]
    plasticity: Option<String>,

    #[arg(long)]
    runtime_dir: Option<String>,

    #[arg(long, default_value = "code")]
    domain: String,

    #[arg(long)]
    default_agent_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct BridgeRequest {
    id: Option<String>,
    tool: String,
    #[serde(default)]
    arguments: Value,
}

#[derive(Debug, Serialize)]
struct BridgeResponse {
    id: Option<String>,
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    elapsed_ms: f64,
}

fn normalize_tool_name(tool: &str) -> &str {
    tool.strip_prefix("m1nd.").unwrap_or(tool)
}

fn inject_default_agent_id(arguments: &Value, default_agent_id: &str) -> Value {
    let mut next = arguments.clone();
    if let Some(map) = next.as_object_mut() {
        if !map.contains_key("agent_id") {
            map.insert(
                "agent_id".to_string(),
                Value::String(default_agent_id.to_string()),
            );
        }
    }
    next
}

fn build_response(
    request: Result<BridgeRequest, serde_json::Error>,
    state: &Arc<Mutex<m1nd_mcp::session::SessionState>>,
    default_agent_id: &str,
) -> BridgeResponse {
    let started = std::time::Instant::now();
    match request {
        Ok(request) => {
            let tool = normalize_tool_name(&request.tool).to_string();
            let args = inject_default_agent_id(&request.arguments, default_agent_id);
            let result = {
                let mut session = state.lock();
                dispatch_tool(&mut session, &tool, &args)
            };
            match result {
                Ok(value) => BridgeResponse {
                    id: request.id,
                    ok: true,
                    result: Some(value),
                    error: None,
                    elapsed_ms: started.elapsed().as_secs_f64() * 1000.0,
                },
                Err(err) => BridgeResponse {
                    id: request.id,
                    ok: false,
                    result: None,
                    error: Some(err.to_string()),
                    elapsed_ms: started.elapsed().as_secs_f64() * 1000.0,
                },
            }
        }
        Err(err) => BridgeResponse {
            id: None,
            ok: false,
            result: None,
            error: Some(format!("invalid request: {}", err)),
            elapsed_ms: started.elapsed().as_secs_f64() * 1000.0,
        },
    }
}

fn load_config(cli: &Cli) -> McpConfig {
    if let Some(ref path) = cli.config {
        if let Ok(contents) = std::fs::read_to_string(path) {
            if let Ok(config) = serde_json::from_str::<McpConfig>(&contents) {
                return config;
            }
        }
    }

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
        .or_else(|| runtime_dir.as_ref().map(|dir| dir.join("graph.json")))
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
        .or_else(|| runtime_dir.as_ref().map(|dir| dir.join("plasticity.json")))
        .unwrap_or_else(|| PathBuf::from("./plasticity_state.json"));

    McpConfig {
        graph_source,
        plasticity_state,
        runtime_dir,
        domain: Some(cli.domain.clone()),
        ..McpConfig::default()
    }
}

async fn handle_client(
    stream: UnixStream,
    state: Arc<Mutex<m1nd_mcp::session::SessionState>>,
    default_agent_id: String,
) -> std::io::Result<()> {
    let (reader, mut writer) = stream.into_split();
    let mut lines = BufReader::new(reader).lines();

    while let Some(line) = lines.next_line().await? {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let request: Result<BridgeRequest, _> = serde_json::from_str(trimmed);
        let response = build_response(request, &state, &default_agent_id);

        let encoded = serde_json::to_vec(&response)?;
        writer.write_all(&encoded).await?;
        writer.write_all(b"\n").await?;
        writer.flush().await?;
    }

    Ok(())
}

fn remove_stale_socket(path: &Path) -> std::io::Result<()> {
    if path.exists() {
        std::fs::remove_file(path)?;
    }
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    let socket_path = PathBuf::from(&cli.socket);

    if let Some(parent) = socket_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    remove_stale_socket(&socket_path)?;

    let config = load_config(&cli);
    let mut server = McpServer::new(config)?;
    server.start()?;
    let state = Arc::new(Mutex::new(server.into_session_state()));
    let default_agent_id = cli
        .default_agent_id
        .clone()
        .or_else(|| std::env::var("M1ND_OPENCLAW_AGENT_ID").ok())
        .unwrap_or_else(|| "openclaw".to_string());

    let listener = UnixListener::bind(&socket_path)?;
    eprintln!(
        "[m1nd-openclaw] Native bridge listening on {}",
        socket_path.display()
    );

    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                break;
            }
            accepted = listener.accept() => {
                let (stream, _) = accepted?;
                let state = Arc::clone(&state);
                let default_agent_id = default_agent_id.clone();
                tokio::spawn(async move {
                    if let Err(err) = handle_client(stream, state, default_agent_id).await {
                        eprintln!("[m1nd-openclaw] client error: {}", err);
                    }
                });
            }
        }
    }

    let _ = std::fs::remove_file(&socket_path);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{inject_default_agent_id, normalize_tool_name};
    use serde_json::json;

    #[test]
    fn normalize_tool_name_strips_m1nd_prefix() {
        assert_eq!(normalize_tool_name("m1nd.search"), "search");
        assert_eq!(normalize_tool_name("search"), "search");
    }

    #[test]
    fn inject_default_agent_id_only_when_missing() {
        let args = json!({"query":"abc"});
        let patched = inject_default_agent_id(&args, "openclaw");
        assert_eq!(patched["agent_id"], "openclaw");
        assert_eq!(patched["query"], "abc");

        let existing = json!({"agent_id":"custom","query":"abc"});
        let preserved = inject_default_agent_id(&existing, "openclaw");
        assert_eq!(preserved["agent_id"], "custom");
    }
}
