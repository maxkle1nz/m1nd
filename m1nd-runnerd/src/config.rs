//! F2.5c — the runner daemon's owner-local config (§5a): the PINNED runners
//! (`runners.toml`) and the shared secret. Capabilities are pinned HERE, in local
//! config, and NOWHERE else — announce proves liveness only and can never widen a
//! pin (§5a). Parsing is honest per field: a bad runner names its id and the field
//! that failed, never a vague "invalid config".

use std::path::{Path, PathBuf};

use serde::Deserialize;

use m1nd_mcp::mission_letter::Capability;
use m1nd_mcp::runnerd_owner::RUNNERD_SECRET_FILE;

/// The config file name inside the runtime root.
pub const RUNNERS_CONFIG_FILE: &str = "runners.toml";

/// The `{packet_file}` substitution token a runner's `command` MUST contain — the
/// packet path is written into the worktree and spliced in at spawn time (§5b).
pub const PACKET_FILE_TOKEN: &str = "{packet_file}";

/// The default per-mission timeout (§5b/§B.4): 30 minutes.
pub const DEFAULT_TIMEOUT_SECS: u64 = 1800;

/// The parsed `runners.toml` (§5a) — a list of pinned runners.
#[derive(Debug, Clone, Deserialize)]
pub struct RunnersConfig {
    /// `[[runner]]` tables. Renamed so the TOML array key is the singular `runner`.
    #[serde(default, rename = "runner")]
    pub runners: Vec<RunnerDef>,
}

/// One pinned runner (§5a/§5b): an id, its single allowed capability, the one-shot
/// command template, the gate command, and the workspace-root allowlist.
#[derive(Debug, Clone, Deserialize)]
pub struct RunnerDef {
    pub id: String,
    /// `build-runner` | `naming-runner` (the MVP capabilities, §5b). The other three
    /// (§5e) are declared out of MVP and refused here.
    pub capability: String,
    /// `["<operator's agent CLI>", …, "{packet_file}"]` — must include the token.
    pub command: Vec<String>,
    /// The gate the daemon runs after the agent exits, hashing its full log (§5c).
    #[serde(default)]
    pub gate_command: Vec<String>,
    /// Absolute repo roots this runner may run in (§5a). A spawn whose workspace is
    /// not under one of these is refused `workspace_not_allowed`.
    #[serde(default)]
    pub workspace_allowlist: Vec<String>,
    /// Per-mission kill timeout in seconds (§5b, default 30 min).
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
}

fn default_timeout() -> u64 {
    DEFAULT_TIMEOUT_SECS
}

impl RunnerDef {
    /// The parsed capability (validated at load, so this cannot fail post-validate).
    pub fn parsed_capability(&self) -> Capability {
        parse_capability(&self.capability).unwrap_or(Capability::BuildRunner)
    }
}

/// The honest per-field config refusals (§5a: "erro honesto por campo").
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("cannot read runners config at {path}: {source}")]
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("runners.toml is not valid TOML: {detail}")]
    Parse { detail: String },
    #[error("runner '{runner}': field `{field}` {detail}")]
    Field {
        runner: String,
        field: &'static str,
        detail: String,
    },
    #[error("runners.toml has no [[runner]] entries — a daemon with no pinned runners can spawn nothing")]
    Empty,
    #[error("duplicate runner id '{0}' — runner ids must be unique")]
    DuplicateId(String),
}

/// Map one of the MVP capability strings to the frozen [`Capability`] (§5b). Only
/// `build-runner` and `naming-runner` are MVP; the other three (§5e) are refused.
pub fn parse_capability(s: &str) -> Result<Capability, String> {
    match s.trim() {
        "build-runner" => Ok(Capability::BuildRunner),
        "naming-runner" => Ok(Capability::NamingRunner),
        other => Err(format!(
            "'{other}' is not an MVP capability — pin `build-runner` or `naming-runner` (loop/hand/review-runner are out of the MVP, §5e)"
        )),
    }
}

/// Load + validate the config from the runtime root (§5a).
pub fn load(runtime_root: &Path) -> Result<RunnersConfig, ConfigError> {
    let path = runtime_root.join(RUNNERS_CONFIG_FILE);
    let text = std::fs::read_to_string(&path).map_err(|e| ConfigError::Read {
        path: path.clone(),
        source: e,
    })?;
    parse(&text)
}

