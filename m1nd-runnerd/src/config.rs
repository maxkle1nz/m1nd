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

/// The default per-BLOCK timeout for the F11-b `/name` naming lane: 20 seconds.
/// Naming is the cheap/fast lane — a hung runner must not hold a batch hostage.
pub const DEFAULT_NAMING_TIMEOUT_SECS: u64 = 20;

/// The default per-mission timeout for the F12 `/curate` curation lane: 5 minutes.
/// A curation is one synchronous propose call (a whole candidate to reason over) —
/// longer than a single-block name, far shorter than a 30-minute build mission.
pub const DEFAULT_CURATION_TIMEOUT_SECS: u64 = 300;

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
    /// `build-runner` | `naming-runner` | `hand-runner` (the supported capabilities).
    /// `hand-runner` serves the F12 `/curate` propose-apply lane (§2). The remaining
    /// two (`loop-runner`/`review-runner`, §5e) are out of the MVP and refused here.
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
    /// Per-BLOCK timeout for the F11-b `/name` naming lane in seconds (default 20).
    /// Distinct from `timeout_secs` on purpose: naming is a synchronous per-block
    /// call, never a 30-minute mission.
    #[serde(default = "default_naming_timeout")]
    pub naming_timeout_secs: u64,
    /// Per-mission timeout for the F12 `/curate` curation lane in seconds (default
    /// 300). A curation is one synchronous propose call over a whole candidate —
    /// bounded here so a hung hand-runner is killed, never a held mission.
    #[serde(default = "default_curation_timeout")]
    pub curation_timeout_secs: u64,
}

fn default_timeout() -> u64 {
    DEFAULT_TIMEOUT_SECS
}

fn default_naming_timeout() -> u64 {
    DEFAULT_NAMING_TIMEOUT_SECS
}

