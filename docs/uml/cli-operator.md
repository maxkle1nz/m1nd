# cli-operator-surface

A two-layer front door: a Rust clap binary (`m1nd-mcp`) that selects one runtime mode (stdio MCP / `--serve` HTTP / `--attach` bridge) or runs one of three offline one-shot operator verbs, plus an npm installer/repair CLI (`m1nd` cli.js) that wires agent hosts to that binary and repairs stale bindings (restart/update/hosts/doctor/kickstart). The two layers meet ONLY through the on-disk runtime binary path and version strings — never in-process.

## Class/Component

```mermaid
flowchart TB
    subgraph Native["Rust binary — m1nd-mcp"]
        CLI["cli.rs — clap struct Cli :21<br/>LONG_VERSION = pkg + git_sha :12<br/>MedullaMigrateMode{Plan,Apply,Rollback} :148"]
        MAIN["main.rs — mode dispatcher :626"]
        SV["enforce_strict_version :602<br/>strict_version_verdict :565 (exit 2)"]

        subgraph Modes["Runtime modes (mutually exclusive)"]
            STDIO["run_stdio_server :493<br/>(default / --no-gui)"]
            HTTP["http_server::run (--serve :1337)"]
            ATTACH["attach_client::run_attach_client :487<br/>NO graph / NO engines / NO lease"]
        end

        subgraph Offline["Offline operator verbs (OFF the MCP surface)"]
            INBOX["run_inbox_sweep :227<br/>(field-report triage)"]
            MED["run_medulla_migrate :330<br/>plan|apply|rollback · CODE-LAND-ONLY"]
        end

        AC["attach_client.rs<br/>forward_with_reinit (-32001 replay)"]
        IR["instance_registry.rs<br/>discover_serve_owner :1575<br/>Q1 runtime_root · Q2 ingest coverage"]
    end

    subgraph Npm["npm — @maxkle1nz/m1nd (cli.js) — NOT the runtime"]
        NMAIN["main :3003"]
        HOSTS["hosts status :1151 / plan / apply<br/>22 hosts, --yes gate"]
        RESTART["restart :2690<br/>build→install→codesign→kickstart"]
        UPDATE["update check|plan|apply|verify|rollback :2536"]
        MAC["codesignAdHoc :2641<br/>managedLaunchdLabels :2662 (/m1nd/i)<br/>kickstartManagedServices :2679"]
        RT["defaultRuntimePath ~/.m1nd/bin/m1nd-mcp :214<br/>runtimeVersion :1780"]
    end

    CLI --> MAIN
    MAIN --> SV
    SV -->|pass| ATTACH
    MAIN --> Modes
    MAIN --> Offline
    ATTACH -.auto.-> IR
    ATTACH -.frames.-> AC
    MED -.plan/apply/rollback.-> MEDENG["medulla_migration.rs"]
    INBOX -.-> MAILBOX["mailbox.rs distribute/inbox_sweep"]
    NMAIN --> HOSTS
    NMAIN --> RESTART --> MAC
    NMAIN --> UPDATE
    RESTART --> RT
    RT -.on-disk binary path + --version string.-> CLI
```

## Sequence

```mermaid
sequenceDiagram
    participant Op as Operator / Host
    participant M as main.rs
    participant Cargo as cargo build
    participant CS as codesign
    participant LC as launchctl

    rect rgb(235,245,255)
    Note over Op,M: NATIVE mode dispatch (order is load-bearing)
    Op->>M: m1nd-mcp --stdio --no-gui
    M->>M: ensure_bwrap_compat_wrapper :628
    M->>M: enforce_strict_version :632 (exit 2 on mismatch)
    M->>M: Cli::parse
    alt --attach set
        M->>M: resolve base_url: env M1ND_ATTACH_URL > auto(discover) > literal
        M->>M: run_attach_client (returns BEFORE McpServer::new)
    else --inbox-sweep
        M->>M: boot SessionState → distribute → print m1nd-inbox-sweep-v0
    else --medulla-migrate apply/rollback
        Note over M: REFUSE (exit 2) without --migrate-project-root
        M->>M: plan/apply/rollback → print m1nd-medulla-migrate-v0
    else --serve
        M->>M: http_server::run
    else default
        M->>M: run_stdio_server (McpServer::serve, SIGINT-aware)
    end
    end

    rect rgb(255,245,235)
    Note over Op,LC: npm restart --binary --yes (macOS reload trio)
    Op->>Cargo: cargo build --release -p m1nd-mcp (if buildable)
    Op->>Op: installRuntimeBinary (copy to temp then rename)
    Op->>CS: codesignAdHoc — ad-hoc sign, MISSING = loud warn NOT fatal
    Note over CS: FINDING: unsigned copy is OS_REASON_CODESIGNING-killed by Gatekeeper
    Op->>LC: managedLaunchdLabels — parse launchctl list, filter /m1nd/i
    Op->>LC: kickstart -k gui uid label — SIGKILL then restart
    Note over LC: FINDING: kill -TERM fallback if kickstart missed KeepAlive races
    Op->>Op: append rebind next_actions (host_rebind_proven:false)
    end
```

## State/Flow