/// Parse + validate config TOML text (§5a) — the testable heart of [`load`].
pub fn parse(text: &str) -> Result<RunnersConfig, ConfigError> {
    let cfg: RunnersConfig = toml::from_str(text).map_err(|e| ConfigError::Parse {
        detail: e.to_string(),
    })?;
    validate(&cfg)?;
    Ok(cfg)
}

/// Validate every runner honestly, per field (§5a).
pub fn validate(cfg: &RunnersConfig) -> Result<(), ConfigError> {
    if cfg.runners.is_empty() {
        return Err(ConfigError::Empty);
    }
    let mut seen: Vec<&str> = Vec::new();
    for r in &cfg.runners {
        let field = |field: &'static str, detail: &str| ConfigError::Field {
            runner: r.id.clone(),
            field,
            detail: detail.to_string(),
        };
        if r.id.trim().is_empty() {
            return Err(field("id", "must be a non-empty runner id"));
        }
        if seen.contains(&r.id.as_str()) {
            return Err(ConfigError::DuplicateId(r.id.clone()));
        }
        seen.push(r.id.as_str());

        if let Err(detail) = parse_capability(&r.capability) {
            return Err(field("capability", &detail));
        }
        if r.command.is_empty() {
            return Err(field(
                "command",
                "must name the agent CLI (at least one element)",
            ));
        }
        if !r.command.iter().any(|a| a.contains(PACKET_FILE_TOKEN)) {
            return Err(field(
                "command",
                "must contain the `{packet_file}` token — the daemon writes the packet into the worktree and splices its path here",
            ));
        }
        if r.gate_command.is_empty() {
            return Err(field(
                "gate_command",
                "must name the gate the daemon runs after the agent exits (§5c)",
            ));
        }
        if r.workspace_allowlist.is_empty() {
            return Err(field(
                "workspace_allowlist",
                "must list at least one absolute repo root this runner may run in (§5a)",
            ));
        }
        for w in &r.workspace_allowlist {
            if !Path::new(w).is_absolute() {
                return Err(field(
                    "workspace_allowlist",
                    &format!(
                        "'{w}' is not an absolute path — the allowlist is absolute repo roots"
                    ),
                ));
            }
        }
        if r.timeout_secs == 0 {
            return Err(field("timeout_secs", "must be greater than zero"));
        }
    }
    Ok(())
}

/// Find a pinned runner by id (§5b.1 — an unpinned id is refused upstream).
pub fn find<'a>(cfg: &'a RunnersConfig, runner_id: &str) -> Option<&'a RunnerDef> {
    cfg.runners.iter().find(|r| r.id == runner_id)
}

/// Whether `workspace` is inside (or equal to) one of the runner's allowlisted roots
/// (§5b.2). Canonicalizes both sides so a `/var`→`/private/var` alias or a trailing
/// slash resolves; falls back to a raw prefix compare when canonicalization fails
/// (e.g. a not-yet-existing path) — the allowlist is the gate, never a filesystem probe.
pub fn workspace_allowed(runner: &RunnerDef, workspace: &str) -> bool {
    let canon = |p: &str| std::fs::canonicalize(p).unwrap_or_else(|_| PathBuf::from(p));
    let ws = canon(workspace);
    runner.workspace_allowlist.iter().any(|root| {
        let root = canon(root);
        ws == root || ws.starts_with(&root)
    })
}

/// Ensure the shared secret exists in the runtime root (§5a): on first boot create
/// `runnerd.secret` (`0600`, 32 random bytes hex); thereafter read it back. The
/// OWNER reads the SAME file to authenticate announce + sign the spawn proxy. The
/// filename is the owner's const (single source of truth).
pub fn ensure_secret(runtime_root: &Path) -> std::io::Result<String> {
    use std::io::Write;
    std::fs::create_dir_all(runtime_root)?;
    let path = runtime_root.join(RUNNERD_SECRET_FILE);

    if let Ok(existing) = std::fs::read_to_string(&path) {
        let t = existing.trim().to_string();
        if !t.is_empty() {
            return Ok(t);
        }
    }

    use rand::RngCore;
    let mut bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();

    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let mut f = opts.open(&path)?;
    f.write_all(hex.as_bytes())?;
    Ok(hex)
}

#[cfg(test)]
mod tests {
    use super::*;

