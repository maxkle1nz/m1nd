//! Agent autonomy: an authenticated loopback HTTP owner prepares exactly the
//! workspace granted by its launcher before the first MCP query.
#![cfg(all(unix, feature = "serve"))]

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

const BIN: &str = env!("CARGO_BIN_EXE_m1nd-mcp");
const AGENT: &str = "agent-autonomy-http-probe";
const SYMBOL: &str = "http_first_query_signal_6d91";

fn write_fixture(root: &Path, symbol: &str) {
    std::fs::create_dir_all(root.join("src")).expect("fixture src");
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"repo-alpha\"\nversion = \"0.0.0\"\n",
    )
    .expect("fixture manifest");
    std::fs::write(
        root.join("src/lib.rs"),
        format!("pub fn {symbol}() -> u64 {{ 42 }}\n"),
    )
    .expect("fixture source");
}

fn source_snapshot(root: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    ["Cargo.toml", "src/lib.rs"]
        .into_iter()
        .map(|relative| {
            let path = root.join(relative);
            (path.clone(), std::fs::read(path).expect("fixture bytes"))
        })
        .collect()
}

fn free_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("ephemeral port");
    listener.local_addr().expect("listener address").port()
}

fn confined_command(workspace: &Path, runtime: &Path, port: u16) -> Command {
    let home = runtime.join("home");
    let temp = runtime.join("tmp");
    std::fs::create_dir_all(&home).expect("fixture home");
    std::fs::create_dir_all(&temp).expect("fixture temp");
    let mut command = Command::new(BIN);
    command
        .arg("--serve")
        .arg("--port")
        .arg(port.to_string())
        .arg("--no-gui")
        .current_dir(workspace);
    for name in [
        "M1ND_WORKSPACE_ROOT",
        "M1ND_PROJECT_ROOT",
        "M1ND_REPO_ROOT",
        "WORKSPACE_ROOT",
        "PROJECT_ROOT",
        "REPO_ROOT",
        "CLAUDE_PROJECT_DIR",
        "CLAUDE_WORKSPACE_ROOT",
        "ANTHROPIC_WORKSPACE_ROOT",
        "ANTIGRAVITY_WORKSPACE_ROOT",
        "ANTIGRAVITY_PROJECT_ROOT",
        "GEMINI_WORKSPACE_ROOT",
        "GEMINI_PROJECT_ROOT",
        "CURSOR_WORKSPACE_ROOT",
        "CURSOR_PROJECT_ROOT",
        "WINDSURF_WORKSPACE_ROOT",
        "WINDSURF_PROJECT_ROOT",
        "VSCODE_WORKSPACE",
        "VSCODE_CWD",
        "INIT_CWD",
        "PWD",
        "OLDPWD",
        "M1ND_RUNTIME_DIR",
        "M1ND_REGISTRY_DIR",
        "M1ND_GRAPH_SOURCE",
        "GRAPH_SNAPSHOT_PATH",
        "M1ND_PLASTICITY_STATE",
        "PLASTICITY_STATE_PATH",
        "M1ND_READ_ONLY",
        "M1ND_NO_GUI",
        "HOME",
        "TMPDIR",
        "TMP",
        "TEMP",
    ] {
        command.env_remove(name);
    }
    command
        .env("M1ND_WORKSPACE_ROOT", workspace)
        .env("M1ND_RUNTIME_DIR", runtime)
        .env("M1ND_REGISTRY_DIR", runtime.join("registry"))
        .env("M1ND_NO_GUI", "1")
        .env("HOME", home)
        .env("TMPDIR", &temp)
        .env("TMP", &temp)
        .env("TEMP", temp);
    command
}