fn default_curation_timeout() -> u64 {
    DEFAULT_CURATION_TIMEOUT_SECS
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

/// Map a supported capability string to the frozen [`Capability`]. `build-runner`
/// (the `/run` code lane), `naming-runner` (the `/name` lane) and `hand-runner`
/// (the F12 `/curate` propose-apply lane) are supported; `loop-runner` and
/// `review-runner` (§5e) remain out of the MVP and are refused. This is the F12
/// amendment: the `hand-runner` refusal is lifted for the curation shape ONLY —
/// `loop`/`review` stay refused byte-identically.
pub fn parse_capability(s: &str) -> Result<Capability, String> {
    match s.trim() {
        "build-runner" => Ok(Capability::BuildRunner),
        "naming-runner" => Ok(Capability::NamingRunner),
        "hand-runner" => Ok(Capability::HandRunner),
        other => Err(format!(
            "'{other}' is not a supported capability — pin `build-runner`, `naming-runner`, or `hand-runner` (loop/review-runner are out of the MVP, §5e)"
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

        let capability = match parse_capability(&r.capability) {
            Ok(c) => c,
            Err(detail) => return Err(field("capability", &detail)),
        };
        // The SYNCHRONOUS lanes — `naming-runner` (`/name`, F11-b) and `hand-runner`
        // (`/curate`, F12) — never spawn a mission: no worktree, no gate, no mission
        // letters, and the daemon never writes a store (it PROPOSES; the owner
        // applies). So the mission-lane requirements relax for both: the
        // `{packet_file}` token is OPTIONAL (the packet arrives on stdin; the token,
        // when present, is substituted with a temp packet file), and `gate_command`/
        // `workspace_allowlist` are OPTIONAL (they gate `/run` missions, which a pure
        // synchronous runner never serves — an empty allowlist refuses every `/run`).
        let is_synchronous = matches!(
            capability,
            Capability::NamingRunner | Capability::HandRunner
        );
        if r.command.is_empty() {
            return Err(field(
                "command",
                "must name the agent CLI (at least one element)",
            ));
        }
        if !is_synchronous && !r.command.iter().any(|a| a.contains(PACKET_FILE_TOKEN)) {
            return Err(field(
                "command",
                "must contain the `{packet_file}` token — the daemon writes the packet into the worktree and splices its path here",
            ));
        }
        if !is_synchronous && r.gate_command.is_empty() {
            return Err(field(
                "gate_command",
                "must name the gate the daemon runs after the agent exits (§5c)",
            ));
        }
        if !is_synchronous && r.workspace_allowlist.is_empty() {
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
        if r.naming_timeout_secs == 0 {
            return Err(field("naming_timeout_secs", "must be greater than zero"));
        }
        if r.curation_timeout_secs == 0 {
            return Err(field("curation_timeout_secs", "must be greater than zero"));
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

    if path.exists() {
        return read_valid_secret(&path);
    }

    use rand::RngCore;
    let mut bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();

    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let mut f = match opts.open(&path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            return read_valid_secret(&path)
        }
        Err(error) => return Err(error),
    };
    f.write_all(hex.as_bytes())?;
    f.sync_all()?;
    Ok(hex)
}

fn read_valid_secret(path: &Path) -> std::io::Result<String> {
    let metadata = std::fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "runnerd secret must be a regular non-symlink file",
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o077 != 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "runnerd secret is group/world accessible",
            ));
        }
    }
    let secret = std::fs::read_to_string(path)?.trim().to_string();
    if secret.len() != 64 || !secret.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "runnerd secret must contain exactly 32 random bytes encoded as hex",
        ));
    }
    Ok(secret)
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
    fn naming_runner_relaxations_and_per_block_timeout() {
        // F11-b: a MINIMAL naming runner — command only. The /name lane reads the
        // packet from stdin (no {packet_file}), runs no gate, and touches no
        // workspace, so those mission-lane fields are optional for this capability.
        let minimal = r#"
[[runner]]
id = "namer-min"
capability = "naming-runner"
command = ["my-namer"]
"#;
        let cfg = parse(minimal).expect("a minimal naming runner is valid (F11-b)");
        assert_eq!(
            cfg.runners[0].naming_timeout_secs, DEFAULT_NAMING_TIMEOUT_SECS,
            "the per-block naming timeout defaults to 20s"
        );
        assert!(cfg.runners[0].gate_command.is_empty());
        assert!(cfg.runners[0].workspace_allowlist.is_empty());

        // The relaxation NEVER reaches a build-runner: the mission-lane
        // requirements (token/gate/allowlist) still bind it.
        let build_no_token = r#"
[[runner]]
id = "b"
capability = "build-runner"
command = ["agent-cli"]
gate_command = ["t"]
workspace_allowlist = ["/abs"]
"#;
        assert!(
            parse(build_no_token).is_err(),
            "a build-runner still requires the packet_file token"
        );

        // A zero per-block naming timeout is refused honestly by field.
        let zero = r#"
[[runner]]
id = "namer-zero"
capability = "naming-runner"
command = ["my-namer"]
naming_timeout_secs = 0
"#;
        assert!(matches!(
            parse(zero).unwrap_err(),
            ConfigError::Field {
                field: "naming_timeout_secs",
                ..
            }
        ));

        // A naming runner with an allowlist still has its entries validated.
        let rel_allowlist = r#"
[[runner]]
id = "namer-rel"
capability = "naming-runner"
command = ["my-namer"]
workspace_allowlist = ["not/absolute"]
"#;
        assert!(matches!(
            parse(rel_allowlist).unwrap_err(),
            ConfigError::Field {
                field: "workspace_allowlist",
                ..
            }
        ));
    }

    #[test]
    fn hand_runner_parses_with_the_synchronous_relaxations_and_curation_timeout() {
        // F12: a hand-runner serves `/curate` — no worktree, no gate, no allowlist,
        // packet on stdin. So a MINIMAL hand-runner (command only) is valid, exactly
        // like a naming-runner, and its per-mission curation timeout defaults to 300.
        let minimal = r#"
[[runner]]
id = "hand-1"
capability = "hand-runner"
command = ["my-hand-cli"]
"#;
        let cfg = parse(minimal).expect("a minimal hand-runner is valid (F12)");
        assert_eq!(cfg.runners[0].parsed_capability(), Capability::HandRunner);
        assert_eq!(
            cfg.runners[0].curation_timeout_secs, DEFAULT_CURATION_TIMEOUT_SECS,
            "the per-mission curation timeout defaults to 300s"
        );
        assert!(cfg.runners[0].gate_command.is_empty());
        assert!(cfg.runners[0].workspace_allowlist.is_empty());

        // A zero curation timeout is refused honestly by field.
        let zero = r#"
[[runner]]
id = "hand-zero"
capability = "hand-runner"
command = ["my-hand-cli"]
curation_timeout_secs = 0
"#;
        assert!(matches!(
            parse(zero).unwrap_err(),
            ConfigError::Field {
                field: "curation_timeout_secs",
                ..
            }
        ));
    }

    #[test]
    fn loop_and_review_runners_stay_refused_byte_identically() {
        // The F12 amendment lifts the refusal for `hand-runner` ONLY — `loop-runner`
        // and `review-runner` remain out of the MVP (§5e), refused at the capability
        // field with the pin law's message unchanged for the two.
        for cap in ["loop-runner", "review-runner"] {
            let toml = format!(
                r#"
[[runner]]
id = "x"
capability = "{cap}"
command = ["c"]
"#
            );
            match parse(&toml).expect_err("loop/review are out of the MVP") {
                ConfigError::Field { field, detail, .. } => {
                    assert_eq!(field, "capability");
                    assert!(
                        detail.contains("loop/review-runner are out of the MVP"),
                        "the §5e refusal is intact: {detail}"
                    );
                }
                other => panic!("expected a capability field error, got {other}"),
            }
        }
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

    #[cfg(unix)]
    #[test]
    fn ensure_secret_refuses_symlink_and_insecure_existing_file() {
        use std::os::unix::fs::{symlink, PermissionsExt};

        let insecure = tempfile::tempdir().unwrap();
        let insecure_path = insecure.path().join(RUNNERD_SECRET_FILE);
        std::fs::write(&insecure_path, "a".repeat(64)).unwrap();
        std::fs::set_permissions(&insecure_path, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert!(ensure_secret(insecure.path()).is_err());

        let linked = tempfile::tempdir().unwrap();
        let target = linked.path().join("target");
        std::fs::write(&target, "b".repeat(64)).unwrap();
        std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o600)).unwrap();
        symlink(&target, linked.path().join(RUNNERD_SECRET_FILE)).unwrap();
        assert!(ensure_secret(linked.path()).is_err());
    }
}