```mermaid
stateDiagram-v2
    [*] --> StrictGate
    StrictGate --> Refused: M1ND_STRICT_VERSION && expected != running (exit 2)
    StrictGate --> Parsed: pass (compile-time identity only)

    state ModeSelect <<choice>>
    Parsed --> ModeSelect
    ModeSelect --> AttachBridge: --attach (no graph/lease)
    ModeSelect --> InboxSweep: --inbox-sweep
    ModeSelect --> MedullaMigrate: --medulla-migrate
    ModeSelect --> HttpServe: --serve
    ModeSelect --> StdioServer: default

    state MedullaMigrate {
        [*] --> mm_plan
        mm_plan --> mm_apply: explicit --migrate-project-root
        mm_apply --> mm_idempotent: 2nd apply → already_migrated (0 moves)
        mm_apply --> mm_rollback: restore from .m5a-backup-*
        note right of mm_rollback
            [unverified] moved-list = *.light.md in project store
            (heuristic, no CLI-level test)
        end note
    }

    AttachBridge --> [*]
    InboxSweep --> [*]
    HttpServe --> [*]
    StdioServer --> [*]
```

## Invariantes
- `--attach` loads NO graph, builds NO engines, takes NO lease: the branch returns before `load_config_from_cli` and can never reach `McpServer::new` (confirmed main.rs :640-680, attach branch precedes config load).
- Strict-version refusal runs before any session/graph/lease, using only compile-time identity; refusal is exit code 2 (confirmed `strict_version_verdict` :565 returns Some(msg) on version_mismatch||sha_mismatch).
- `--read-only` (or truthy `M1ND_READ_ONLY`) always wins over a config file — force-OR'd onto `config.read_only`.
- `--medulla-migrate` apply/rollback REFUSE (exit 2) without explicit `--migrate-project-root`; destination brain is never derived from ambient binding (test `apply_without_explicit_project_root_is_refused`).
- `--medulla-migrate` is idempotent: a second apply moves zero claims and reports `already_migrated`.
- Relative persist targets anchor under `runtime_dir`; explicit absolute paths pass through untouched; the `temp` sentinel resolves to an OS-temp per-pid file (6 unit tests).
- `--attach auto` resolves only a ReadWrite AND `owner_live==Some(true)` AND `!stale` owner with a reachable URL; stdio-only owners publish no port and are skipped. Those four gates are shared by BOTH discovery questions (`is_attachable_serve_owner`): (1) runtime_root match, then (2) an owner whose declared roots (`workspace_root` + `ingest_roots.json`) COVER the caller root — canonical path identity, worktree resolved to its main repo, `>1` covering owner refuses naming all candidates, and the bearer token is read from the RESOLVED owner's runtime root, not the client's.
- npm restart/update mutate ONLY with `--yes`; check/plan/status are read-only dry-runs.
- launchd labels discovered GENERICALLY from `launchctl list` filtered `/m1nd/i` — never a hardcoded personal label (confirmed `managedLaunchdLabels` :2662-2671).
- A missing `codesign` tool is a loud warning appended to next_actions, never fatal (confirmed `codesignAdHoc` :2643 returns `{missing:true}` on ENOENT).
- Every npm operator command carries a `non_claims` list: does NOT prove a live host rebound / refresh a cached tool list / repair graph.
- `stopRuntimeProcesses` never kills its own pid; matches only `argv[0]` basename == `m1nd-mcp[.exe]`.

## Gaps
- **[high] `restart --yes` macOS reload trio is entirely untested**: `codesignAdHoc`/`managedLaunchdLabels`/`kickstartManagedServices` (confirmed present cli.js:2641/2662/2679) have ZERO coverage — all three `restart()` test calls use `--no-kill` or omit `--yes`, so the codesign/kickstart/kill branch never runs. The exact Gatekeeper/launchd incident it was built for cannot be regression-guarded.
- **[high] `--medulla-migrate rollback` has no CLI-level proof**: reachable + mutating, but no test invokes it. The moved-list is reconstructed heuristically (files ending `.light.md` in the project store, confirmed main.rs:435-445) and unverified end-to-end; `most_recent_backup` :477 also untested at the seam.
- **[medium] `--attach auto` owner-discovery CLI path untested at binary level**: `resolve_attach_auto` precedence (env>auto>literal) + runtime_root derivation; a misresolved runtime_root would silently bridge to the wrong owner or fail. Only `instance_registry`'s in-module tests exercise discovery.
- **[medium] `--inbox-sweep` has no binary-level test**: the multi-step `run_inbox_sweep` flow is only indirectly covered by the mailbox module tests.
- **[medium] crates.io↔npm dist-tag parity enforced only by a human-invoked skill** (`/release-parity`, `disable-model-invocation:true`), not CI — the historical npm-lags-crates.io (beta.8 vs beta.0) failure can recur if the operator forgets.
- **[low] `restart --yes` does not verify the swapped binary's version/sha before declaring done** (unlike `update apply`'s `version_verified` gate); `after_version` is read but not asserted against an expected target.
- **[low] `managedLaunchdLabels` matches ANY `/m1nd/i` label and kickstarts all of them**: a co-resident non-owner m1nd service would be force-restarted by an unrelated repair.
```