fn wait_for_refusal(mut child: Child) -> std::process::Output {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if child.try_wait().expect("poll conflicting owner").is_some() {
            return child.wait_with_output().expect("collect conflicting owner");
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let output = child.wait_with_output().expect("stop conflicting owner");
            panic!(
                "conflicting HTTP owner stayed live instead of refusing: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        std::thread::sleep(Duration::from_millis(25));
    }
}

struct Owner {
    child: Child,
    endpoint: String,
    token: String,
    session_id: String,
    client: reqwest::Client,
    next_id: i64,
}

impl Owner {
    async fn spawn(workspace: &Path, runtime: &Path) -> Self {
        Self::spawn_with_registry(workspace, runtime, None).await
    }

    async fn spawn_with_registry(
        workspace: &Path,
        runtime: &Path,
        registry: Option<&Path>,
    ) -> Self {
        std::fs::create_dir_all(runtime).expect("external runtime");
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(runtime, std::fs::Permissions::from_mode(0o700))
            .expect("owner-private external runtime");
        let port = free_port();
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(5))
            .build()
            .expect("HTTP client");
        let mut command = confined_command(workspace, runtime, port);
        if let Some(registry) = registry {
            command.env("M1ND_REGISTRY_DIR", registry);
        }
        let child = command
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn HTTP owner");
        let endpoint = format!("http://127.0.0.1:{port}/mcp");
        let token_path = runtime.join(m1nd_mcp::http_security::HTTP_AUTH_TOKEN_FILE_NAME);
        // Own cleanup before initialization too: a failed readiness assertion
        // must not leave a server running after its temporary fixture disappears.
        let mut owner = Self {
            child,
            endpoint,
            token: String::new(),
            session_id: String::new(),
            client,
            next_id: 1,
        };
        // A cold HTTP owner prepares and embeds the granted workspace before
        // publishing its fixture credential. Bound the wait without assuming
        // a 30-second startup on a contended runner.
        let deadline = Instant::now() + Duration::from_secs(120);
        let (token, session_id) = loop {
            assert!(
                owner.child.try_wait().expect("poll HTTP owner").is_none(),
                "HTTP owner exited before initialization"
            );
            let token = match m1nd_mcp::http_security::read_existing_bearer_token(&token_path) {
                Ok(token) => token,
                Err(_) if Instant::now() < deadline => {
                    tokio::time::sleep(Duration::from_millis(25)).await;
                    continue;
                }
                Err(error) => panic!("owner did not create its fixture credential: {error}"),
            };
            let response = owner
                .client
                .post(&owner.endpoint)
                .bearer_auth(&token)
                .header("accept", "application/json, text/event-stream")
                .header("content-type", "application/json")
                .header("m1nd-caller-root", workspace.to_string_lossy().as_ref())
                .json(&serde_json::json!({
                    "jsonrpc": "2.0", "id": 1, "method": "initialize",
                    "params": {
                        "protocolVersion": "2025-06-18",
                        "capabilities": {},
                        "clientInfo": { "name": AGENT, "version": "1" }
                    }
                }))
                .send()
                .await;
            if let Ok(response) = response {
                let session = response
                    .headers()
                    .get("mcp-session-id")
                    .and_then(|value| value.to_str().ok())
                    .map(str::to_owned);
                let body = response.json::<serde_json::Value>().await.ok();
                if body
                    .as_ref()
                    .and_then(|value| value.get("result"))
                    .is_some()
                {
                    break (token, session.expect("initialize session header"));
                }
            }
            assert!(Instant::now() < deadline, "HTTP owner did not initialize");
            tokio::time::sleep(Duration::from_millis(50)).await;
        };
        owner.token = token;
        owner.session_id = session_id;
        owner
    }

    async fn call_with_root(
        &mut self,
        root: &Path,
        name: &str,
        arguments: serde_json::Value,
    ) -> serde_json::Value {
        self.call_on_session(&self.session_id.clone(), Some(root), name, arguments)
            .await
    }

    async fn call_on_session(
        &mut self,
        session_id: &str,
        root: Option<&Path>,
        name: &str,
        arguments: serde_json::Value,
    ) -> serde_json::Value {
        self.next_id += 1;
        let mut request = self
            .client
            .post(&self.endpoint)
            .bearer_auth(&self.token)
            .header("accept", "application/json, text/event-stream")
            .header("content-type", "application/json")
            .header("mcp-session-id", session_id);
        if let Some(root) = root {
            request = request.header("m1nd-caller-root", root.to_string_lossy().as_ref());
        }
        let response = request
            .json(&serde_json::json!({
                "jsonrpc": "2.0", "id": self.next_id, "method": "tools/call",
                "params": { "name": name, "arguments": arguments }
            }))
            .send()
            .await
            .expect("HTTP tool request");
        assert_eq!(response.status(), reqwest::StatusCode::OK);
        response.json().await.expect("JSON-RPC response")
    }

    async fn initialize_session_without_root(&mut self) -> String {
        self.next_id += 1;
        let response = self
            .client
            .post(&self.endpoint)
            .bearer_auth(&self.token)
            .header("accept", "application/json, text/event-stream")
            .header("content-type", "application/json")
            .json(&serde_json::json!({
                "jsonrpc": "2.0", "id": self.next_id, "method": "initialize",
                "params": {
                    "protocolVersion": "2025-06-18",
                    "capabilities": {},
                    "clientInfo": { "name": AGENT, "version": "1" }
                }
            }))
            .send()
            .await
            .expect("initialize caller-unknown session");
        assert_eq!(response.status(), reqwest::StatusCode::OK);
        let session_id = response
            .headers()
            .get("mcp-session-id")
            .and_then(|value| value.to_str().ok())
            .expect("new session id")
            .to_string();
        let body: serde_json::Value = response.json().await.expect("initialize response");
        assert!(body.get("result").is_some(), "{body}");
        session_id
    }

    async fn unauthenticated_status(&self) -> reqwest::StatusCode {
        self.client
            .post(&self.endpoint)
            .header("content-type", "application/json")
            .json(&serde_json::json!({"jsonrpc":"2.0","id":99,"method":"tools/list"}))
            .send()
            .await
            .expect("unauthenticated request")
            .status()
    }

    fn shutdown(mut self) {
        self.token.clear();
        self.client = reqwest::Client::new();
        self.child.kill().expect("stop fixture owner");
        let status = self.child.wait().expect("wait fixture owner");
        assert!(
            !status.success(),
            "killed fixture owner unexpectedly exited zero"
        );
    }
}