    const GOOD: &str = r#"
[[runner]]
id = "build-1"
capability = "build-runner"
command = ["agent-cli", "run", "{packet_file}"]
gate_command = ["cargo", "test"]
workspace_allowlist = ["/abs/repo"]

[[runner]]
id = "name-1"
capability = "naming-runner"
command = ["namer", "{packet_file}"]
gate_command = ["true"]
workspace_allowlist = ["/abs/repo"]
"#;

    #[test]
    fn valid_config_parses_both_runners() {
        let cfg = parse(GOOD).expect("valid config parses");
        assert_eq!(cfg.runners.len(), 2);
        assert_eq!(cfg.runners[0].parsed_capability(), Capability::BuildRunner);
        assert_eq!(cfg.runners[1].timeout_secs, DEFAULT_TIMEOUT_SECS);
        assert!(find(&cfg, "name-1").is_some());
        assert!(find(&cfg, "ghost").is_none());
    }

    #[test]
    fn empty_config_is_refused() {
        assert!(matches!(parse("").unwrap_err(), ConfigError::Empty));
    }

    #[test]
    fn bad_capability_names_the_field() {
        let toml = r#"
[[runner]]
id = "x"
capability = "loop-runner"
command = ["c", "{packet_file}"]
gate_command = ["t"]
workspace_allowlist = ["/abs"]
"#;
        let err = parse(toml).expect_err("loop-runner is out of the MVP");
        match err {
            ConfigError::Field { field, .. } => assert_eq!(field, "capability"),
            other => panic!("expected a capability field error, got {other}"),
        }
    }

    #[test]
    fn command_without_packet_file_token_is_refused() {
        let toml = r#"
[[runner]]
id = "x"
capability = "build-runner"
command = ["agent-cli", "run"]
gate_command = ["t"]
workspace_allowlist = ["/abs"]
"#;
        let err = parse(toml).expect_err("no {packet_file} token");
        assert!(err.to_string().contains("packet_file"), "got {err}");
    }

    #[test]
    fn each_required_field_is_checked() {
        // missing gate_command
        let no_gate = r#"
[[runner]]
id = "x"
capability = "build-runner"
command = ["c", "{packet_file}"]
workspace_allowlist = ["/abs"]
"#;
        assert!(matches!(
            parse(no_gate).unwrap_err(),
            ConfigError::Field {
                field: "gate_command",
                ..
            }
        ));

        // relative allowlist entry
        let rel = r#"
[[runner]]
id = "x"
capability = "build-runner"
command = ["c", "{packet_file}"]
gate_command = ["t"]
workspace_allowlist = ["not/absolute"]
"#;
        assert!(matches!(
            parse(rel).unwrap_err(),
            ConfigError::Field {
                field: "workspace_allowlist",
                ..
            }
        ));

        // duplicate id
        let dup = r#"
[[runner]]
id = "x"
capability = "build-runner"
command = ["c", "{packet_file}"]
gate_command = ["t"]
workspace_allowlist = ["/abs"]
[[runner]]
id = "x"
capability = "naming-runner"
command = ["c", "{packet_file}"]
gate_command = ["t"]
workspace_allowlist = ["/abs"]
"#;
        assert!(matches!(
            parse(dup).unwrap_err(),
            ConfigError::DuplicateId(_)
        ));
    }

    #[test]
    fn workspace_allowlist_prefix_and_alias() {
        let cfg = parse(GOOD).unwrap();
        let r = find(&cfg, "build-1").unwrap();
        assert!(workspace_allowed(r, "/abs/repo"), "exact root");
        assert!(workspace_allowed(r, "/abs/repo/sub/dir"), "under the root");
        assert!(!workspace_allowed(r, "/abs/other"), "outside the root");
        assert!(!workspace_allowed(r, "/abs"), "the parent is not allowed");
    }

    #[test]
    fn ensure_secret_creates_then_reuses() {
        let dir = tempfile::tempdir().unwrap();
        let a = ensure_secret(dir.path()).unwrap();
        assert_eq!(a.len(), 64, "32 bytes → 64 hex chars");
        let b = ensure_secret(dir.path()).unwrap();
        assert_eq!(a, b, "the secret is stable across boots");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(dir.path().join(RUNNERD_SECRET_FILE))
                .unwrap()
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(mode, 0o600, "the secret is 0600");
        }
    }
}
