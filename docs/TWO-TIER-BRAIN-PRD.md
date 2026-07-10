# The Two-Tier Brain — PRD

**Per-project brains + a shared medulla · the memory that travels with git (official, proof-grown)**

> **Status:** OFFICIAL — maintainer-approved direction (2026-07-03), formalized as the canonical PRD.
> **Provenance (three-seat Fable design, critique baked in):** an architect (Canonical Architecture v1, decisions Q1–Q6), a lifecycle/operations seat (decisions B1–B10, live-machine probes), and an adversarial critic whose **MANDATORY-FIX list and KILL list are BINDING in this PRD** — each is encoded inline and marked `[C-FIX n]` / `[KILLED]`, never appended as a caveat. Where the critic's steelman of the single-brain alternative won an axis, this PRD **adopts that outcome** and says so (§2).
> **Ground:** every `file:line` anchor below was verified at `origin/main` @ `f737931`; the repo HEAD this PRD lands on is `98b745a` (docs/brand assets only between the two — code anchors unaffected). **The symbol is the contract, the line is a hint — re-anchor at implementation start.** Live-machine numbers are from probes on the maintainer's machine, 2026-07-03 ~17:00, and are marked *(measured)*. Every effort figure not marked *(measured)* is an estimate written in words, never a precision bar.
> **Two live facts shaped this design and are part of its evidence:** (1) the medulla's persistence failure is **root-caused** — the launchd plist has no `WorkingDirectory`, so the owner runs with cwd=`/` (documentary proof: the live lease records `"workspace_root": "/"`, `"graph_source": "/graph_snapshot.json"`); relative-path persists fail with os error 30 while absolute-path writes succeed. (2) Today's 16:44 medulla restart **silently destroyed its own code graph** (6,221 nodes → ~124 memory nodes) and overwrote the 6.2 MB embedding cache down to 82 KB *(measured)*. The mixed single brain amputates itself on every restart; this PRD is the cure, demonstrated urgent the day it was written.
> **Amended 2026-07-03 (same day, evening): §9.5 First-Contact Reception — the front desk.** Maintainer direction verbatim in §9.5; triggered by field evidence that arrived hours after the PRD landed (an Antigravity session in `~/project-d` silently bound the `~/m1nd` brain — `field-reports.jsonl`, 21:39). Reception anchors verified at `origin/main` @ `6c53c47`.

---

## 1. Thesis

**A teammate clones the repo. Git delivered `brain.json`, `agent-memory/*.light.md`, `battery/` — and no snapshot. Their first session: the graph is empty, but the project's memory loads instantly, and `north` answers with the project's decisions and gotchas before m1nd has parsed a single line of code.** The clone knows *why* the code is the way it is before it knows the code. That is the compounding thesis made visceral, on day one, for free — and it costs a gitignore and an init command.

Two theses, one product:

1. **Product thesis — the memory is the brain; it travels with git.** One brain per repo owns that repo's code graph, agent-memory, and calibration; the memory clones with the repo, so every teammate and every agent inherits it. Exactly one **medulla** per maintainer/machine holds what is *not* any repo's: doctrine, preferences, cross-project findings.
2. **Engineering thesis — process-per-repo is the namespace mechanism.** Spawn-on-miss rides the *existing, tested* lease/registry/boot-GC machinery (`acquire_with_mode`, `instance_registry.rs:134-220`; stale-GC `:391-426`; PR #225 re-init `c797714`): zero new locking code, zero verb rewrites, isolation by OS construction. This — not any "structural impossibility" — is why two-tier is cheap where the single-brain alternative is expensive (§2).

The organizing law, stated with corrected precision `[C-FIX 8]`: **the wrong workspace binding becomes impossible via the default path** — there is no shared owner for project questions to land on. Overrides can still misbind (`M1ND_ATTACH_URL`, a stale `M1ND_RUNTIME_DIR` pin copied between repos, symlinked worktrees), which is exactly why the existing `workspace_binding_mismatch` guard (`session.rs:747-836`, live-proven against the served owner from `~/m1nd-l00p`) survives — demoted from front line to defense-in-depth backstop, never deleted. The reception protocol (§9.5) extends the law to the first moment of contact: **silent binding is legal only when the caller's resolved root matches the brain being bound** — every other first contact is offered a choice, never defaulted (TT-INV-12).

---

## 2. The single-brain steelman — verdict, stated explicitly

The adversarial critic built the strongest possible alternative — **SB-N: one resident owner + namespaces** (federate every active repo into today's single brain via the existing id-prefixing at `layer_handlers.rs:6050`; scope every `north`/`seek` by the `M1ND_WORKSPACE_ROOT` hint every host already sends; tag claims per project; evict cold namespaces). SB-N would delete five of this design's six net-new mechanisms and make cross-project code queries possible. The verdicts, adopted verbatim:

**REJECTED as the substrate — two-tier wins Axis 1, but for the honest reason, not the seats' original one.** SB-N's namespace scoping is not a filter bolted onto `north`; it is a rewrite of the substrate's core assumption. Every verb handler, the calibration state, the trust rows, plasticity, and the binding guard all assume ONE graph, ONE workspace, ONE calibration row per process. Making namespaces first-class means auditing and patching essentially the whole verb surface, with months of regression risk. **Process-per-repo delivers the identical guarantee with zero verb changes: the OS is the namespace implementation** — a reuse-first argument, because the codebase's single-graph assumption makes process isolation the cheapest correct namespace mechanism. Two more decisive merits: **blast radius** (proven live today — the 16:44 restart destroyed the mixed brain's graph and embeddings for *everything at once*; per-repo owners cap that loss at one repo), and **per-brain calibration is a mandate delivered for free** (`session.rs:1498` loads `calibration_state.json` from `runtime_root` — pointing `runtime_root` at `<repo>/.m1nd` *is* the implementation, where SB-N needs calibration schema surgery).

**ADOPTED from the steelman — four postures this PRD takes because the critic's steelman won them:**

1. **The medulla IS the retained single brain.** The whole design is honestly framed as: *single brain where global works; process-per-repo only where code graphs live.* This disarms the complexity-tax critique instead of pretending it away.
2. **Per-repo memory roots matter more than per-repo processes.** The compounding win ships with the gitignore + init slice alone; the sequencing reflects that (memory split lands before any federation machinery — §14).
3. **Single-pane operations.** `doctor` + the existing `list_instances` must render the fleet as one table from day one (Slice 2 gate), so N processes stay as inspectable as one.
4. **Cross-project code query is a declared NON-GOAL** — SB-N's genuine advantage, structurally forfeited here (the medulla is forbidden code; project graphs are isolated at read time). The escape hatch is named, not wished for: spin up a throwaway analysis brain and `federate`/`federate_auto` the repos into it — that mechanism exists today (`layer_handlers.rs:5972+`, `audit_handlers.rs:2088+`) and is exactly what it is for. Unstated, this forfeit becomes next quarter's regret; stated, it is a design boundary.

---

## 3. Decision register — every open question closed

Architect decisions Q1–Q6 and lifecycle decisions B1–B10, as amended by the binding review. Amendments are marked; nothing is silently rewritten.

| # | Question | DECISION (final) | Notes / amendment |
|---|---|---|---|
| Q1 | Who spawns project owners? | **The bridge self-spawns** on discovery miss. Not launchd-per-repo, not a supervisor. | The bridge is the only host-neutral seat; launchd is macOS-only. Crash recovery free: next bridge respawns; #225 covers mid-session restarts (see risk §21.14). |
| Q2 | Idle shutdown? | **Yes for project brains** — `--idle-exit-secs 1800`: persist snapshot → release lease → exit 0. **Never for the medulla** (launchd KeepAlive). | Nx-daemon/LSP precedent. `[KILLED: B4's wake-persist-exit special case — plain idle-exit already drains; the special case added code and guaranteed cold mornings.]` |
| Q3 | Federated-read shape | **End state: server-side** (project owner composes a read-only HTTP MCP call to the medulla inside `north`). **v1 ships the hook-side two-call compose instead** — the doctrine feed must not wait for an HTTP composition layer. | `[C-FIX 5 + Axis-4 adoption]` Server-side moves whole to V2, with a negative cache mandated (`[C-FIX 6]`). §10. |
| Q4 | `mcp-config`: embedded vs bridge | **Bridge model becomes the default emit** (`args: ["--attach","auto"]`); today's embedded `--stdio --no-gui` survives behind `--embedded` (CI/one-shot). | The live machine already proved the bridge model on all four hosts. |
| Q5 | Read-only-fs persist failure | **CLOSED — root cause isolated** (was honestly open in Architecture v1): plist lacks `WorkingDirectory` → cwd=`/` → relative-path persists fail (os error 30). Fix = plist key + absolute `--graph` + the durable code fix (persist targets resolve against `runtime_root`, never cwd). Pre-cutover blocker, Slice 0. | Documentary proof: live lease `workspace_root:"/"`. The Codex launcher never hit it because it `cd`s and passes an absolute `--graph` (`~/.codex/bin/m1nd-stdio-session.sh:23-27`). |
| Q6 | One registry or per-tier? | **One shared registry: `~/.m1nd/registry`**, achieved by **re-pointing** the medulla's `--registry-dir` — [KILLED: the S6 `runtimes/claude → ~/.m1nd/medulla` directory move + compat symlink — zero user value, real breakage risk]. `brain_kind` field keeps legacy entries parseable (serde default). | A project owner must *see* the medulla entry to resolve it without hardcoded addresses. |
| — | Naming | **Project Brain / Medulla / The Two-Tier Brain**; code identifier `brain_kind: project\|medulla`. | HUMAN-LAYER S5 collision resolved in §4.3. |
| — | Calibration in git? | **Never committed.** Per-brain AND per-machine (§5.1 rationale). | |
| — | Brain auto-creation? | **Never silent.** `m1nd init` is the only birth. `--attach auto` in a brainless repo routes to the medulla + honest gap. | Security stance §9.4. Reception (§9.5) *offers* the birth — the exact `m1nd init` command in `options[]` — and never executes it. |
| — | First contact | **The reception packet** — on (new session) AND (no binding chosen OR caller root ∉ the would-be brain): ONE structured packet, ONE round trip; the agent chooses; the choice is sticky and memorizable. Silent binding only on cwd match. | Maintainer direction 2026-07-03, verbatim in §9.5; field-triage: the Antigravity silent-bind (`field-reports.jsonl` 21:39). Slice 2R. |
| B1 | Brain present, graph empty | **Auto-warm:** boot memory-first, serve immediately, background-ingest the workspace root, wear `warming` honestly. `brain.json` (born only by explicit init) IS standing consent; ingest reads only the repo's own files. | **Amendment:** background ingests are capped machine-wide at **one at a time** (post-reboot CPU-storm guard) `[C-FIX, Axis 3]`. |
| B2 | `m1nd init` posture | **Foreground, with progress** — the one blocking moment in the system, human-invoked. Prints a birth certificate. | Gains the loud commit warning + `--private` `[C-FIX 1]` and the `$HOME` refusal `[C-FIX 9]`. |
| B3 | Field telemetry | **Global mailbox stays** — one file, `~/.m1nd/field-reports.jsonl`; reports gain a `"brain"` field. | Argued in §19. |
| B4 | Idle clock & sleep | Idle = monotonic time since last tool call. | `[KILLED: the wake-persist-exit special case.]` Honest consequence: owners that survive sleep give a *warm* morning; owners that idle-exited give a doctrine-only first beat (§10). |
| B5 | Upgrade choreography | **Medulla first (kickstart), project brains converge lazily on respawn.** No fleet orchestrator. | `doctor --bounce` (immediate convergence) moves to V2 `[KILL list]`; v1 immediate convergence = `m1nd brain stop` + next call. |
| B6 | Removal surface | Ladder `stop → clean → eject` on the `delete_instance_state` file set (`instance_registry.rs:348-362`). | **v1 ships `stop` only**; `clean`/`eject` are V2 `[KILL list]`. |
| B7 | Wedged owner | The ONE failure that asks a human: **doctor prescribes the verified `kill -TERM`, never executes it silently.** | §17 row 2. |
| B8 | Swarm roster | `[KILLED for v1 — no slice, no gate, no RED: by the maintainer's own doctrine it is a hypothesis, not a feature.]` Retained as a V2 hypothesis with its own slice+gate before it is real. | §15. |
| B9 | Degradation grammar | **Exactly ONE `fix` per degraded envelope**, routed through the existing `recovery_playbook`. A menu is a failure of nerve. | Invariant TT-INV-4. |
| B10 | Legacy debris | Reclaimed only in the LAST migration step, after soak: `_expired/` 145 MB *(measured)*, 20 hex per-session runtimes, binary `.bak`s (~290 MB). Nothing before proof. | §16 M8. |

**Net-new surface, complete list (everything else is composition):** (a) bridge walk-up + spawn-on-miss; (b) `--port 0` + real-port readback via the existing `set_running_endpoint`; (c) the hook-side two-call composed north (v1) / the server-side federated beat (V2); (d) `M1ND_RUNTIME_DIR`/attach-auto emit in `mcpServerEntry` + the selective `.m1nd/.gitignore` written by `init`; (e) `brain_kind` verb gate + registry field, `M1ND_AGENT_ID`; (f) **memory hygiene: the secret-scan in the `memorize` write path + the conflict-marker guard** `[C-FIX 1, 2 — mandated by review]`; (g) V2: `memorize --promote`; (h) **reception (§9.5): bridge first-contact detection + the `M1nd-Caller-Root` forward + the owner-side packet composer + the `bind` answer verb + optional last-known `node_count`/`edge_count` registry fields** (same serde-default posture as `brain_kind`).

---

## 4. Topology & names

### 4.1 The two tiers

- **PROJECT BRAIN** — one per repo, owns exactly one thing: *this repo*. Runtime lives in `<repo>/.m1nd/`. Holds the code graph, the project's agent-memory (decisions/gotchas), and its **own** calibration state. Spawned on demand, idle-exits, ephemeral port, lease-guarded. **The memory travels with git.**
- **MEDULLA** — exactly one per maintainer/machine-fleet. Runtime stays physically at `~/.m1nd/runtimes/claude/` (`[KILLED: the directory move]`) and is *logically* the medulla, carried by `brain_kind:"medulla"` in its `brain.json` and registry entry. Holds doctrine, maintainer preferences, cross-project findings. **Code graphs are forbidden and verb-gated.** Pinned `:1338`, launchd-kept — and it is honestly framed as **the retained single brain** (§2, adopted posture 1).

### 4.2 One diagram

```mermaid
flowchart TD
    subgraph HOSTS["any MCP host — claude · codex · gemini · antigravity"]
        H1["session"]
    end
    H1 -->|"stdio MCP — ONE global config per host"| BR
    BR["BRIDGE — m1nd-mcp --attach auto<br/>walk up cwd to .m1nd/brain.json<br/>registry hit: attach · miss: spawn · no brain: medulla<br/>~6 MB RSS (measured)"]
    subgraph TIER1["PROJECT BRAINS — one per repo, spawn-on-demand"]
        PA["repo A — A/.m1nd/<br/>code graph · project memory (git-tracked)<br/>own calibration · ephemeral port · idle-exit"]
        PB["repo B — B/.m1nd/<br/>same shape, fully isolated"]
    end
    MED["MEDULLA — one per maintainer/machine<br/>doctrine · preferences · cross-project findings<br/>code graphs FORBIDDEN (verb gate)<br/>pinned :1338 · launchd KeepAlive"]
    BR -->|"attach — HTTP MCP, no lease"| PA
    BR -->|"attach"| PB
    BR -->|"no brain anywhere: attach + honest gap"| MED
    BR -.->|"doctrine beat — v1 hook-composed, ≤1 s, fail-open"| MED
    PA -.->|"V2: server-side federated read (deferred)"| MED
    PA ==>|"PROMOTION — upward only, provenance-stamped<br/>(v1 manual · V2 --promote)"| MED
    REG[("~/.m1nd/registry — ONE shared phonebook<br/>lease · heartbeat · brain_kind")]
    BR --- REG
    PA --- REG
    PB --- REG
    MED --- REG
```

### 4.3 The "Project Brain (S5)" naming collision — resolved

`docs/HUMAN-LAYER-PRD.md` §4 names S5 "Project Brain — read the shared memory (read-only)": a **UI surface**. Reconciliation, one sentence of doc change (scheduled in Slice 6, not smuggled into this commit): **S5 becomes the "Project Brain Panel" — the human's read-only window onto a project brain's agent-memory.** The runtime tier owns the bare name; the panel is its view — semantically exact, since S5 renders precisely what the per-repo tier holds.

---

## 5. Project Brain subsystem

### 5.1 Runtime layout in `<repo>/.m1nd/` — committed vs ignored, with rationale

Policy law: **memory is the brain; caches are the metabolism. The brain travels with git; metabolism is per-machine.** The repo's own `.gitignore:48-54` already encodes this in miniature (track `agent-memory/*.light.md`, ignore `.history/`/`.locks/`) — the policy below is that precedent, generalized.

*(Pointer, 2026-07-05: the project's curated handoff document — PATHOS as a verified soul with anchored claim states, a freshness receipt, and a curator at the doc-gate — rides this same committed class and is spec'd whole at `docs/SOUL-PRD.md`; its write half mounts on the M5a→M6 medulla ladder, its read-only verifier (S0) composes shipped organs.)*

| Path in `<repo>/.m1nd/` | Git | Rationale |
|---|---|---|
| `brain.json` | **COMMIT** | Brain identity manifest (schema §5.2). **Data only — never executable paths** (§9.4). |
| `agent-memory/*.light.md` | **COMMIT** | The irreplaceable asset — project decisions/gotchas. This IS the compounding thesis. *(Unless `--private` — §5.2.)* |
| `battery/` | **COMMIT** | Proof-grown doctrine: this brain's battery cases are proof assets; they travel like memory (CI wiring is V2). |
| `.gitignore` | **COMMIT** | The selective policy itself, written by `m1nd init`. |
| `agent-memory/.history/`, `.locks/` | ignore | Existing precedent verbatim (`.gitignore:48-54`): per-machine supersession/audit runtime. **Honest consequence — zombie claims:** supersession chains fork silently across machines (§7.3). |
| `graph_snapshot.json` | ignore | Rebuildable via ingest; carries machine-absolute paths; matches the existing root rule (`.gitignore:23`). |
| `embeddings_cache.bin`, `*.tmp` | ignore | Content-addressed, runtime-regenerated (`.gitignore:28-31`); only the writable owner persists it (`session.rs:1409-1413`). |
| `calibration_state.json` | **ignore — decided** | Calibration is the brain's *proprioception on this body*: τ measured against THIS machine's prediction history. Committing it imports someone else's error history as your ground truth — it would falsify honesty, the one thing m1nd must never do. Re-arms cheaply via `calibrate_predict`. "Per brain" means *not shared across brains*; it also must not be shared across bodies. |
| `plasticity_state.json`, `antibodies.json`, `tremor_state.json`, `trust_state.json`, `savings_state.json`, `boot_memory_state.json`, `daemon_*.json`, `ingest_roots.json`, `auto_ingest_state.json`, `document_cache.json`, `cache_index.json`, `xray.ledger.jsonl` | ignore | The full runtime set enumerated by `delete_instance_state` (`instance_registry.rs:348-362`); `ingest_roots.json` carries absolute paths. Durable knowledge in `boot_memory` earns git travel by being **memorized** into a `.light.md`, never by committing the KV file. |
| `agent-pack/` | ignore | Regenerable, binary-versioned (`m1nd install-skills`); today's blanket `.m1nd/` rule (`.gitignore:46`) existed for exactly this. |
| `logs/` | ignore | Spawn/serve logs. |

In the m1nd repo itself, blanket line `.gitignore:46` (`.m1nd/`) is **replaced** by this selective block — otherwise the m1nd repo's own project-brain memory could never travel (Slice 3 RED proves the blanket swallows it).

### 5.2 `m1nd init` — the birth certificate (the ONE foreground moment)

`m1nd init` (existing command, `npm/lib/cli.js:26`) becomes the brain's birth: writes `brain.json` + the selective `.m1nd/.gitignore`, runs the first ingest **foreground with progress** (B2 — embedding dominates first-ingest wall time; the exact number is a **Slice 3 gate measurement, not a claim**), persists the first snapshot (every future spawn is warm), and prints the birth certificate.

```json
{
  "schema": "m1nd-brain-v0",
  "brain_kind": "project",
  "name": "<repo basename>",
  "created_ts": 1751558400000,
  "expected_sha": "f737931",
  "strict_binary": false,
  "promotion": { "default": "propose" }
}
```

Binding rules `[C-FIX 1, 9]`:

- **The loud warning is part of the ceremony:** `agent-memory/ will be COMMITTED — treat it like code you would push. Secrets in claims are refused at write time (§7.1), but review before pushing a public repo.`
- **`--private` mode:** adds `agent-memory/` to the ignore set for repos whose memory must not travel (public repos, sensitive infra). The trade is stated: no memory-before-code clone for this repo.
- **`init` refuses to run at `$HOME`** (a home-brain would try to ingest the maintainer's world), and the walk-up (§9.1) excludes `$HOME` itself for the same reason.
- `brain.json` is **inert data** — no binary paths, no hooks, no auto-exec directives (§9.4).

### 5.3 Owner lifecycle

**Spawn-on-demand (bridge-side, Q1).** On discovery miss with `brain.json` present, the bridge spawns a detached owner:

```
m1nd-mcp --serve --no-gui --port 0 \
         --runtime-dir <repo>/.m1nd \
         --graph <repo>/.m1nd/graph_snapshot.json \
         --idle-exit-secs 1800
env: M1ND_EXPECTED_SHA=<brain.json .expected_sha>, M1ND_WORKSPACE_ROOT=<repo>
```

- **Binary:** always `std::env::current_exe()` — the same binary the bridge runs. Never a path from `brain.json` (§9.4).
- **Session independence `[C-FIX 4]`:** the spawn is fully detached (own session — setsid/double-fork discipline); **the owner must outlive the bridge and the host session that birthed it.** Gated in Slice 2 (orphan-survival gate) — without this, every session-end would kill a brain mid-write.
- **Port (net-new, small):** `--port 0` = OS-assigned ephemeral; the owner reports the *actual* bound port through the existing `set_running_endpoint` registry write (the mechanism `entry_base_url` documents, `instance_registry.rs:645-650`). Deletes the port-collision class for the project tier while keeping TCP (the bridge is HTTP/SSE). The medulla alone keeps a pinned port.
- **Single-spawn race protection: zero new code.** Both racers exec; the per-`runtime_root` exclusive PID+heartbeat lease (`acquire_with_mode`, `instance_registry.rs:134-220`; proven by `rejects_live_runtime_root_collision_for_foreign_owner` and the read-only coexistence tests at `:725`/`:881`) lets exactly one win; the loser exits 0 "already owned"; the bridge retries discovery (250 ms × 20 ≤ 5 s) until the winner publishes `bind+port`, then attaches. **The bridge never blocks a session on this** — spawn-and-return `[C-FIX 3]`.
- **Warm boot:** snapshot-boot from `graph_snapshot.json` (persisted at idle-exit/SIGTERM/post-ingest). The ≤2.5 s attachable-after-spawn figure is a **budget with a Slice 3 gate, not a claim** `[C-FIX 3]`. Cold boot without a snapshot: boot memory-first (proven live: agent-memory loads inside the boot second), serve immediately, **auto-warm** in the background (B1, machine-wide cap: one background ingest at a time), report `needs_ingest`/`warming` honestly — never block a session.
- **Idle-exit (Q2):** 30 min (monotonic since last tool call) → persist snapshot + embeddings + calibration → release lease → exit 0. Discovery then misses cleanly; the next bridge respawns warm. Crash path: heartbeat stops → entry stale in 30 s (`STALE_AFTER_MS`) → boot GC (`spawn_boot_gc`, `:391-426`) sweeps. Gated in Slice 2 (persist-on-idle AND persist-on-SIGTERM, `--idle-exit-secs 2` in test) `[C-FIX 4]`.
- **Per-brain calibration: free.** `session.rs:1498` loads `calibration_state.json` from `runtime_root` — pointing `runtime_root` at `<repo>/.m1nd` *is* the implementation.
- **Per-brain drift guard: free.** `brain.json.expected_sha` → env → `enforce_strict_version` (`main.rs:293-322`); drift surfaces in every `north.binding.fingerprint` (live-proven: `binary_lags_repo:true`). Advisory default: `strict_binary:false` = spawn + surface drift; `true` = refuse to spawn (fail-closed, never redirect) — after an upgrade, strict brains demand a deliberate, reviewed re-pin of `expected_sha`, committed like code.
- **`m1nd brain stop [path]` (v1):** resolve `runtime_root` → registry → PID → SIGTERM (persists on the way down), lease released. RAM freed; nothing deleted; the next call resurrects warm. (`clean`/`eject`: V2, §20.)

### 5.4 The first hour, lived (compressed; the contracts above make it true)

1. **Brainless repo:** SessionStart hook → bridge walks up, no brain → medulla attach → one calm line: doctrine + `project_brain_absent — fix: m1nd init`. The host offers; the user accepts.
2. **`m1nd init`:** foreground ingest with progress → `BRAIN BORN: <name> · N nodes · memory travels with git` → `git add .m1nd && git commit` (as the repo's configured author — never as an AI identity).
3. **Fresh clone of a brained repo:** memory-before-code (§1) — claims served at `node_count == 0`, `warming` worn, graph fills in the background, second session warm. **This is Slice 3's formal gate**, not marketing.
4. **Second host joins (same repo, same minute):** its bridge walks up to the same `brain.json` → registry HIT → attaches the **same owner process**. One brain, many hands; provenance separates them (§11). Bridges cost ~6 MB RSS *(measured — three live now)*.

**Morning honesty `[C-FIX 3]`:** the first touch of a repo is **warm iff its owner survived** (still resident, or snapshot-boot completes inside the attach window) and **doctrine-only + `warming` when it didn't**. Anchors arrive on the second beat. This is stated here, in the failure table, and in the hook's own output — never styled away.

---

## 6. Medulla subsystem

**Belongs (all as `.light.md` claims + boot-memory KV):** operating doctrine (proof standard, git identity, model routing); maintainer preferences; cross-project findings (patterns confirmed in ≥2 repos); distilled field-telemetry learnings about m1nd behavior; host/tool runbooks. The live claim set (8 today *(measured)*, growing daily) is triaged at migration time from a live enumeration — never from this document (§16 M4).

**FORBIDDEN — structurally, not aspirationally:** code graphs, ingest roots, code snapshots, single-repo gotchas, secrets. Enforcement (net-new, small): `brain_kind:"medulla"` makes the owner **refuse `ingest`, `federate`, `federate_auto`, `auto_ingest_*`** with a typed error naming the project brain as the right destination. The medulla cannot be re-polluted by an eager agent. (The promotion write path is the single deliberate crossing — §8.)

**Address & discovery:** physical path unchanged (`~/.m1nd/runtimes/claude/`) `[KILLED: the move]`; pinned `:1338`; launchd `com.m1nd.serve` re-scoped medulla-only **after the Slice 0 persistence fix**, with `WorkingDirectory` set and an absolute `--graph`. Consumers resolve the medulla **via the unified registry** (`brain_kind=="medulla"` entry; bridge keyword `--attach medulla`), falling back to the literal `:1338` — no more hardcoded-only addresses in hooks. Machines without launchd get a project-only experience + `medulla_unreachable` gap; `m1nd doctor` prescribes the service install.

**Size discipline — the medulla is an index, not a warehouse:** soft cap 200 active claims (doctor warns at 300); no code embeddings; a periodic consolidation pass merges/supersedes; any claim that merely restates CLAUDE.md doctrine is pruned to a pointer (the maintainer's DRY rule). Small enough that the doctrine beat is always ≤1 s.

**Known steady state:** a pure memory-only medulla weighs **~170 MB RSS** *(measured this hour — the post-purge weight is known in advance because the 16:44 restart already produced it)*.

---

## 7. Memory hygiene — the mandated subsystem `[C-FIX 1, 2]`

The review's single incident-grade finding: **committed memory without a write-path guard is a leak machine.** The false-coverage claim is retired — the gitleaks PostToolUse hook matches `Edit|Write|MultiEdit` and extracts `tool_input.file_path`; `mcp__m1nd__memorize` never matches, the file is written *inside the m1nd-mcp process*, invisible to any host hook; Codex/Gemini/Antigravity have no such hook at all; and m1nd's own write path has zero scanning *(probed at f737931)*. Under today's `~/.m1nd` layout this was privately inert; the moment memory moves into the repo, it must be closed **before** the gitignore flips (Slice 4 lands before Slice 6 cutover; the init warning lands with Slice 3).

### 7.1 Secret-scan inside the write path

Host-independent, inside `memorize` (and the future `--promote`): scan the claim text + evidence excerpts against a secret-pattern corpus (connection strings with credentials, cloud keys, bearer/OAuth tokens, private-key headers). On hit → **typed refusal naming the pattern class; nothing is written.** The agent is told to redact and re-memorize. Honesty about the mechanism: pattern-based scanning has false negatives (risk §21.2) — this is a floor, not a proof; `--private` and the init warning are the defense-in-depth layers.

### 7.2 Conflict-marker guard at ingest & recall

The ingest walker reads `*.light.md` wholesale (`tools.rs:374-408`) — a git conflict marker inside a claim would be ingested as memory content and served in `north`: poisoned orientation. Guard (net-new, small): claims containing conflict-marker lines (`<<<<<<<`/`=======`/`>>>>>>>` at line start) are **refused at ingest and flagged at recall — never served in north**; the flag names the file and the one fix (resolve the merge).

### 7.3 The merge policy — one policy, stated once

- **New claims are conflict-resistant by construction:** one claim per file; parallel additions merge cleanly.
- **Supersession is merge-hostile:** the L1GHT rewrite-supersession rewrites the same file in place — two branches superseding one claim = a text conflict. Policy: **prefer keeping both claims live and `learn wrong` the loser** after merge; a text conflict is resolved by the human/agent doing the merge, protected by §7.2 from silent poisoning.
- **`.history/` does not travel** (per-machine, gitignored — existing precedent). Honest consequence, named: **supersession audit chains fork across machines, and machine B can merge-resurrect a claim machine A superseded (zombie claims).** Mitigation: the conflict-marker guard + `learn wrong` on sight; full cross-machine supersession sync is an **open problem** (§21.3), not a hidden one.
- **Eject never purges history:** a secret that reached a committed claim is a git-history-scrub job plus credential rotation — never an eject (§20).

---

## 8. Promotion & supersession across tiers

**Direction of flow:** knowledge is *promoted up* (project → medulla, by copy-with-provenance); doctrine *flows down* only by read at north-time — never copied into project brains.

**Promotion rules — a claim qualifies when BOTH hold:**

- **P1 — repo-agnostic when restated:** names no single repo's file/line/flag.
- **P2 — one of:** observed in ≥2 projects · maintainer doctrine/preference explicitly stated · distilled field-telemetry finding about m1nd itself.

**v1 mechanic — manual, documented `[KILL list: Slice 6 mechanization deferred]`:** the orchestrator (or maintainer) runs `memorize` **against the medulla** (attach `--attach medulla`) with the provenance fields written into the claim by convention — `origin_brain: <repo>`, `origin_claim: <slug>`, `promoted_by: <agent_id>`, `ts` — and marks the project-side claim `promoted_to: medulla@<slug>`. The project copy **remains in place**: it is the local witness; promotion elevates, never moves. *(Pointer, 2026-07-05: the mechanized `promote` verb, the full claim state machine, and the audit grammar are spec'd at `docs/MEDULLA-PRD.md` §3/§7, slice M6.)*

**V2 mechanic — `memorize --promote`:** the project owner writes the claim to the medulla over the same HTTP client as the federated read, stamping the same provenance automatically; a maker `agent_id` gets a typed refusal. No inbox/queue machinery (reuse-first): direct write + provenance + the consolidation pass demoting junk; `learn wrong` on the medulla kills a bad promotion instantly without touching the project witness.

**Who decides — stated with corrected precision `[C-FIX 8]`:** any agent proposes; only `*:orchestrator` or `human:founder` promote. **This is etiquette enforced by provenance audit, NOT a security boundary** — `agent_id` is self-declared, and any agent that attaches the medulla directly can `memorize` without `--promote`. Provenance makes violations auditable; delegation packets instruct makers accordingly; nothing stronger is claimed.

**Supersession & conflict resolution:**

- Within a tier: the existing L1GHT rewrite-supersession stands (prior belief → `.history/`, `State: outdated`), plus the §7.3 merge policy.
- Across tiers: **no tier ever edits the other's claims.** Conflicts are *composed and labeled*, never silently resolved. The composed north marks conflicting pairs and ranks by this table (mirroring the maintainer's truth hierarchy — code > PATHOS > memory — and doctrine precedence):

| Conflict class | Winner | Note |
|---|---|---|
| Fact about repo X | **Project brain X** | It sits closest to the code; the code itself is rank 0. |
| Cross-project fact / maintainer preference | **Medulla** | |
| Operating doctrine | **Medulla by default**; the repo's own doctrine claim wins *inside that repo* — **unless** the medulla claim is flagged `absolute` (maintainer-set) | Exactly CLAUDE.md's precedence: ABSOLUTE-global > repo doctrine > general global. |
| Any of the above vs live code/git | **Reality** | Stale memory gets `learn wrong`/superseded in the same session. |

---

## 9. Discovery & routing subsystem

### 9.1 Bridge-side resolution — host configs stay global

Precedence (first hit wins):

1. **`M1ND_ATTACH_URL`** — absolute override (exists, `cli.rs:88-99`).
2. **`M1ND_RUNTIME_DIR` / `--runtime-dir`** — explicit pin: that path is the `runtime_root`, no walk.
3. **Walk-up (net-new):** starting dir = `M1ND_WORKSPACE_ROOT` → host env candidates (`WORKSPACE_ROOT_ENV_CANDIDATES`, `session.rs:475-490`) → `cwd`. Ascend to the nearest ancestor containing `.m1nd/brain.json`; stop below `$HOME` and fs root — **`$HOME` itself is excluded** `[C-FIX 9]`. Nested brains: **nearest wins** (LSP-root precedent; the `nested_workspace_binding` guard stays as backstop). Found → `runtime_root = <that>/.m1nd`.
4. **Registry discovery:** `discover_serve_owner_base_url(runtime_root, registry)` (`instance_registry.rs:673-713`) — unchanged; exact canonicalized match, read-only, no lease. Hit → attach.
5. **Miss + brain exists → spawn** (§5.3), bounded retry, attach — spawn-and-return, never block.
6. **No brain anywhere → medulla:** registry entry with `brain_kind=="medulla"` → fallback literal `http://127.0.0.1:1338` → attach; the session runs medulla-only and north carries `project_brain_absent` + `fix: m1nd init`. **Never auto-create a brain.**
7. **Spawn fails / all down →** degrade to medulla if reachable (`project_brain_unavailable` gap); else ONE typed error routed through `recovery_playbook`. Hooks stay fail-open regardless.

```mermaid
flowchart TD
    S["bridge start: m1nd-mcp --attach auto"] --> E1{"M1ND_ATTACH_URL set?"}
    E1 -->|yes| A1["attach that URL — absolute override"]
    E1 -->|no| E2{"M1ND_RUNTIME_DIR pin?"}
    E2 -->|yes| D0["runtime_root = pin — no walk"]
    E2 -->|no| W["walk up: M1ND_WORKSPACE_ROOT → host env → cwd<br/>nearest ancestor with .m1nd/brain.json<br/>$HOME itself excluded"]
    W -->|found| D0
    W -->|none| M0{"registry: brain_kind == medulla?"}
    D0 --> R{"registry discovery<br/>discover_serve_owner_base_url"}
    R -->|hit| AT["attach owner"]
    R -->|"miss + brain.json"| SP["spawn detached owner — port 0, lease-arbitrated<br/>retry discovery 250 ms × 20 · spawn-and-return"]
    SP -->|"winner publishes bind+port"| AT
    SP -->|"retries exhausted"| M0
    M0 -->|yes| AM["attach medulla<br/>gap: project_brain_absent · fix: m1nd init"]
    M0 -->|no| ML{"literal 127.0.0.1:1338 up?"}
    ML -->|yes| AM
    ML -->|no| ERR["ONE typed error via recovery_playbook<br/>hooks stay fail-open — exit 0"]
```

### 9.2 Registry schema evolution

`InstanceRegistryEntry` (`instance_registry.rs:19-39`) gains **one optional field**: `brain_kind: "project"|"medulla"` (serde default `"project"` — legacy files keep parsing). Everything else — lease semantics, staleness, GC, conflicts — unchanged. **One registry dir for both tiers** (Q6): `~/.m1nd/registry`, achieved by re-pointing the medulla's `--registry-dir`; the live `registry-claude` split is consolidated at migration. **Scale correction, inline:** the earlier "~54k legacy entries" figure was stale; the live registry holds **143 instance files / 10 leases** *(measured)* — scale claims are re-measured at gate time, never inherited (TT-INV-11).

### 9.3 Config carriers

Global host configs carry only `--attach auto` (works from any cwd) + `M1ND_WORKSPACE_ROOT` + `M1ND_AGENT_ID`. Project-scoped configs written by `m1nd mcp-config --project` *additionally* pin `M1ND_RUNTIME_DIR=<proj>/.m1nd` — an explicit short-circuit of the walk (worktree/symlink edge cases). Env pin > walk, per §9.1. `--embedded` keeps today's `--stdio --no-gui` form (CI/one-shot).

### 9.4 Security stance (binding)

Cloning a repo must never grant it execution. Therefore: `brain.json` is **inert data** — no binary paths, no hooks, no auto-exec directives; spawn always execs the user-installed `current_exe()`; `expected_sha` is a *pin that can only refuse*, never redirect; `init` is the only birth and refuses at `$HOME`; the only consequence of a cloned `.m1nd/` is that ingest reads the repo's own files — which any session does anyway. The committed-memory privacy stance lives in §7 and §20.

### 9.5 First-Contact Reception — the front desk (amendment, 2026-07-03)

> **The question this answers:** on the first message of a session, shouldn't m1nd show the agent its options — create a new instance or attach to an existing project by searching among the active ones — and hand the agent all the information it needs to choose and act?
>
> **Field evidence (the RED, live):** hours after this PRD landed, an Antigravity session opened in `~/project-d` attached the `~/m1nd` brain through the served owner **in silence** — `health` answered ok, nothing flagged cwd ≠ bound workspace, and the maintainer needed three questions to understand what it was talking to (`~/.m1nd/field-reports.jsonl`, 2026-07-03T21:39). §9.1 routes correctly *when the two-tier topology exists*; reception is the missing conversational face of that routing — and its degraded mode (§9.5.5) kills this exact failure **before** any two-tier slice lands.

**The law (TT-INV-12):** silent binding is legal only when the caller's resolved root matches the brain being bound. Every other first contact gets the front desk: **ONE structured packet, ONE round trip** — the system hands the agent everything it needs to choose, the agent chooses, the choice sticks. Never an interrogation, never a silent default.

#### 9.5.1 The packet — `schema: m1nd-reception-v0`

Agent-first: every field machine-readable, every option carrying the exact call that executes it. Composed by the owner the bridge landed on (a cheap registry read + its own live state), returned as the result of the session's first tool call when the trigger holds (§9.5.2).

| Field | Contents | Source (built / unbuilt) |
|---|---|---|
| `where_you_are` | `caller_root` + `resolved_via` (`env:M1ND_WORKSPACE_ROOT` \| `bridge_cwd` \| `roots/list`) + **match verdict**: `bound_match` \| `known_brain_elsewhere` (the Antigravity case: you are *here*, the brain you'd get is *there*) \| `unknown_repo` (virgin — no brain anywhere on the walk) \| `ambiguous` (>1 candidate claims the root: nested brains on the ancestor chain, monorepo subdir with its own brain, worktree/symlink alias) + the evidence paths behind the verdict | Resolution machinery **built** (env candidates `session.rs:393-421`, walk-up = Slice 2); the verdict classifier reuses the mismatch guard's path comparison (`session.rs:747-836`) lifted from per-call opt-in to first-contact default — **unbuilt** |
| `what_exists` | The brain registry, one row per known brain: `workspace_root`, `runtime_root`, `name` (from `brain.json`), `brain_kind`, liveness (`live{pid, port, attached_sessions}` \| `dormant{last_heartbeat_age_s, snapshot{mtime, bytes}}`), `node_count`/`edge_count`, freshness (last ingest/tick age), `calibration_armed`. **"Known" = present in the registry (live or last-seen) or on the caller's walk-up path** — the registry is the machine's only brain index; a brain it never saw is invisible to reception, said in the packet, and no filesystem-wide `brain.json` hunt is ever attempted. | Enumeration **built** (`list_instances`, `instance_registry.rs:281`, read-only, no lease). Per-brain `attached_sessions` + `query_count` + `calibration_armed` **BUILT 2026-07-05 (ladder R14)** — the `/api/instances` listing enriches each entry from the brain's own warm `SessionState` (`http_server.rs::instances_listing`), partitioned on the session's bound brain, not the owner-global total. A dormant brain has no live wire sessions, so those live fields are **absent, never faked** (TT-INV-2); dormant `node_count`/`edge_count` still come from the last-published manifest. |
| `options[]` | Each `{id, call, consequence}` — the exact string to execute: **`bind_existing(brain)`** → MCP `bind {brain: <runtime_root>}`; **`create_project_brain(cwd)`** → shell `m1nd init` *(the ONE option that is a command, not an MCP call — birth stays foreground, consented, and the only birth, §3/§5.2; reception offers, never executes)*; **`medulla_only`** → MCP `bind {brain: "medulla"}` (proceed wearing `project_brain_absent`) | `bind` verb **unbuilt** (§9.5.2); `m1nd init` **built** as CLI, clone-gate semantics = Slice 3 |
| `suggested` + `why` | Exactly one suggestion with its reason. v1 rule-base: `bound_match` → bind (silent, no packet); `unknown_repo` → offer create; **mismatch → NEVER auto-bind** (the Antigravity rule — the packet exists *because* the default is illegal here). V2: the composer consults medulla policy claims first (§9.5.2 learning). | Rule-base **unbuilt**, trivial; policy consult **V2** |
| `honest_gaps` | What reception itself does not know: `stale_brain` (snapshot/heartbeat old — age stated), `calibration_unarmed` (verdicts will cap at `reverify`), `warming`, `persistence_degraded`, `medulla_policy_unread` (v1 always carries this — §9.5.2), `relevant_medulla_memories` (medulla-only claims that mention this repo — **V2**, needs the federated-read machinery §10.2), `caller_root_unknown` (direct-HTTP callers, §9.5.4) | Gap grammar **built** (TT-INV-2 vocabulary); items compose from existing state |

#### 9.5.2 Trigger, carrier, stickiness, the answer, the learning

- **Trigger — automatic:** (new session) AND (no binding chosen for this session OR resolved `caller_root` outside the would-be brain's `workspace_root`). Detection is bridge-side — the bridge is the only seat that knows the caller (§9.5.4).
- **Carrier — no new "must-call" verb:** the **first `tools/call` of the session, whatever verb the agent sends** (doctrine says `north`), returns the reception packet as its result when the trigger holds. Zero doctrine change on hosts; an agent that has never heard of reception still receives it, reads `options[]`, and acts — the intent that "the system should hand over all the information" made mechanical. On `bound_match` there is **no interruption**: the verb flows through and `where_you_are` collapses to one stamp inside `north.binding` (which already carries the fingerprint).
- **Sticky — never re-ask:** the choice is held for the session's lifetime (bridge process for stdio sessions; wire `Mcp-Session-Id` state at the owner for HTTP). Honest v1 limit: **mid-session cwd travel is not re-detected** — the bridge's cwd is fixed at spawn; the per-call `scope` guard remains exactly the backstop §1 says it is (risk §21.15).
- **The answer — `bind`, one net-new verb, bridge-intercepted.** Reuse audit, per the mother rule: no existing verb means "choose a brain" — `session_handshake.scope` *validates* a binding, `north` *uses* one, env pins are set-at-spawn; overloading any of them is contortion, so the few clear lines win. `bind {brain}` is answered by the **bridge** (it re-routes its attach; the next verb wears the chosen brain's fingerprint). For direct-HTTP callers with no bridge, the owner answers `bind` **honestly but cannot re-route the caller**: it returns the target's endpoint (`bind`/`port` from the registry) and instructs re-attach — the asymmetry is named, not hidden.
- **The learning — defaults that learn:** every option footer carries the exact `memorize` call that turns this choice into standing policy on the medulla (e.g. `label: "reception_policy_new_repo"`, claim: *"new repo → always create a project brain"*, `kind: state`). v1: the agent executes it (manual, one call, the machinery exists today). V2: the composer *consults* medulla policy claims at compose time and moves `suggested` accordingly — riding the Q3 federated-read + negative-cache machinery, never blocking the packet on a medulla read (until then, `honest_gaps` carries `medulla_policy_unread`).

#### 9.5.3 Who does what — first contact, end to end

The bridge detects (it owns the caller's truth); the owner composes (it owns the registry view + its live stats); the agent chooses; the bridge executes the bind; the medulla remembers the policy when told to.

```mermaid
sequenceDiagram
    autonumber
    participant A as agent (any host)
    participant B as bridge (--attach auto)
    participant O as owner (landed-on brain / medulla)
    participant M as medulla (:1338)
    A->>B: first tools/call — north(task), per doctrine
    B->>B: resolve caller_root (env → cwd) · compare vs owner workspace_root
    alt bound_match
        B->>O: forward verb unchanged (+ M1nd-Caller-Root header)
        O-->>A: normal north — where_you_are stamped in binding
    else mismatch / virgin / ambiguous
        B->>O: forward + first_contact detected (M1nd-Caller-Root)
        O->>O: compose m1nd-reception-v0 — registry read + live stats
        O-->>A: THE PACKET — where_you_are · what_exists · options[] · suggested+why · honest_gaps
        A->>B: bind {brain} — or runs `m1nd init` (create), then bind
        B->>B: re-route attach to chosen owner · choice sticky for session
        B-->>A: bound — next verb wears the chosen brain's fingerprint
        opt policy worth keeping
            A->>M: memorize(reception_policy_*) — defaults learn (v1 manual)
        end
    end
```

#### 9.5.4 The wire truth — how the caller's cwd is actually known *(verified at `6c53c47`)*

- **Hop 1, host → bridge: AVAILABLE TODAY.** The bridge is a process the host spawns, so it inherits the host session's env — `mcp-config` already emits `M1ND_WORKSPACE_ROOT` into every host config (`npm/lib/cli.js:481-509`), the candidate ladder is host-neutral (`WORKSPACE_ROOT_ENV_CANDIDATES`, `session.rs:393-421`), and the bridge's own spawn cwd is the fallback. This is the same knowledge §9.1's walk-up rides; reception adds no wire surface here.
- **Hop 2, bridge → owner: A REAL GAP TODAY.** The attach client sends only `Accept`, `Content-Type`, `Mcp-Session-Id`, `MCP-Protocol-Version` (`attach_client.rs:248-260`); MCP `initialize.clientInfo` carries name/version — **no cwd field exists in the spec**; and the owner resolves its workspace from **its own** env + cwd (`session.rs:1269`), not the caller's. This is mechanically why the project-d session wore the m1nd brain with a straight face. Net-new, small: the bridge stamps **`M1nd-Caller-Root: <resolved>`** on every forwarded request; the owner stores it per wire session; **absent → unknown** (legacy bridges keep working — the serde-default posture, applied to the wire).
- **The standards-track channel, honestly weighed:** MCP defines a client `roots` capability (`roots/list` + `notifications/roots/list_changed`) — file-URI workspace roots the server may request from the client. The bridge MAY consult it to *refine* `resolved_via` when the host declares the capability at `initialize`. **None of the four live hosts is proven to serve `roots/list` today** — unverified, so it is a refinement, never the v1 truth source; env + cwd is.
- **Direct HTTP (no bridge):** the header is optional by construction; without it reception degrades to `where_you_are.verdict: caller_root_unknown` — the packet still lists `what_exists` and options, and says plainly that it cannot compute the match (TT-INV-2: absent ≠ wrong).
- **Reconnect collapse — [SHIPPED 2026-07-05 · ORGANISM R13, field letter#49].** Hop 1's cwd fallback has a sharp edge: after an MCP reconnect the bridge re-resolves `caller_root` from its spawn cwd, and a host launched ABOVE the repo (the classic `~` launch) yields the host cwd — an ANCESTOR of the repo, not the repo. The fresh wire session (`bound_project_root` cleared) then has a `caller_root` with no brain of its own, the bound graph does not cover it, and the call falls to the owner graph suggesting `ingest project_root=<host cwd>` — the wrong root, blind to the existing project brain sitting UNDER the caller. **Fix:** the routing seam now consults the disk roster (the R8 cold-listing) via `ProjectBrainRegistry::covering_brain` — the UNIQUE known brain related to the caller by ancestry (either direction; `None` on zero = unknown repo, or >1 = ambiguous; an exact match is excluded, it is a silent bind). On the owner-default mismatch path, `mcp_http::enrich_reception_with_roster` rewrites the reception to name that brain (`known_brain` + the `ingest_your_repo` call pointing at the repo root, a warm re-bind — not a fresh birth). One seam covers `north` / `health` / `session_handshake`; a matched caller still binds silently (TT-INV-12). Proof: `tests/two_tier_project_brains.rs::reconnect_reception_prefers_the_existing_brain_over_the_host_cwd` (+ the unchanged-branches test + a `covering_brain` unit test).

#### 9.5.5 Degraded reception — implementable NOW, pre-two-tier

Nothing in the Antigravity fix waits for per-project brains. Today, already built: the bridge knows the caller root (hop 1); the owner's bound root is one cheap `session_handshake` away (binding fingerprint — test `server.rs:5914`); the registry enumerates read-only (`list_instances`); `~/.m1nd/runtimes/*` carry snapshot mtimes + `ingest_roots.json`. So the degraded packet is: on mismatch, the first call returns reception with `what_exists` = the runtimes/registry enumeration, and `options[]` = attach-another-runtime (`M1ND_ATTACH_URL` / `M1ND_RUNTIME_DIR` pin — **restart-scoped, stated honestly**: no live `bind` verb exists yet), ingest-cwd-into-a-fresh-runtime, or proceed-mismatched by explicit consent. What the full protocol adds on top (Slices 2+3 machinery): the walk-up verdict, live `bind` re-routing, the spawn-backed `bind_existing`, and `m1nd init` as the create path. **Degraded reception has no dependency on Slices 0–2 and MAY be pulled first — the field friction is live today.**

> **SHIPPED 2026-07-04 (Slice 2R degraded mode):** the `M1nd-Caller-Root` hop-2 header (serde-default; absent → unknown), the owner-side `reception_verdict` (reuses the mismatch guard's `path_starts_with_loosely` over `workspace_root` + `ingest_roots`), and the compact `reception` block on `north` / `health` / `session_handshake` (schema `m1nd-reception-degraded-v0`: `match` / `caller_root` / `bound_workspace` / `honest` / `options[]`). Scope shipped: the mismatch flag + machine-executable options + silence-on-match (TT-INV-12) + honesty-by-omission on unknown caller root (§9.5.4). STILL OPEN for Slice 2R proper: the full `m1nd-reception-v0` packet (`what_exists` registry enumeration, `known_brain_elsewhere` / `unknown_repo` / `ambiguous` verdicts, walk-up), the live `bind` answer verb, spawn-backed options, and `create_project_brain` via `m1nd init`.
>
> **SHIPPED 2026-07-04, same day (interim variant — owner-hosted project brains: one-call bootstrap + silent cwd routing).** Field pressure (a second project's sessions wearing the m1nd brain all day; the maintainer's verdict: until this is fixed the system isn't functional — the friction means the agent simply doesn't use it) pulled the FUNCTIONAL core of per-project brains ahead of the process-per-repo slices, as an honest **shipped variant**, not the canon:
>
> - **What shipped:** the ONE served owner now hosts MULTIPLE graphs — its bound dev graph (untouched; still serves stdio and matching callers exactly as before) plus N per-project brains (`m1nd-mcp/src/project_brains.rs`), each a full `SessionState` with its own store, lease (same-PID multi-lease was already legal in `instance_registry.rs`) and persistence. **One-call bootstrap:** `ingest` gains `project_root` — one call creates the store, ingests the caller's repo, binds the wire session, and returns the NEW brain's `north` packet in the same response (`schema: m1nd-project-brain-bootstrap-v0`). **Silent routing** (TT-INV-12): per call, precedence = bootstrap directive → session sticky choice → caller-root match (bound first, then project brains, warm-booting dormant stores) → bound default; NEW sessions from a bootstrapped root bind their brain with zero setup calls and no reception block. The reception `ingest_your_repo` option now carries this REAL invocation. Registry entries gain serde-default `brain_kind` (`"project"` stamped on project brains) so the existing `list_instances`/doctor surface tells the fleet apart.
> - **Shipped-variant divergences, stated:** (1) stores are **owner-side** — `<owner runtime_root>/project-brains/<fingerprint(root)>/` — NOT the canonical `<repo>/.m1nd/`: repo-local dirs are bound to the consented `m1nd init` birth (TT-INV-8), which this interim deliberately does not ship, and an owner-side store writes nothing into anyone's repo; (2) brains are **in-process graphs inside the one owner**, not process-per-repo owners — the §2 blast-radius and per-brain-calibration arguments for process isolation still stand and remain the end state; this variant is the smallest cut that kills the live friction. Both fold into the canon when Slices 2/3 land (a repo-local brain simply wins discovery over an owner-side store).
> - **Still open (unchanged by this variant):** process-per-repo spawn-on-miss + walk-up (Slice 2), `m1nd init` + memory-travels-with-git (Slice 3), the medulla split (Slices 5-7) *(pointer, 2026-07-05: the memory layer of that split — states, promotion, pull-only recall, delegation inheritance, the mailbox boxes — is now SPEC'D whole at `docs/MEDULLA-PRD.md`, the M-ladder M5a→M7b, grounded on a live probe of THIS variant's routing)*, the live `bind` verb and the full `m1nd-reception-v0` packet. ~~**eviction/limits for the in-owner brain map (none today — unbounded, one graph's RAM per bootstrapped project)**~~ — **CLOSED, SHIPPED 2026-07-05 (ORGANISM ladder R15, §C9.1):** the warm map is LRU-bounded (`DEFAULT_WARM_BRAIN_CAP = 4`, configurable) with per-brain persist-on-evict — every insert (bootstrap + warm-boot) routes through `ProjectBrainRegistry::insert_with_eviction`, which flushes the least-recently-used **project** brain to its store (the bound dev graph is not in the map, so it never evicts) before dropping it; a `kill -9` is survived by every brain warm-booting from its own snapshot. Proof: `tests/two_tier_project_brains.rs::eviction_gate_bounds_the_map_and_persists_on_evict_surviving_kill9` + `project_brains::eviction_gate_tests`. *(Brain-scoped `graph_changed` and per-brain **execution/browsing** over REST — once open here — are now **SHIPPED 2026-07-04** via HUMAN-LAYER-PRD §4A.9's `?brain=` selector; see the next bullet.)*
>   - **REST enumeration — SHIPPED 2026-07-04 (this PR):** `/api/instances` now LISTS every hosted project brain, PROJECT-named. Each entry carries a server-resolved `display_name` (repo basename — the bound graph reads "m1nd", never its `agent-memory` sidecar or the runtime dir "claude") + `project_root` (resolved from the store's `project_brain.json` manifest for hosted brains, from `SessionState::project_root_display` for the bound one), `brain_kind` distinguishes them, and the bound brain floats first. A project brain also carries its **own graph counts** (warm brain when live, else the manifest's recorded `node_count`/`edge_count`, stamped at bootstrap) + a freshness stamp — because a project brain lives IN-PROCESS and has no owner-instance `running`/`stale`/lock status, the Hall shows those counts (never "not running"), a calm live dot, and NO lock badge (HUMAN-LAYER-PRD §4A.3, project-aware `hallSemantics`). ~~**Still open:** per-brain *browsing/execution* over REST~~ — **CLOSED, SHIPPED 2026-07-04 (slice 2H).**
>   - **Per-brain Open over REST — SHIPPED 2026-07-04 (slice 2H, HUMAN-LAYER-PRD §4A.9):** `/api/graph/*` and `POST /api/tools/*` now take `?brain=<project_root>`, routed through the SAME resolution the wire uses (`resolve_brain` → `project_brains.rs`: exact-root match → warm brain, else warm-boot the dormant store; absent = bound, byte-compatible; unknown root → honest error, never auto-created; registered-roots-only, loopback-only). Every `/api/graph/*` response carries the `served_brain` echo; `GET /api/tools` stamps `rest_brain_selector: true` (feature-detect); `graph_changed` gained the optional `brain_root`. The GUI adopts it end-to-end (Open enabled on hosted cards, tree opens the brain in-tab dropping echo mismatches — INV-15, chip flips to the echo, warm-boot in words — INV-05, 1T lenses/filters/meaning-search ride the selector — INV-16). Proof: `m1nd-mcp/tests/per_brain_open.rs` (7/7) + 29 new UI tests.
>   - **Cold-listing bug fixed — SHIPPED 2026-07-04 (same slice):** a field-mailbox bug (class:bug, "a project brain disappeared") — after an owner restart a dormant project brain vanished from the Hall until a routed call warm-booted it, because `instances_listing` only re-listed a brain once it was warm. Fixed with `ProjectBrainRegistry::disk_roster()` + a cold union in `instances_listing`: it scans `<runtime_root>/project-brains/*/project_brain.json` on disk and lists dormant brains with ZERO routed calls (display_name from the root basename, counts/freshness from the manifest — listing ≠ warm-boot; the in-memory map wins duplicates). RED→GREEN pinned in `per_brain_open.rs` (`hall_lists_dormant_project_brain_from_disk_after_restart`).
>   - **Per-brain session/query PARTITION (§9.5.1) — SHIPPED 2026-07-05 (ORGANISM ladder R14).** The honesty gap this closes (field-report letter#51): the Hall's G4 aliveness line and the `/api/instances` listing wore the OWNER-GLOBAL session/query counters, so sessions bound to OTHER hosted brains inflated a card that named one brain. The partition keys on the session's bound brain: because each project brain is a full `SessionState` and a routed call dispatches against the brain that owns the caller (`mcp_http::route_and_run` → `serve_and_compose`), a tool call carrying `agent_id` records its session (`track_agent`) and increments `queries_processed` on THAT brain only — the counters were already partitioned at the data layer; R14 exposes the partition at the surface. **What shipped:** `instances_listing` (`http_server.rs`) now enriches every entry with its OWN `attached_sessions` + `query_count` + `calibration_armed` — self/bound brain from its own `SessionState`, a warm project brain via `ProjectBrainRegistry::warm_session_stats` (`SessionState.sessions.len()` / `queries_processed` / `calibration_armed()`), a dormant brain **absent** (no live wire sessions — never a faked 0, TT-INV-2). The owner-WIDE total is NOT gone: it stays on the owner's own receipt (`/api/instances/self`, `/api/health`), labeled owner-wide. The **`what_exists` registry fields `attached_sessions` + `calibration_armed`** (the §9.5.1 reception-packet table, previously marked *unbuilt*) are REAL where a live brain backs them. UI: G4 (`m1nd-ui/src/lib/cardV2.ts`) reads the per-brain count and the interim "across all brains" qualifier is **removed** (the number is now truly the brain's own); absent → the row renders nothing. **Proof:** RED→GREEN pinned in `m1nd-mcp/tests/hall_brains_listing.rs` (`per_brain_counters_partition_on_the_bound_brain_not_owner_global` — two brains driven to divergent session/query counts, each card wears its OWN count and NEVER the cross-brain sum; `dormant_project_brain_omits_live_session_counters_absent_honest`); UI `cardV2.test.ts`/`card-v2.test.tsx` (qualifier gone, absent-honest). Full `cargo test -p m1nd-mcp` green; UI 207/207.
>   - **Overlap guard on mint — SHIPPED 2026-07-10 (field friction: twin brains for one project).** The field report (class:friction): a Codex session opened with cwd in a repo's PARENT folder while a brain already existed for the repo INSIDE it → the mint path cunhou a SECOND brain that re-ingested the repo from above (double cost, memories fragmented across two stores that needed a manual migration); separately, a git WORKTREE of a brained repo grew its own brain that orphaned when the worktree died. Law: before minting a NEW project brain, the mint path classifies `project_root` against every existing brain (warm map ∪ on-disk roster, `ProjectBrainRegistry::existing_brain_roots`) into **child** (inside an existing brain's root), **parent** (an existing brain's root is inside this one — the mother-folder case), or **worktree** (`.git` is a gitdir file under `<main>/.git/worktrees/` and the main repo has a brain) and REFUSES with one honest `overlap_<class>` `InvalidParams` naming the conflicting root + the two ways forward — bind to the existing brain (`ingest project_root=<existing>`, the 90% case) or pass the escape hatch `allow_overlap:true` to mint a separate brain anyway. `allow_overlap` is a routing directive read from the raw ingest arguments (deliberately NOT an `IngestInput` field), stripped before the inner ingest exactly like `project_root`; the exact same root stays warm-reuse (`reused:true`), never a refusal. Mirrors the `synthetic:true` posture of `mission_post`: refuse by default, explicit escape, a message that teaches the right call. **Proof:** `project_brains::overlap_guard_tests` (parent/child/worktree refuse; `allow_overlap:true` and disjoint roots mint; same-root warm-reuse never refuses) + pure-unit tests of `detect_root_overlap` (child/parent/worktree/none + gitdir resolution). Agent-facing surface updated same PR: `ingest` schema `allow_overlap` + `docs/uml/routing-reception.md` invariant.
>   - **REST-seam parity — SHIPPED 2026-07-10 (same day, live-smoke hole).** The live smoke caught the guard protecting only the JSON-RPC seam: `POST /api/tools/ingest` (`http_server::handle_tool_call`) IGNORED `project_root` in the body and dispatched the ingest on the RESOLVED brain — the BOUND graph when `?brain=` is absent — so a bootstrap-shaped call through the REST door REPLACED the owner's bound ingest_roots (restored by hand). Fix: ONE seam-shared core, `mcp_http::run_bootstrap_core` (bound-shadow guard → guarded mint with the overlap guard + `allow_overlap` escape → ingest → same-response `north`), now called by BOTH doors — the wire frame (`run_bootstrap`, which adds the sticky session bind) and a REST interception styled exactly like `mission_spawn`/`candidate_naming` (which states the `?brain=` routing law instead). The refusal reaches REST callers as an honest HTTP 400 `invalid_params` carrying the guard's full message; `bootstrap_directive` is the one definition of "this ingest is a bootstrap" for both seams; a directive-less REST ingest (incl. `?brain=` re-ingest) is byte-untouched. **Proof:** `http_server::tests` REST battery driving the REAL handler — parent refusal + bound-graph-untouched (the test that would have caught the hole), disjoint mint parity (same envelope), no-directive dispatch unchanged, `allow_overlap` escape.
> - **Proof:** RED pinned live (`m1nd-mcp/tests/two_tier_project_brains.rs` — pre-fix, `ingest` from another root REPLACED the bound dev graph: the clobber, captured failing) → GREEN 4/4; the enumeration surface pinned in `m1nd-mcp/tests/hall_brains_listing.rs` (both brains listed, `display_name` = project basename, bound-first, naming guard) + `session.rs` unit tests (the agent-memory-sidecar leak) → GREEN; full `cargo test -p m1nd-mcp` green; the Hall render-proof rides the real captured `/api/instances` fixture (`m1nd-ui/src/__fixtures__/instances.json`), UI 83/83 incl. the naming guard.

#### 9.5.6 Relationship to the existing first-contact surfaces — no zombies

- **`session_handshake` — NOT superseded; orthogonal axis.** Handshake validates the trust of an *already-chosen* binding + the host tool surface (`observed_tool_count`/`missing_tools`) + opt-in per-call `scope`; reception happens *before* a binding is chosen. Reception **reuses** the guard's path-comparison classification and **demotes** the guard's first-line role exactly as §1 already demotes it: front line at first contact belongs to reception; the guard remains the mid-session backstop (scope drift, override misbinds). **One prescribed migration, same slice:** the guard's `suggested_fix.preferred` ("rebind the MCP host with `M1ND_WORKSPACE_ROOT`…", `session.rs:765`, `tools.rs:3545/3861`) gains a first branch — *when the session never answered a reception packet, the fix is "answer it"* — so the two surfaces point at each other instead of prescribing parallel cures.
- **`north` — reception precedes and feeds it, never competes.** On `bound_match`, north is untouched. On mismatch/virgin, the agent's first north *call* returns the packet; the first north *orientation* happens one round trip later, against the chosen brain. `needs_ingest` keeps its meaning (post-bind, empty graph); `create_project_brain` is the pre-bind path to the same repair.
- **`M1ND_INSTRUCTIONS` (`server.rs:33-89`) — doctrine unchanged, one paragraph added.** "Call `north(task)` FIRST" survives verbatim — reception rides that first call; it does not ask agents to learn a new opening move. The instructions gain a short §0 teaching the packet shape and the `bind` answer. **Agent-workflow surface gate applies:** the instructions text, the agent-pack/skills text, and the host rule files ship in the SAME PR as the implementing slice — prescribed here, owed by Slice 2R.
- **The Pre-Flight Card echo (HUMAN-LAYER-PRD Slice 1, surface S2).** The reception packet IS the Pre-Flight Card's binding header: one packet, two renderings — "rendered for the human it is the Pre-Flight Card; rendered for the agent it is the delegation packet" is that PRD's own shell doctrine, and reception supplies the binding block it renders. Data contract only in this PRD; no UI work is smuggled in. *(Pointer, 2026-07-04: the reception truth gained a second human rendering — HUMAN-LAYER §4A's Hall renders the brains-registry/`what_exists` view as the owner-level projects area, and its Brain Chip echoes this packet's match verdict on every surface; still data-contract-only from this side.)*

---

## 10. The composed north — doctrine + project, fail-open

### 10.1 v1 shape — the hook-side two-call compose `[C-FIX 5; Axis-4 adoption]`

The doctrine feed may not regress across cutover, and slice-1 value must not wait for an HTTP composition layer. So v1 composes **in the hook** (~10 lines of shell, inside the existing `timeout 4 … || exit 0`):

1. **Project beat:** `--attach auto` → `north(task)` against the project owner (or medulla-only when no brain).
2. **Doctrine beat:** `--attach medulla` → `seek` doctrine + boot-memory keys, **≤1 s hard timeout**.
3. **Compose:** one calm ≤1200-char packet (today's hook budget), items tier-tagged, gaps carried.

```mermaid
sequenceDiagram
    autonumber
    participant H as SessionStart hook — timeout 4, fail-open
    participant B as bridge (--attach auto)
    participant P as project owner (repo/.m1nd)
    participant M as medulla (:1338)
    H->>B: resolve cwd (walk-up)
    alt owner live — registry hit
        B->>P: north(task) — ≤2 s warm
        P-->>B: anchors + project memory + fingerprint
    else brain exists, no owner
        B->>P: spawn detached (port 0) — return immediately
        Note over B,P: never waits — beat degrades to warming
    end
    H->>M: doctrine beat — seek/boot_memory, ≤1 s HARD
    alt medulla up
        M-->>H: doctrine claims (tier: medulla)
    else down or timeout
        Note over H,M: gap: medulla_unreachable — labeled, never blocking
    end
    H->>H: compose ≤100 ms — tier-tagged packet, ONE fix per gap
    H-->>H: exit 0 ALWAYS — worst case is today's baseline
```

### 10.2 V2 shape — server-side federated read (deferred whole `[KILL list]`)

The end state (Q3): the project owner's `north` handler performs a read-only HTTP MCP call to the medulla and composes it as a **third feed** into the existing memory-beat merge (`server.rs:2949-3128`, which already composes boot-memory KV + L1GHT recall into one `memory: Vec` at `:3120`), reusing `attach_client.rs` machinery. Ships with: 5-min TTL cache per owner; expired-cache-while-down served **labeled** `stale: true, age_s: n` (staleness is worn, never hidden); and a **negative cache / circuit breaker (60 s)** so a down medulla degrades one beat, not every session's startup all day `[C-FIX 6]`. No graph merges ever: `federate`/`federate_auto` remain multi-repo *ingestion* tools inside one brain and are verb-gated OFF on the medulla.

### 10.3 Latency budget — the hook's ≤4 s ceiling stands (`~/.claude/hooks/m1nd-north.sh`, `timeout 4`, fail-open)

| Phase | Budget |
|---|---|
| Bridge resolve + attach (warm) | ≤ 300 ms |
| Project `north` (warm) | ≤ 2 s |
| Medulla doctrine beat | ≤ 1 s **hard timeout** |
| Compose (shell) | ≤ 100 ms |
| Slack | ≥ 600 ms |

**Cold paths, stated honestly `[C-FIX 3]`:** the hook **spawns-and-returns, never waits.** Snapshot exists → one attach attempt inside the project beat's window; owner not ready in time → `warming` + doctrine-only. Full ingest needed → background spawn, `warming` + medulla-only. **The first session of a repo whose owner idle-exited gets rung 4: doctrine-only; anchors arrive on the second beat.** The "≤2.5 s warm from snapshot" figure is a budget until Slice 3 measures it.

### 10.4 Degradation ladder (compose, don't couple — absent ≠ wrong)

1. Both up → project anchors + project memory + medulla doctrine + honest gaps.
2. Medulla down/timeout → project north + `gaps:[{kind: medulla_unreachable}]` (V2 adds the labeled stale cache).
3. Project brain absent → medulla-only + `gaps:[{kind: project_brain_absent, fix: "m1nd init"}]` + `needs_ingest`.
4. Project brain warming → medulla-only + `warming{pid}`.
5. Both down → hook exits 0 silently (existing fail-open).

Every memory item carries `tier: project|medulla` + the source brain's fingerprint, so binary drift stays visible **per brain** in one composed view.

---

## 11. Identity & provenance

**`agent_id` convention:** `host:role`, lowercase, single colon. Hosts: `claude, codex, gemini, antigravity, human, ci`. Roles: `orchestrator, maker, reviewer, researcher, guardian, founder, battery`. Examples: `claude:orchestrator`, `codex:maker`, `human:founder`, `ci:battery`.

> **[A1 · superseded by `docs/ORGANISM-PRD.md` §C5.1]** The `host:role` sketch above is **superseded** by the canonical grammar `host:tier:name[@parent]` (e.g. `claude:main:fable-orchestrator`, `claude:sub:burst-1t@fable-orchestrator`). The promote-gate parses the **tier** token (`tier == "main"` OR `human:founder`), never greps the name; hosts are open data, never a hardcoded enum. Source of design law: §C5.1.

**Transport (net-new, tiny):** `M1ND_AGENT_ID` env read at bridge start and injected into every forwarded call's meta; per-call explicit `agent_id` (already a free-form string in the verbs — no schema change) wins. `mcp-config` writes each host's default (`M1ND_AGENT_ID=codex:maker`, etc.).

**Attribution:** `memorize`/`learn`/`mission_*` stamp `agent_id` + brain; promotion chains preserve `origin_brain → promoted_by` (§8). Cross-brand provenance becomes readable history: *"codex:maker learned this in project-b; claude:orchestrator promoted it."* Restated plainly: provenance is **audit**, not enforcement (§8, §21.1).

**Delegation Packet (NEXTGEN §O.12) — dual-tier sourcing, mandatory fields:**

- *From the project brain:* north anchors (file:line), the `impact` set for the touched surface, relevant claims (`seek`), a calibration snapshot (trust level + n — how much to trust predictions *here*), `needs_ingest`/drift flags.
- *From the medulla:* the doctrine block (git identity rule, proof standard, model routing), maintainer preferences relevant to the task domain, promotion etiquette (makers propose, never promote).
- *Footer:* `sources: {project: <repo>@<git-sha> + brain fingerprint, medulla: <n claims>, composed_ts}` — the subagent's grounding is auditable.

Where a host has a SessionStart surface, subagents also receive the composed north automatically; where it doesn't, the packet text is the carrier.

---

## 12. Interop map — every existing surface

| Surface | Today (verified) | Two-tier adaptation |
|---|---|---|
| **Claude hook** | `~/.claude/hooks/m1nd-north.sh` hardcodes `--attach http://127.0.0.1:1338` (line 10); `timeout 4`, fail-open | **hooks v2 (Slice 6):** the two-call compose (§10.1); `--attach auto` + `--attach medulla`; spawn-and-return; `timeout 4` + fail-open unchanged. |
| **Codex** | `~/.codex/hooks.json` SessionStart = headroom only; `~/.codex/bin/m1nd-stdio-session.sh` spawns per-session owners under `~/.m1nd/runtimes/<hash>` | Add the composed-north call to SessionStart; **retire `m1nd-stdio-session.sh`** once Slice 2 lands (it was the proven seed; bridge spawn supersedes it — per-session runtimes collapse into per-repo brains). |
| **Gemini / Antigravity** | No hook surface (honest gap) | mcp-config wiring + agent-pack `m1nd-first` rule = north-on-first-tool-call. Stated honestly: north-before-first-token only where hooks exist. |
| **`mcp-config` / `hosts apply`** | Emits embedded `--stdio --no-gui` + `M1ND_WORKSPACE_ROOT` only (`npm/lib/cli.js:481-509`, `1043-1050`) | **The highest-leverage edit:** default emit = `--attach auto` + `M1ND_WORKSPACE_ROOT` + `M1ND_AGENT_ID`; `--project` also pins `M1ND_RUNTIME_DIR`; `--embedded` keeps today's form. Idempotent writer (`writeHostConfig:1062-1073`) unchanged. [KILLED: `hosts apply --remove` polish — not v1/v2 scope.] |
| **`install-skills` / agent-pack** | Writes `<proj>/.m1nd/agent-pack/` (`cli.js:606-616`) | Same dir, coexists with the brain; stays gitignored; skill text updated (Slice 6) to teach composed north + promotion etiquette. |
| **Rule files (CLAUDE.md/AGENTS.md)** | Host rule file written by install-skills | Gains the two-tier paragraph: project brain = repo truth; medulla = doctrine; memorize→project by default; promotion is orchestrator etiquette. |
| **Battery** | Tracked battery precedent (`.gitignore:56-60`) | **Per-brain batteries:** `<repo>/.m1nd/battery/` committed (Slice 3 policy); cases accumulate per slice (§14). CI wiring (project battery in repo CI; medulla doctrine-battery on the doctor schedule) is **V2**. |
| **Field telemetry** | `~/.m1nd/field-reports.jsonl`, global; 30 lines live | **Stays one global mailbox** (B3, argued §19). Reports gain a `"brain"` field → per-brain calibration ground truth. `[KILLED: auto-field-report-on-corruption — reporting stays a doctrine act by agents/humans, not silent machinery.]` |
| **launchd** | `com.m1nd.serve` serves the mixed `:1338`; **no `WorkingDirectory` → cwd=`/` → persist failures (live, root-caused)** | Slice 0 fix (plist key + absolute `--graph` + code fix); re-scoped medulla-only at migration; project brains are never launchd-managed. |
| **Workspace-binding guard** | `session.rs:747-836`, `:1296-1342`; wired into seek/handshake/recovery | **Unchanged.** Role shifts to defense-in-depth backstop (§1); its `suggested_fix` gains the answer-the-reception branch (§9.5.6, Slice 2R). |
| **`M1ND_INSTRUCTIONS`** (initialize instructions) | Static text injected at `initialize` (`server.rs:33-89`): "call `north` first" + verb doctrine | **Doctrine unchanged**; gains the reception §0 (packet shape + `bind` answer) in the SAME PR as Slice 2R — agent-workflow surface gate (§9.5.6). |
| **`federate` / `federate_auto`** | Merge repos into ONE owner's graph (`layer_handlers.rs:6050` id-prefixing) | Unchanged for real multi-repo analysis *within a project brain*; refused on the medulla; docs state they are NOT the cross-tier mechanism — and they ARE the §2 escape hatch for the cross-project-code-query non-goal. |
| **`persist` / snapshot boot** | Exists; the medulla never persisted (bug, now root-caused) | Project brains persist at idle-exit/SIGTERM/post-ingest; medulla persists memory state only. |
| **HUMAN-LAYER-PRD S5** | "Project Brain" = read-only memory UI | Renamed **"Project Brain Panel"** — one doc line, scheduled in Slice 6 (§4.3). |
| **Docs/PATHOS** | The doc gate | Each slice ends at the documentation gate; this PRD itself lands with the §O.13 pointer in the same PR. |

---

## 13. Honesty invariants (TT-INV — the floor; every slice must hold all of them)

1. **TT-INV-1 · Fail-open is sacred.** The SessionStart ceiling (`timeout 4 … || exit 0`) survives every failure in §17; the worst case is today's baseline — a session with no m1nd context, never a blocked session.
2. **TT-INV-2 · Absent ≠ wrong.** Every degradation is a labeled gap (`project_brain_absent`, `medulla_unreachable`, `warming{pid}`, `needs_ingest`) — composed, never silently absorbed.
3. **TT-INV-3 · Staleness is worn, never hidden.** Any cached block served is labeled `stale: true, age_s`; every memory item carries `tier` + source-brain fingerprint; binary drift surfaces per brain in every composed north.
4. **TT-INV-4 · ONE fix per degraded envelope** (B9), routed through the existing `recovery_playbook`. A menu is a failure of nerve.
5. **TT-INV-5 · Budgets are gates, not claims** `[C-FIX 3]`. No "warm ≤2.5 s" prose anywhere until the Slice 3 gate measures it; same for first-ingest wall time and graph-loaded RSS.
6. **TT-INV-6 · The morning is cold when it is cold.** First touch of a drained repo = doctrine-only + `warming`, stated in the packet itself.
7. **TT-INV-7 · Promotion gating is etiquette-by-provenance, not a security boundary** `[C-FIX 8]`. `agent_id` is self-declared; violations are auditable, not preventable.
8. **TT-INV-8 · No silent births, no executable manifests.** `m1nd init` is the only way a brain is born; `brain.json` is inert data; spawn execs `current_exe()` only; `expected_sha` can only refuse.
9. **TT-INV-9 · Memory is never truly lost while git lives — and eject never purges history.** A leaked secret is a history-scrub + rotation job, never an eject (§20); the write-path scan (§7.1) is a floor, not a proof.
10. **TT-INV-10 · A brain without a green battery is a hypothesis.** Applies to features too: the swarm roster stays a hypothesis until it has a slice and a gate.
11. **TT-INV-11 · Scale and coverage claims are re-measured at gate time.** The "~54k entries" → 143-files correction (§9.2) is the standing precedent; "~90% composes"-style unfalsifiable percentages are banned from this PRD's claims `[C-FIX 8]`.
12. **TT-INV-12 · Silent binding is legal only on match.** A new session whose resolved caller root matches the brain being bound flows silently; every other first contact receives the reception packet (§9.5) — one packet, one choice, sticky. Binding on mismatch without a recorded choice is a bug, never a default. (Field-proven RED: the Antigravity silent-bind, 2026-07-03.)

---

## 14. PROOF-GROWN plan — V1 slices (each: scope · RED→GREEN gate · battery additions)

Every slice independently shippable; **global rollback** at any point = re-point host configs to `--attach http://127.0.0.1:1338` (one line per host, ≤5 min). Nothing is claimed without its gate green. Scope verdict adopted from the review (Axis 6): **the shippable V1 spine is deliberately ~⅓ of the seats' original plan**; everything else is V2 (§15).

### Slice 0 — Medulla persistence fix (pre-cutover blocker; Q5 root-caused)

- **Scope:** plist `WorkingDirectory` + absolute `--graph`; durable code fix — every persist target (snapshot, ingest-roots, the full `delete_instance_state` file set) resolves against `runtime_root`, never cwd; net-new tiny `persistence_degraded: true` flag in handshake/north when a persist write fails (keep serving from RAM — never stop).
- **RED (live now):** `serve-claude.err.log` logs `ingest roots persist failed: Read-only file system (os error 30)`; every restart logs `No graph snapshot found, starting fresh`; the live lease records `workspace_root:"/"`.
- **GREEN:** persist → restart → "loaded snapshot" in the log; zero os-30 lines; new lease `workspace_root` ≠ `"/"`; a second restart is clean.
- **Battery:** persist→warm-reboot cycle case; regression case: owner with read-only cwd + writable `runtime_root` persists everything (the live failure, mechanized).

### Slice 1 — Ephemeral ports

- **Scope:** `--port 0`; owner publishes the real bound port via the existing `set_running_endpoint` write (`instance_registry.rs:645-650`); medulla alone keeps `:1338`.
- **RED:** two owners on the default port → second fails bind (failing integration test).
- **GREEN:** two live owners, registry entries publish distinct real ports, `discover_serve_owner_base_url` returns each correctly.
- **Battery:** coexistence case beside the existing tests at `instance_registry.rs:725`/`:881`.

### Slice 2 — Bridge walk-up + spawn-on-miss + lifecycle gates `[C-FIX 3, 4, 9]`

- **Scope:** walk-up resolution (§9.1, `$HOME` excluded); detached session-independent spawn; bounded retry; race via the existing lease (zero new locking); `--idle-exit-secs` (persist → release → exit 0; no wake special case); spawn-and-return semantics; machine-wide background-ingest cap (one); `m1nd brain stop`; `doctor`/`list_instances` fleet single-pane table (§2 adopted posture 3).
- **RED:** `--attach auto` from a repo with `brain.json` but no live owner errors today (captured as a failing test); a spawned owner dies with its spawning session; idle-exit and SIGTERM-persist are untested.
- **GREEN:** same call spawns, attaches, `session_handshake.binding.workspace_root == <repo>`. **Race gate:** two bridges spawn simultaneously ×20 loops → exactly one owner PID per `runtime_root`, both bridges attach OK. **Orphan gate:** bridge exits → owner PID alive → a second bridge attaches the same PID. **Idle gate:** `--idle-exit-secs 2` → persists on the idle clock AND on SIGTERM; lease released; next spawn boots from the snapshot. **Walk-up gate:** a dir directly under `$HOME` with no brain never resolves `$HOME`. **Fleet gate:** `doctor` renders every live owner (path, kind, port, pid, age) in one table.
- **Battery:** race ×20 case · orphan case · idle/SIGTERM-persist case · `$HOME`-exclusion case · fleet-table case.

### Slice 2R — First-Contact Reception: the front desk (§9.5; field-triage: antigravity silent-bind)

*Lettered insert, deliberately: it lands **with/before the clone gate** — reception de-risks the live field friction earliest, and renumbering Slices 3–8 would ripple through every cross-reference in this PRD for zero information. Sequencing truth: the **degraded mode (§9.5.5) depends on nothing in Slices 0–2 and MAY be pulled first**; the full packet (walk-up verdict, live `bind`, spawn-backed options) completes on Slice 2 machinery, and the `create_project_brain` option becomes real when Slice 3's `init` lands.*

- **Scope:** bridge first-contact detection (trigger §9.5.2); the `M1nd-Caller-Root` forward (hop-2 gap closed, §9.5.4); owner-side packet composer (`m1nd-reception-v0` — registry read + live stats + optional last-known count fields, serde-default); the `bind` answer verb (bridge-intercepted; owner-side honest-endpoint answer for direct-HTTP); sticky per-session choice; the v1 rule-based `suggested`; the option-footer `memorize` policy calls; **agent-workflow surfaces in the SAME PR** — `M1ND_INSTRUCTIONS` §0, agent-pack/skills text, host rule files (§9.5.6); the binding-guard `suggested_fix` reception branch; the Pre-Flight binding-header data contract handshake with HUMAN-LAYER (data only, no UI).
- **RED (live-proven, already on file):** the field report verbatim — Antigravity in `~/project-d` silently attached the `~/m1nd` brain; `health` answered ok; three maintainer questions to surface the binding (`field-reports.jsonl` 2026-07-03T21:39). Mechanized: a bridge whose resolved caller root sits outside the owner's `workspace_root` forwards verbs and returns results **with no mismatch surface** (assert today's silence as the failing test).
- **GREEN:** same setup → the FIRST call returns the reception packet: `verdict: known_brain_elsewhere`, `what_exists` lists the m1nd brain with counts/freshness, every option carries its exact call, `suggested` is NOT a silent bind. `bound_match` cwd → **no interruption** (verb flows; one binding stamp). Choice sticky: the second call never re-asks. `bind(existing)` re-routes: the next `north` wears the chosen brain's fingerprint. Birth stays consented: no `brain.json` is ever created by reception itself (assert: filesystem untouched until a human/agent runs `m1nd init`). Direct-HTTP with no header → `caller_root_unknown` packet, options still listed. Legacy bridge (no header) → owner behaves exactly as today (no regression).
- **Battery:** the antigravity repro case · bound_match no-interrupt case · sticky-choice case · ambiguous nested-brain case · `caller_root_unknown` direct-HTTP case · legacy-bridge no-header case · never-births case.

### Slice 3 — `m1nd init` + selective gitignore + the clone gate (the money demo, formalized)

- **Scope:** `init` writes `brain.json` + the selective `.m1nd/.gitignore` (§5.1); foreground ingest with progress; snapshot persist; **loud commit warning + `--private`** `[C-FIX 1]`; **refuses at `$HOME`** `[C-FIX 9]`; the m1nd repo's blanket `.gitignore:46` replaced by the selective block; `battery/` committed-dir policy.
- **RED:** `git check-ignore` proves today's blanket `.m1nd/` swallows `agent-memory/x.light.md`; a fresh clone's first north carries no memory.
- **GREEN:** check-ignore battery — `.light.md`/`battery/`/`brain.json`/`.gitignore` **tracked**; snapshot/embeddings/calibration/`.locks/`/`.history/` **ignored**; same battery green in the m1nd repo. **Clone gate (formal):** fresh clone → first north returns the committed claims while `node_count == 0`, wearing `warming` — memory-before-code, mechanically proven. **Measurement gate:** record first-ingest wall time and graph-loaded RSS here (fills the two honest unknowns; RSS budget ≤500 MB).
- **Battery:** check-ignore battery · clone-gate script · `$HOME`-refusal case · `--private` check-ignore variant.

### Slice 4 — Memory hygiene: secret-scan + conflict-marker guard `[C-FIX 1, 2 — must land before cutover]`

- **Scope:** §7 whole — the scan inside `memorize` (and the future promote path), typed refusal; the marker guard at ingest and recall; the merge policy documented in the agent-pack/rule text.
- **RED:** `memorize` a `postgres://user:pass@host/db` claim → lands in a committed `.light.md` (the proven hole: PostToolUse matcher `Edit|Write|MultiEdit` never sees memorize; zero scanning in the Rust write path); a `.light.md` containing `<<<<<<<` is ingested and served in north.
- **GREEN:** memorize-with-secret → typed refusal naming the pattern class, nothing written; marker-bearing claim refused at ingest / flagged at recall and absent from north; clean claims unaffected.
- **Battery:** secret corpus (credentialed URLs, cloud keys, tokens, private-key headers) refused · marker corpus refused/flagged · false-positive canary (prose mentioning the word "password" passes).

### Slice 5 — `brain_kind` + medulla verb gate + registry unification

*(Pointer, 2026-07-05: the MEMORY layer of the 5–7 block — claim state machine, storage split + M4-triage deepening, `tier`-scoped pull-only recall, the `promote` verb, delegation-packet memory slice, the misdelivery telemetry class + per-project mailbox boxes — is spec'd whole at **`docs/MEDULLA-PRD.md`** as lettered slices **M5a · M5b · M6 · M7 · M7b**, the 2R insert precedent; Slices 5–8 below keep their numbers and their process/registry/hook scope unchanged.)*

- **Scope:** the optional `brain_kind` registry field (serde default `"project"`); the medulla's `brain.json` set `"medulla"`; typed refusal of `ingest`/`federate`/`federate_auto`/`auto_ingest_*` naming the project brain; discovery-by-kind + bridge keyword `--attach medulla` (registry-resolved, literal `:1338` fallback); medulla launchd unit re-pointed at the unified `~/.m1nd/registry`. `[KILLED: the directory move.]`
- **RED:** the medulla accepts `ingest` today (assert it — the structural violation is possible); the medulla is discoverable only via a hardcoded address.
- **GREEN:** typed refusal with the redirect; registry entry carries `brain_kind`; `--attach medulla` resolves via the unified registry; legacy entries still parse.
- **Battery:** the medulla doctrine-battery seed — `north` returns doctrine · `ingest` refused typed · legacy-entry parse case.

### Slice 6 — Hooks v2 + config cutover, doctrine continuity gated `[C-FIX 3, 5]`

- **Scope:** Claude hook v2 = spawn-and-return + the two-call compose (§10.1); Codex SessionStart gains the same compose; **retire `~/.codex/bin/m1nd-stdio-session.sh`**; `mcp-config`/`hosts apply` default emit `--attach auto` + `M1ND_WORKSPACE_ROOT` + `M1ND_AGENT_ID` (`--project` pins `M1ND_RUNTIME_DIR`; `--embedded` legacy); agent-pack/rule-file two-tier text; the HUMAN-LAYER S5 → "Project Brain Panel" doc line (§4.3).
- **RED:** `grep 1338 ~/.claude/hooks/m1nd-north.sh` hits as the only resolution path; Codex SessionStart lacks north; `hosts status` shows embedded entries; the live cwd failure (`~/m1nd-l00p` handshake = `wrong_workspace_binding`).
- **GREEN — the grounding's live failure, inverted:** parallel sessions in `~/m1nd` and `~/m1nd-l00p` return each repo's own anchors / medulla-only + `project_brain_absent` respectively, and **no `wrong_workspace_binding`**. **Doctrine-continuity gate:** a post-cutover session in a brainless repo still receives the doctrine block (the channel the old hook provided may not regress). `hosts status` shows `--attach auto` ×4; the hook's medulla resolution is registry-by-kind with the literal as fallback only.
- **Battery:** the cwd-matrix probe script (four hosts × two repos) · doctrine-continuity case · a bounce-under-live-session case for the #225 re-init claim (see risk §21.14).

### Slice 7 — Migration (the medulla split), mechanized equivalence gate `[C-FIX 7]`

- **Scope:** execute the §16 runbook M0→M6.
- **RED (live-proven):** `session_handshake` for `~/m1nd-l00p` against the served owner = `wrong_workspace_binding`; the medulla holds m1nd-code claims and the m1nd ingest root.
- **GREEN — the equivalence gate, mechanized or it is theater:** the **frozen 10-probe set** (seek/why/impact about m1nd code + doctrine probes), with **defined compared fields** (claim slugs found; anchor file paths), and the **baseline JSON committed BEFORE migration starts** — ⊇ becomes set inclusion checked by script, not vibes. Plus: claim-count conservation (`count(baseline) == count(project-active) + count(medulla-active)`); the l00p repro clean; the four-host matrix green.
- **Battery:** the frozen probe set + comparison script land in the m1nd project brain's `battery/`.

### Slice 8 — Calibration re-arm + soak + reclaim

- **Scope:** archive the mixed calibration row (n=9196, measured over a mixed graph — invalid for either tier); each brain re-arms `calibrate_predict` from n=0; 7-day soak (doctor + field reports daily); then B10 reclaim (`_expired/` 145 MB, 20 hex session runtimes, binary `.bak`s keep newest, `registry-claude/`) only after verifying no live lease references each path; cold copy retained ≥30 days on the external Cofre volume.
- **RED:** any brain wearing the inherited mixed τ.
- **GREEN:** per-brain trust rows independent, n climbing from 0; post-reclaim doctor clean and the attach matrix still green.
- **Battery:** calibration-independence probe · doctor-warns-at->5-owners case.

---

## 15. V2 backlog (proof-gated — each needs its own RED→GREEN before "done") & the kill list

**V2 backlog:**

1. **Server-side federated north** (§10.2) — with the negative cache/circuit breaker `[C-FIX 6]`, TTL + labeled staleness, chaos gates (medulla killed → project-only within the 1 s timeout; brainless → medulla-only), and a latency gate (warm composed north p95 ≤ 2 s over a 50-call loop).
2. **`memorize --promote` mechanization** (§8) — provenance stamped automatically; maker refusal typed; `learn wrong` kill-path gated.
3. **Per-brain batteries in CI** — the m1nd repo's CI runs its project brain's battery; the medulla doctrine-battery runs on the doctor schedule.
4. **Removal ladder `brain clean` / `brain eject`** (§20) — v1 ships `stop` only.
5. **`doctor --bounce [<repo>|--all]`** — fleet convergence with sessions riding #225; gate: bounce-all under a live session leaves N fresh entries, session uninterrupted.
6. **Swarm roster** (B8) — an honest HYPOTHESIS: `swarm:[{agent_id, last_call_age_s, mission?}]` composed from call provenance + open `mission_*` state; visibility, never mutexes. Not real until it has a slice and a gate (TT-INV-10).
7. **Reception V2** (§9.5) — the policy-consulting composer (`suggested` moves on medulla `reception_policy_*` claims, riding item 1's federated-read + negative cache); `roots/list` refinement of `resolved_via` where hosts prove the capability; mid-session cwd-travel re-detection (v1 honestly leaves it to the scope-guard backstop); `relevant_medulla_memories` in `honest_gaps`.

**KILLED (for the record, with reasons):** B4's wake-persist-exit special case (plain idle-exit suffices; the special case guaranteed cold mornings) · the S6 `runtimes/claude → ~/.m1nd/medulla` move + compat symlink (zero user value, real breakage risk; the registry re-point achieves Q6) · Slice-5 cache/TTL/stale machinery in v1 (deferred whole with the slice) · promotion mechanization in v1 (manual, documented) · `doctor --bounce` in v1 · `brain clean`/`eject` in v1 · auto-field-report-on-corruption (telemetry stays a doctrine act) · `hosts apply --remove` polish · the "~90% composes" figure (unfalsifiable) · "structurally impossible" language (corrected to "impossible via the default path" everywhere).

---

## 16. Migration runbook — reversible, proof at each step

**Pre-state, verified this hour:** medulla `:1338` on binary f737931 (swapped 16:44 today); RSS 170 MB holding **memory only** (the code graph was destroyed by the 16:44 restart — there is *less* to migrate than assumed); 8 `.light.md` claims *(measured; growing — triage from live enumeration)*; `calibration_state.json` n=9196 (mixed, invalid to carry); live lease in `registry-claude/` carrying the `workspace_root:"/"` evidence; `registry/leases/` holds 1 json + 7 leaked `.tmp`; 21 runtime dirs incl. 145 MB `_expired/`; the hook hardcodes `:1338`.

**Zero-data-loss doctrine (binding for every step):** nothing is deleted until M8, after soak, and then only artifacts that are (a) inside the cold copy AND (b) rebuildable. Claims are *never* deleted anywhere — supersession only, `.history/` preserved. Deliberately not carried forward: the mixed calibration row and the embedding cache — both archived; the live system already proved the cache is volatile (it self-destructed today).

| Step | ACTION | PROVE (gate to proceed) | ROLLBACK |
|---|---|---|---|
| **M0 — Freeze + cold copy + frozen baseline** | Capture the baseline against the served owner: the **frozen 10-probe set with defined extraction fields, committed as JSON** `[C-FIX 7]` + `boot_memory` list + `ls agent-memory/` + a copy of the calibration file → `~/.m1nd/migration/<date>/baseline/`. Then `launchctl bootout` the serve unit and `cp -a ~/.m1nd ~/.m1nd.pre-two-tier.<date>` (~1.0 GB *(measured)*); plist + hook copied alongside. | Baseline JSON readable + committed; `diff -rq` spot-check on the copy. | Bootstrap the old plist — nothing has changed yet. |
| **M1 — Medulla persistence fix** (= Slice 0 applied live) | Plist gains `WorkingDirectory` + absolute `--graph`; binary carries the runtime-root-resolved persist fix. | Zero `os error 30` after a `persist`; kill + restart logs "loaded snapshot", not "starting fresh"; the new lease's `workspace_root` ≠ `"/"`. | Restore the M0 plist. |
| **M2 — Selective gitignore in `~/m1nd`** | Replace blanket `.gitignore:46` with the §5.1 block. | `git check-ignore` battery green. | `git revert`. |
| **M3 — Birth of the m1nd project brain** | `cd ~/m1nd && m1nd init` → fresh ingest (~6.2k nodes expected), fresh embeddings (the 82 KB corpse is not carried), snapshot persisted. *This re-creates what the 16:44 restart destroyed — the first concrete repair the new architecture performs.* | Code-probe half of the baseline answered ⊇ by the project brain **via the committed comparison script**; second spawn warm within the measured budget; registry entry `brain_kind=project`; **record first-ingest wall time + graph-loaded RSS** (TT-INV-5). | `m1nd brain stop ~/m1nd` + remove `~/m1nd/.m1nd/` working copy (manual in v1 — eject is V2); nothing else touched. |
| **M4 — Memory triage** (the only hand-curated judgment) | Enumerate ALL claims at cutover (8 today, growing). Write the triage table (claim → tier → new slug), then: repo-specific → `memorize` into the project brain with provenance, then supersede on the medulla with a `moved_to:` pointer. Doctrine stays. | `count(baseline) == count(project-active) + count(medulla-active)`; every claim `seek`-able at exactly one live tier; `.history/` holds every superseded original. | Supersession is non-destructive — reactivate from `.history/` (or the cold copy). |
| **M5 — Medulla purity** | Medulla `brain.json` → `brain_kind:"medulla"`; drop `~/m1nd` from ingest roots (persistable now, thanks to M1); restart; verb gate live. | `ingest` on the medulla → typed refusal naming the project brain; doctrine half of the baseline still answered identically; RSS at the ~170 MB memory-only baseline *(already measured — the pure-medulla weight is known)*. | Remove the kind flag; re-ingest — the graph is rebuildable by definition. |
| **M6 — Registry unification + host cutover** (Slices 5+6 applied live) | Launchd unit gains `--registry-dir ~/.m1nd/registry`; sweep the 7 leaked `.tmp` leases; hooks v2 + `hosts apply --host all`; retire the Codex launcher script. **No directory move** `[KILLED: S6]`. | Discovery via the unified registry by kind; the l00p inversion gate (Slice 7 RED→GREEN); doctrine-continuity gate; `hosts status` ×4; `grep 1338` in the hook hits only the fallback line. | One line per host back to the literal `:1338` (≤5 min); old `--registry-dir` restored. |
| **M7 — Calibration re-arm** | Archive the mixed row to `~/.m1nd/migration/<date>/archive/calibration_state.mixed.json`; each brain runs `calibrate_predict` from n=0. | Per-brain trust rows independent, n climbing; no brain wears the inherited τ. | Restore nothing — re-arming is forward-only by design; the archive preserves the row for forensics. |
| **M8 — Soak 7 days → reclaim** (B10) | Watch field-reports + `m1nd doctor` daily. Then, verifying no live lease references each path: rm `_expired/` (145 MB), the 20 hex session dirs, `bin/*.bak` except newest (~290 MB), `registry-claude/`. Cold copy → the external Cofre volume, kept ≥30 days. | Doctor clean daily through soak; post-reclaim attach matrix green. | The cold copy IS the rollback for anything reclaimed in error. |

**Total rollback at ANY step, ≤5 minutes:** bootout new units → `mv ~/.m1nd ~/.m1nd.two-tier-failed && cp -a ~/.m1nd.pre-two-tier.<date> ~/.m1nd` → restore plist + hook from M0 → bootstrap → revert the four host-config lines. Back to today's world exactly.

```mermaid
stateDiagram-v2
    [*] --> M0
    M0: M0 freeze + cold copy + frozen baseline JSON
    M1: M1 medulla persistence fix (Slice 0 live)
    M2: M2 selective gitignore in ~/m1nd
    M3: M3 m1nd init — project brain born (fresh ingest)
    M4: M4 memory triage (count-conserving)
    M5: M5 medulla purity (brain_kind + verb gate + root dropped)
    M6: M6 registry unification + hooks v2 host cutover
    M7: M7 calibration re-arm (n=0 per brain)
    SOAK: M8 soak — 7 days, doctor + field reports daily
    RECLAIMED: reclaim debris (145 MB _expired, baks, registry-claude)
    ROLLBACK: pre-two-tier restored (≤5 min from any step)
    M0 --> M1
    M1 --> M2
    M2 --> M3
    M3 --> M4
    M4 --> M5
    M5 --> M6
    M6 --> M7
    M7 --> SOAK
    SOAK --> RECLAIMED
    M1 --> ROLLBACK
    M3 --> ROLLBACK
    M5 --> ROLLBACK
    M6 --> ROLLBACK
    RECLAIMED --> [*]
    ROLLBACK --> [*]
```

---

## 17. Failure-mode UX — the honest degradation grammar

**Four laws.** (1) Fail-open is sacred (TT-INV-1). (2) ONE next action per degraded envelope (TT-INV-4). (3) Absent ≠ wrong (TT-INV-2). (4) **Self-heal first, prescribe second, never auto-kill** — exactly one failure asks a human for anything.

| # | Failure | Detection | Automatic behavior | Maintainer sees | THE one action |
|---|---|---|---|---|---|
| 1 | Project owner down (idle-exit/crash) | discovery miss; mid-session `-32001` | respawn warm; live bridges ride #225 re-init (PROVEN by #233's restart tests — see risk §21.14) | nothing, or one `warming` beat | none |
| 2 | Owner **wedged** (alive, lease held, heartbeat stale) | spawn loses lease AND discovery filters stale; retry window expires | degrade to medulla + `gaps:[project_owner_wedged{pid}]` | `trust=degraded · project owner wedged (pid N)` | `m1nd doctor --repo .` → prints the verified `kill -TERM N`; **maintainer approves** (B7) |
| 3 | Spawn race (two sessions, one brain) | lease arbitration | loser exits 0; bridge re-discovers, attaches winner | nothing — invisible by design | none (Slice 2 gate proves ×20) |
| 4 | Registry stale entry | `owner_live && !stale` filter; boot GC | filtered; post-wake heartbeat refreshes ≤5 s | nothing | none |
| 5 | **Medulla unreachable** | doctrine beat 1 s hard timeout | project north + `gaps:[medulla_unreachable]`; v1 has no doctrine cache (V2 adds the labeled stale cache + 60 s negative cache) | `doctrine: unavailable this beat` | `launchctl kickstart gui/$UID/com.m1nd.serve` (doctor prints it verbatim) |
| 6 | Port collision | project tier: **deleted by design** (`--port 0`); medulla `:1338` squatted → launchd flaps | collapses into #5 | as #5 | doctor names the squatter PID + the kickstart |
| 7 | Snapshot corrupt | load error at spawn | rename to `graph_snapshot.corrupt.<ts>.json` (evidence kept), boot memory-first, auto-warm (B1); `snapshot_corrupt_recovered` worn in north; doctor lists corpses. `[KILLED: the auto-filed field report — reporting stays a doctrine act.]` | one `warming` north | none — self-heals; the corpse feeds triage |
| 8 | Binary drift | `expected_sha` vs `current_exe` (`main.rs:293-322`) | advisory (default): spawn + drift worn in every fingerprint (live today: `binary_lags_repo:true`). `strict_binary:true`: refuse to spawn, medulla-only + `gaps:[binary_drift_blocked]` | drift line in north | doctor prints the exact reinstall / deliberate re-pin |
| 9 | **Persistence degraded** (the live os-30 class) | persist failure at write time | keep serving from RAM — never stop; `persistence_degraded:true` in every handshake/north (Slice 0) | `⚠ not persisting — warm boots off until fixed` | doctor prints the failing path + permission check |
| 10 | Both tiers down | connect errors everywhere | hook exits 0 silently; in-session bridge returns ONE typed error through `recovery_playbook` | a bare session | `m1nd doctor` |

The maintainer's view of any failure is always the same shape: **one calm line in the packet, one command to run.** The agent's view is the full typed envelope. Nobody ever waits.

---

## 18. Upgrades & resource discipline

### 18.1 Upgrade choreography (B5, amended)

Install replaces `~/.m1nd/bin/m1nd-mcp` keeping ONE `.bak` (today's 6 baks/~350 MB *(measured)* get reclaimed at M8; doctor flags the excess).

1. **Medulla first:** `launchctl kickstart -k` — restarts on the new binary; attached sessions ride #225's transparent re-init (one `-32001` re-handshake) — **PROVEN by #233's `bridge_survives_two_restarts_including_binary_swap` before cutover leans on it** (risk §21.14).
2. **Project brains converge lazily:** owners are ephemeral — idle-exit retires the old binary; the next spawn *is* the upgrade. Zero fleet orchestration (the Nx model). Immediate convergence in v1: `m1nd brain stop <repo>` + next call. (`doctor --bounce` is V2.)
3. **Skew, worn honestly:** bridge↔owner skew surfaces in the binding (`owner_version_skew`), tolerated while the MCP handshake succeeds. Owner↔medulla skew rides the stable `seek`/`boot_memory` verb contract; every composed north carries both tiers' fingerprints. **Snapshots are caches, never contracts:** version mismatch at load takes the corruption branch — re-ingest, never a format migration, never a crash.
4. **Pins:** advisory by default; `strict_binary:true` for determinism-demanding repos (CI battery repos) — refuse-to-spawn is fail-closed and never redirects. A heterogeneous fleet (repo A pinned old, repo B newest) is a *supported state*: that is precisely what per-brain runtimes buy.

### 18.2 Resource discipline — measured, with the honest unknowns named

| Thing | Value |
|---|---|
| Medulla RSS (memory-only, ~124 nodes) | **170 MB** *(measured)* — the pure-medulla steady state post-M5, known in advance |
| Attach bridge RSS | **~6 MB** each *(measured — three live)* — sessions are effectively free |
| Embedding cache at 6.2k nodes | **6.2 MB** *(measured earlier today, pre-wipe)* |
| Binary on disk | 59 MB (+ ~290 MB reclaimable `.bak`s) *(measured)* |
| `~/.m1nd` total / runtimes / `_expired` | 1.0 GB / 180 MB / **145 MB reclaimable** *(measured)* |
| Graph-loaded owner RSS (6.2k nodes) | **honest unknown** — destroyed before capture by today's restart; **measured at the Slice 3 gate**, budget ≤500 MB (TT-INV-5) |
| First-ingest wall time (6k-node repo) | **honest unknown** — embedding-dominated; **measured at the Slice 3 gate** |

**Policy:** idle-exit 1800 s is the governor; expected steady state 1–3 resident owners + the medulla (an orchestra day peaks ~4–5; doctor warns >5). **Sleep, stated honestly (B4 amended):** owners that survive sleep give a warm morning (first touch = registry hit); owners that accumulated 30 awake-idle minutes exited and give a doctrine-only first beat. launchd is for the medulla only; project owners are demand-driven, always. **Worktrees:** each worktree carries the committed `.m1nd/` → its own owner over its own branch's graph (runtime files are per-worktree because gitignored). Never share a runtime across branches — that re-creates the mixed-graph disease.

---

## 19. Multi-agent choreography

> **[A2 · pointer to `docs/ORGANISM-PRD.md` §C2.2 · §C7.5]** The letter grammar (derived fates `wet_ink | in_flight | fired_clay | external`; `rejected`/`expired` → receipt **dispositions**, not fates) and box-birth consent (consent-deferred: local box now, git-travel only after `m1nd init`'s loud warning) are **constitutional law** at §C2.2 and §C7.5. `docs/MEDULLA-PRD.md` §9.2 stays the mechanism spec; where it disagrees, the constitution wins.

**Scene:** one repo, simultaneously — `claude:orchestrator`, `codex:maker`, `antigravity:reviewer`, plus subagents. All bridges walk up to the same `brain.json` → same `runtime_root` → **the same owner process**. One graph, one memory, one calibration. Writes serialize inside the single owner at call granularity — the ReadWrite lease means exactly one writer process *exists*, so cross-process write races are impossible by construction, not by discipline.

- **What each hand sees:** the identical composed north. What differs is provenance: every call stamped by `M1ND_AGENT_ID`. A `memorize` by Codex is `seek`-able by Claude on the next call — same process RAM, zero sync machinery.
- **Subagents:** inherit the dual-tier Delegation Packet (§11) and attach the same brain from the same cwd; their `memorize` lands in the project tier under their own agent_id; promotion is proposed, never executed, by makers (§8).
- **Coordination visibility:** the swarm roster is a V2 hypothesis (§15) — v1 ships provenance-stamped history and the fleet single-pane, nothing more, because nothing more has a gate.
- **Field telemetry — DECIDED (B3): the global mailbox stays** *as the WRITE path* — **amended 2026-07-05 (maintainer direction): letters are additionally FILED into repo-side per-project boxes** (`<repo>/.m1nd/inbox.jsonl`, the box is the project's property and travels with git; the medulla box holds only genuinely projectless letters; `inbox_sweep` reads spool ∪ boxes so triage keeps the whole view — spec'd at `docs/MEDULLA-PRD.md` §9.2, human face at HUMAN-LAYER §4A.11). One file, `~/.m1nd/field-reports.jsonl`, remains every agent's write surface and the never-truncated archive. Argued then, still true where it matters: (a) reports are about m1nd-the-product, not the project — their one consumer WAS the improvement loop, which sweeps ONE file, and the maintainer's doctrine hardcodes that path into every agent's standing instructions (30 live reports prove the loop works *(measured)*); (b) reports must *outlive* project brains — a bug witnessed in one repo must survive that repo's eject, because it is calibration ground truth about m1nd (the spool archive carries this); (c) ~~per-project mailboxes buy a collector, a doctrine rewrite, and no new consumer~~ — **premise retired 2026-07-05: the Hall's per-project Mailbox IS the new consumer, and the write doctrine is untouched (zero rewrite)**; (d) O_APPEND JSONL is race-safe at this volume. **Adaptation:** reports gain `"brain": "project:<name>"|"medulla"` — per-brain ground truth becomes a filter for free. Schema stays free-form; nothing blocks on the new field.

---

## 20. Eject & uninstall — the afterlife

**v1 ships `m1nd brain stop` only** (§5.3). The full ladder is V2, specified here so it is built once:

1. **`stop`** — SIGTERM (persists on the way down), lease released. Nothing deleted; next call resurrects warm.
2. **`clean`** (V2) — stop + delete the **metabolism only** (the gitignored set: snapshot, embeddings, calibration, caches, `.locks/`, `.history/`). The committed brain — `brain.json`, `agent-memory/`, `battery/`, `.gitignore` — is untouched. The local factory reset; the standing prescription for "the brain is acting weird".
3. **`eject`** (V2) — stop + clean + delete the committed brain files + drop registry entries + remove `--project` runtime pins from host configs. **Guard:** refuses on uncommitted `agent-memory/` changes unless `--force`, and prints the git truth before acting.

**What git carries away:** eject removes the working copy only. Every clone and every commit still holds the brain — `git checkout <sha> -- .m1nd/` resurrects its mind whole, and the runtime rebuilds around it. **Memory is never truly lost while git lives.** Stated with equal honesty (TT-INV-9): eject does **not** purge history — a secret leaked into a claim is a git-history-scrub job plus rotation, never an eject; the §7.1 write-path scan exists to make that event rare, not impossible.

**The privacy boundary coincides with the tier boundary** — said once, plainly: maintainer doctrine and preferences live only in the medulla and reach project sessions by **read**, composed in RAM, never persisted into `<repo>/.m1nd/`. Pushing a repo can expose project claims only — never the maintainer's cross-project mind. Promotion is the single deliberate crossing, upward only, provenance-stamped.

**Full machine uninstall:** bootout + rm the plist; archive the medulla's mind (`cp -a` its `agent-memory/` — plain markdown, no export tool needed); remove host entries + hooks by hand (an `hosts apply --remove` convenience was `[KILLED]` as polish); `npm rm -g` the CLI; `rm -rf ~/.m1nd`. Repos keep their `.m1nd/` — inert data (§9.4), harmless without the binary, alive again the day m1nd returns.

---

## 21. Open risks (named, not waved away)

1. **`agent_id` is self-declared.** Promotion gating, provenance, and any future roster are convention + audit, not enforcement (TT-INV-7). A misbehaving agent can misattribute; provenance makes it visible after the fact only.
2. **The secret-scan is pattern-based → false negatives exist.** Committed memory in **public** repos remains a human policy call: the init warning and `--private` are the layers above the floor; residual risk stays open until real-world soak.
3. **Zombie claims across machines.** `.history/` doesn't travel, so supersession forks silently; a merge can resurrect a superseded claim (§7.3). Mitigated (marker guard, `learn wrong` on sight), not solved — cross-machine supersession sync is future design work.
4. **Medulla-down latency burn (v1):** every session start pays the 1 s doctrine timeout while the medulla is down — bounded per-session, not per-call; the V2 negative cache removes it. Accepted for v1.
5. **Morning cold quality:** the first beat of a drained repo is doctrine-only; if the Slice 3 snapshot-boot measurement lands worse than the ≤2.5 s budget, the warm story (and this PRD's prose) must be re-cut to the measured number — TT-INV-5 makes that mandatory, not optional.
6. **First-ingest wall time is unknown** (embedding-dominated). Foreground `init` on a large repo may be minutes; the progress display is the only mitigation shipped in v1.
7. **Graph-loaded RSS is unknown** (budget ≤500 MB, measured at Slice 3). A many-worktree orchestra day could stress RAM; idle-exit + the >5-owner doctor warning are the governors; unproven at scale.
8. **`strict_binary` pin churn:** strict repos demand a deliberate re-pin after every upgrade — accepted friction, fail-closed by design.
9. **Cross-project code query is forfeited** (§2, adopted posture 4). The throwaway-federate-brain escape hatch is documented; if the need becomes chronic, that is a v3 design question, not a medulla exception.
10. **Single unified registry under more writers:** existing lease/GC machinery is assumed sufficient; 7 leaked `.tmp` leases were already observed live *(measured)* and are swept at M6 — watch during soak.
11. **Hosts without SessionStart** (Gemini/Antigravity): north-on-first-tool-call only; north-before-first-token stays an honest gap on those hosts.
12. **The medulla claim set grows daily** (8 at write time). M4 triage must enumerate at cutover — this document's inventory is already stale by design.
13. **Nested-brain edge cases:** nearest-wins is the rule; symlinked/worktree layouts can still surprise — the binding guard backstop and the `--project` pin are the escape valves.
14. **The #225 transparent-re-init claim now has its gate — PROVEN.** During this PRD's own writing session, two `north` calls against the live served owner returned `-32001 Unknown or expired Mcp-Session-Id` with **no transparent recovery at the tool surface** (the session's MCP handle predated that day's 16:44 medulla restart; retry did not recover; field-reported — mailbox line 30). PR #233 closed it: `tests/attach_reinit.rs::bridge_survives_two_restarts_including_binary_swap` drives the SAME bridge session through two consecutive owner restarts (incl. a full kill / binary-swap) and asserts transparent recovery + a fresh session id each time, and `owner_unknown_session_wire_shape_is_recoverable` locks the owner's unknown-session wire shape to the re-init trigger. "Sessions ride #225" is now a PROVEN fact under those tests, no longer a budgeted claim (TT-INV-5 discharged for this resilience claim).
15. **Reception trust rides the first hop (§9.5).** `M1nd-Caller-Root` is self-declared by the bridge, exactly as env pins are self-declared by hosts — a lying or misconfigured bridge misbinds with a confident packet. Same trust class as `agent_id` (TT-INV-7): audit, not enforcement; the scope-guard backstop and the packet's evidence paths make it inspectable. Also honest: v1 does not re-detect mid-session cwd travel (§21's scope-guard backstop covers it), and hosts render `initialize` instructions unevenly — the packet-as-first-result carrier exists precisely because instructions alone cannot be trusted to be read.

---

## Appendix A — verified contract index

All code anchors verified at `origin/main` @ `f737931` (repo HEAD at landing: `98b745a`, docs-only delta). **The symbol is the contract, the line is a hint — re-anchor at implementation start.**

| Contract | Where |
|---|---|
| Registry entry schema / lease acquire / state-file enumeration / boot GC / endpoint readback / discovery / coexistence tests | `m1nd-mcp/src/instance_registry.rs:19-39 · 134-220 · 348-362 · 391-426 · 645-650 · 673-713 · 725, 881` |
| Strict version enforcement / serve boot | `m1nd-mcp/src/main.rs:56-90 · 275-340 · 293-322` |
| Workspace-root env candidates / binding guard / nested guard / state loads (embeddings, calibration) | `m1nd-mcp/src/session.rs:450 · 475-503 · 589 · 747-836 · 1296-1342 · 1379-1500 · 1409-1413 · 1498` |
| Attach URL override / bridge CLI | `m1nd-mcp/src/cli.rs:27-33 · 88-99` |
| HTTP attach client (reused for doctrine beat / V2 federated read / V2 promote) | `m1nd-mcp/src/attach_client.rs` |
| North memory-beat merge seam (V2 third feed lands here) | `m1nd-mcp/src/server.rs:2949-3128` (merge vec at `:3120`) |
| `federate` / `federate_auto` (id-prefixing; the §2 escape hatch) | `m1nd-mcp/src/layer_handlers.rs:5972+ · 6050` / `audit_handlers.rs:2088+` |
| L1GHT ingest walker (the §7.2 guard site) | `m1nd-mcp/src/tools.rs:374-408` |
| npm CLI: init / mcp-config emit / install-skills / host config writer | `npm/lib/cli.js:26-31 · 481-509 · 606-616 · 649-668 · 1043-1073` |
| Gitignore precedents (root snapshot / embeddings / blanket `.m1nd/` / selective memory / battery) | `.gitignore:23 · 28-31 · 46 · 48-54 · 56-60` |
| S5 naming (→ "Project Brain Panel") | `docs/HUMAN-LAYER-PRD.md` §4 (S5 row) |
| Owner-restart re-init | PR #225 (`c797714`) — see risk §21.14 |
| **Reception anchors (§9.5 — verified at `6c53c47`)** | `M1ND_INSTRUCTIONS` const `server.rs:33-89` · attach request headers (the hop-2 gap) `attach_client.rs:248-260` · wire-session mint at `initialize` `mcp_http.rs:395-448` · registry enumeration `instance_registry.rs:281` (`list_instances`) · handshake carries the binding fingerprint `server.rs:5914` (test) · guard `suggested_fix` migration sites `session.rs:765` / `tools.rs:3545, 3861` · workspace resolved from owner-side env+cwd `session.rs:1269` |
| Live artifacts (probed 2026-07-03) | `~/Library/LaunchAgents/com.m1nd.serve.plist` (no `WorkingDirectory`) · `~/.claude/hooks/m1nd-north.sh:10` · `~/.claude/hooks/verify-edit.sh` (matcher `Edit\|Write\|MultiEdit`) · `~/.codex/bin/m1nd-stdio-session.sh:23-27` · `~/.m1nd/registry-claude/leases/cdd1c483184e7cb0.json` (`workspace_root:"/"`) · `~/.m1nd/serve-claude.err.log` (os error 30) · `~/.m1nd/runtimes/claude/agent-memory/` (8 claims) · `~/.m1nd/field-reports.jsonl` (30 lines; the reception RED is the 21:39 antigravity line) |