impl Drop for Owner {
    fn drop(&mut self) {
        self.token.clear();
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

fn tool_payload(response: &serde_json::Value) -> serde_json::Value {
    assert!(
        response.get("error").is_none(),
        "JSON-RPC error: {response}"
    );
    assert_ne!(
        response["result"]["isError"].as_bool(),
        Some(true),
        "tool returned an MCP error: {response}"
    );
    response["result"]["structuredContent"]
        .clone()
        .as_object()
        .map(|_| response["result"]["structuredContent"].clone())
        .or_else(|| {
            response["result"]["content"][0]["text"]
                .as_str()
                .and_then(|text| serde_json::from_str(text).ok())
        })
        .unwrap_or_else(|| response["result"].clone())
}

#[tokio::test(flavor = "multi_thread")]
async fn authenticated_http_prepares_exact_grant_before_first_query() {
    let temp = tempfile::tempdir().expect("tempdir");
    let workspace = temp.path().join("repo-alpha");
    let sibling = temp.path().join("repo-beta");
    let runtime = temp.path().join("external-runtime");
    write_fixture(&workspace, SYMBOL);
    write_fixture(&sibling, "header_payload_must_not_grant_0a72");
    let before = source_snapshot(&workspace);

    let mut owner = Owner::spawn(&workspace, &runtime).await;
    assert_eq!(
        owner.unauthenticated_status().await,
        reqwest::StatusCode::UNAUTHORIZED,
        "the existing local bearer boundary must stay active"
    );

    let search = tool_payload(
        &owner
            .call_with_root(
                &workspace,
                "search",
                serde_json::json!({"agent_id": AGENT, "query": SYMBOL}),
            )
            .await,
    );
    assert!(
        search["total_matches"].as_u64().unwrap_or(0) > 0,
        "the first query must return matches, not merely echo its query: {search}"
    );
    let rendered = serde_json::to_string(
        search["results"]
            .as_array()
            .expect("structured search results"),
    )
    .expect("search results JSON");
    assert!(
        rendered.contains(SYMBOL) && rendered.contains("src/lib.rs"),
        "the first authenticated query must retrieve the real fixture symbol: {search}"
    );

    let sibling_search = tool_payload(
        &owner
            .call_with_root(
                &sibling,
                "search",
                serde_json::json!({
                    "agent_id": AGENT,
                    "query": "header_payload_must_not_grant_0a72",
                    "scope": sibling
                }),
            )
            .await,
    );
    // The envelope echoes the query even on refusal; only results are matches.
    assert_eq!(sibling_search["results"], serde_json::json!([]));
    assert_eq!(sibling_search["total_matches"], 0);
    assert_eq!(sibling_search["proof_state"], "blocked");

    let canonical_workspace = std::fs::canonicalize(&workspace).expect("canonical workspace");
    let refresh = tool_payload(
        &owner
            .call_with_root(
                &workspace,
                "ingest",
                serde_json::json!({
                    "agent_id": AGENT,
                    "path": canonical_workspace,
                    "mode": "refresh",
                    "adapter": "code"
                }),
            )
            .await,
    );
    assert_eq!(refresh["ok"], serde_json::json!(true), "{refresh}");
    let graph_after_success =
        std::fs::read(runtime.join("graph_snapshot.json")).expect("graph after refresh");
    let roots_after_success =
        std::fs::read(runtime.join("ingest_roots.json")).expect("roots after refresh");

    let unknown_session = owner.initialize_session_without_root().await;
    let unknown = tool_payload(
        &owner
            .call_on_session(
                &unknown_session,
                None,
                "ingest",
                serde_json::json!({
                    "agent_id": AGENT,
                    "path": canonical_workspace,
                    "mode": "refresh",
                    "adapter": "code"
                }),
            )
            .await,
    );
    assert_eq!(unknown["ok"], serde_json::json!(false), "{unknown}");
    assert_eq!(
        unknown["refused"],
        serde_json::json!("refresh_caller_root_unknown"),
        "{unknown}"
    );

    let foreign = tool_payload(
        &owner
            .call_with_root(
                &sibling,
                "ingest",
                serde_json::json!({
                    "agent_id": AGENT,
                    "path": canonical_workspace,
                    "mode": "refresh",
                    "adapter": "code"
                }),
            )
            .await,
    );
    assert_eq!(foreign["ok"], serde_json::json!(false), "{foreign}");
    assert_eq!(
        foreign["refused"],
        serde_json::json!("refresh_root_not_exact"),
        "{foreign}"
    );
    assert_eq!(
        std::fs::read(runtime.join("graph_snapshot.json")).unwrap(),
        graph_after_success,
        "HTTP refresh refusals changed the successful snapshot"
    );
    assert_eq!(
        std::fs::read(runtime.join("ingest_roots.json")).unwrap(),
        roots_after_success,
        "HTTP refresh refusals changed the declared roots"
    );

    let graph = m1nd_core::snapshot::load_graph(&runtime.join("graph_snapshot.json"))
        .expect("independent snapshot readback");
    assert!(
        graph
            .nodes
            .label
            .iter()
            .any(|label| graph.strings.resolve(*label).contains(SYMBOL)),
        "persisted graph must independently contain the fixture symbol"
    );
    assert!(
        graph.nodes.label.iter().all(|label| !graph
            .strings
            .resolve(*label)
            .contains("header_payload_must_not_grant_0a72")),
        "a foreign query must not import the sibling into the persisted graph"
    );
    let roots: Vec<String> = serde_json::from_slice(
        &std::fs::read(runtime.join("ingest_roots.json")).expect("persisted roots"),
    )
    .expect("root list JSON");
    assert_eq!(
        roots,
        vec![std::fs::canonicalize(&workspace)
            .expect("canonical fixture")
            .to_string_lossy()
            .to_string()],
        "the persisted identity must contain only the exact launcher grant"
    );
    assert_eq!(
        source_snapshot(&workspace),
        before,
        "source fixture changed"
    );
    assert!(!workspace.join(".m1nd").exists(), "cache escaped runtime");
    owner.shutdown();
}

#[tokio::test(flavor = "multi_thread")]
async fn http_launcher_registry_override_cannot_escape_private_runtime() {
    let temp = tempfile::tempdir().expect("tempdir");
    let workspace = temp.path().join("repo-alpha");
    let runtime = temp.path().join("external-runtime");
    let injected_registry = workspace.join("injected-registry");
    write_fixture(&workspace, SYMBOL);
    let owner = Owner::spawn_with_registry(&workspace, &runtime, Some(&injected_registry)).await;
    let confined_registry = runtime.join("registry");
    assert!(
        !m1nd_mcp::instance_registry::list_instances(Some(&confined_registry))
            .expect("private registry readable")
            .is_empty(),
        "live HTTP owner must advertise under the private runtime"
    );
    assert!(
        !injected_registry.exists(),
        "HTTP must not create or publish under injected registry in source workspace"
    );
    let listing: serde_json::Value = owner
        .client
        .get(owner.endpoint.replace("/mcp", "/api/instances"))
        .bearer_auth(&owner.token)
        .send()
        .await
        .expect("HTTP instance listing")
        .json()
        .await
        .expect("instance listing JSON");
    assert!(
        listing["instances"]
            .as_array()
            .is_some_and(|entries| !entries.is_empty()),
        "HTTP listing must read from its private registry, not an ambient override: {listing}"
    );
    owner.shutdown();
}

#[tokio::test(flavor = "multi_thread")]
async fn http_conflicting_grant_refuses_and_preserves_snapshot_and_roots() {
    let temp = tempfile::tempdir().expect("tempdir");
    let original = temp.path().join("repo-alpha");
    let conflicting = temp.path().join("repo-beta");
    let runtime = temp.path().join("external-runtime");
    write_fixture(&original, SYMBOL);
    write_fixture(&conflicting, "http_conflicting_signal_3e18");

    let owner = Owner::spawn(&original, &runtime).await;
    owner.shutdown();
    let graph_before = std::fs::read(runtime.join("graph_snapshot.json")).expect("graph before");
    let roots_before = std::fs::read(runtime.join("ingest_roots.json")).expect("roots before");
    let port = free_port();
    let child = confined_command(&conflicting, &runtime, port)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("run conflicting owner");
    let output = wait_for_refusal(child);
    assert!(
        !output.status.success(),
        "conflicting HTTP owner must refuse"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("launcher_workspace_conflicts_with_bound_graph"),
        "conflicting HTTP refusal must preserve the shared root validator: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        std::fs::read(runtime.join("graph_snapshot.json")).unwrap(),
        graph_before
    );
    assert_eq!(
        std::fs::read(runtime.join("ingest_roots.json")).unwrap(),
        roots_before
    );
}
