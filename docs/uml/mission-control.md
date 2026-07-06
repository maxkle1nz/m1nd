# mission-control

A bounded, repo-scoped agent-mission state machine: `mission_start/event/next/verify/handoff/close` persist a per-mission JSON contract to disk and hand the agent one next move plus an evidence-class gate, so a subagent's scoped mission ends in a code-anchored proof packet — deliberately reserved for SubagentStop, never the default Stop loop.

## Class/Component

```mermaid
classDiagram
    class MissionState {
        +String mission_id  "msn_<ts>_<slug>"
        +String agent_id  "owner"
        +String repo, task
        +String mode, budget, risk
        +String phase  "locate→...→closed"
        +String status  "active|closed"
        +Vec events  "unbounded ledger"
        +Vec claims  "MissionClaimState"
        +graph_state_at_start  "stale snapshot"
        +context_guard_at_start
    }
    class MissionClaimState {
        +String claim
        +String verdict
        +String evidence_grade  "direct|graph_only|inferred"
    }
    class Handlers~mission_handlers.rs~ {
        handle_mission_start :71
        handle_mission_event :132
        handle_mission_next :165
        handle_mission_verify :200  "the gate"
        handle_mission_handoff :262
        handle_mission_close :309
    }
    class Persistence {
        save_mission :455  "fs::write truncate-in-place"
        load_mission :463
        mission_path :450  "runtime_root/mission-control/<id>.json"
        validate_mission_id :472  "msn_* no path-sep"
        ensure_agent :487  "owner-only mutate"
    }
    class EvidenceGate {
        classify_evidence :939
        is_direct_kind :996  "grep/view/file_read/test_run..."
        is_graph_kind :989  "activate/seek/audit"
        event_is_referenced :979
        is_coverage_sweep_kind :1008
    }
    class Planner {
        analyze_events :808
        next_move :844  "exactly one move + do_not[]"
        budget_consumed :1029  "clamp [0,1]"
    }
    class LightSeam {
        try_write_light_memory :386
        "→ light_author_handlers::handle_light_author"
        "failure-tolerant"
    }
    class ServerDispatch~server.rs~ {
        "dispatch arms :4257-4286"
        "tool schemas :2031-2170"
        "auto-ingest tick EXEMPTS mission_* :3996"
        "ESSENTIAL: start/next/close :406-408"
        "READ_ONLY_DENIED_TOOLS: NO mission_* (gap)"
    }
    class Inputs~protocol/layers.rs~ {
        MissionStartInput :3048
        "defaults mode=review budget=normal risk=medium"
        MissionCloseInput.write_light_memory:bool :3099
    }

    Handlers --> MissionState : builds/mutates
    Handlers --> Persistence : load→mutate→save
    Handlers --> EvidenceGate : verify uses
    Handlers --> Planner : next uses
    Handlers --> LightSeam : close(write_light_memory)
    ServerDispatch --> Handlers : deserialize + call
    Inputs --> ServerDispatch : typed params
    MissionState --> MissionClaimState : records
```

## Sequence

```mermaid
sequenceDiagram
    participant A as Subagent
    participant S as server.rs dispatch
    participant H as mission_handlers
    participant D as Disk (mission-control/<id>.json)
    participant L as light_author

    Note over A,S: (auto-ingest tick is SKIPPED for all mission_* :3996)
    A->>S: mission_start{repo,task,mode,budget,risk}
    S->>H: handle_mission_start :71
    H->>H: validate mode/budget/risk enums :501<br/>route_for + budget_envelope<br/>snapshot graph_state + context_guard
    H->>D: save_mission (phase=locate, status=active)
    H-->>A: mission_id + route + starter_moves + non_claims

    loop investigation
        A->>H: mission_event{action}
        H->>D: load (ensure_agent) → append_event<br/>(auto evidence_class) → save
        H-->>A: event_id + digest + budget_consumed
        A->>H: mission_next
        H->>H: analyze_events + next_move
        H-->>A: ONE move + do_not[] + soft_warning
    end

    A->>H: mission_verify{claim, evidence_refs}
    H->>H: classify_evidence :939
    alt grade == direct
        H-->>A: verified_for_mission (+ record claim)
    else graph_only / inferred
        H-->>A: insufficient_evidence + next_required_move=read_file
    end

    A->>H: mission_close{write_light_memory?}
    H->>D: flip status/phase=closed + assemble proof packet
    opt write_light_memory
        H->>L: try_write_light_memory (best-effort)
        alt ok
            L-->>H: .light.md path (ingested)
        else err
            L-->>H: attach light_memory_error (close still succeeds)
        end
    end
    H-->>A: proof packet (verified/rejected claims, digest, gaps)
```

## State/Flow

```mermaid
stateDiagram-v2
    [*] --> active: mission_start
    active --> active: mission_event / mission_next (phase advances locate→...)

    state verify <<choice>>
    active --> verify: mission_verify
    verify --> active: graph_only/inferred → insufficient_evidence (must read_file)
    verify --> verified: grade==direct → verified_for_mission

    state closeGate <<choice>>
    verified --> closeGate: mission_close
    closeGate --> active: bug_hunt mode + NO coverage_sweep yet → forbidden
    closeGate --> closed: review/other OR bug_hunt after coverage_sweep

    verified --> handed_off: mission_handoff (resumable packet)
    handed_off --> closed: recipient closes
    closed --> [*]
```

## Invariantes
- `mission_id` must start `msn_` and be `[A-Za-z0-9_-]` — path-escape rejected before any fs op (validate_mission_id :472; test `mission_id_rejects_path_escape` :1172). Confirmed in code.
- Only the owning `agent_id` may load-then-mutate (ensure_agent :487). Confirmed in code.
- mode ∈ {bug_hunt,review,refactor,docs_drift,architecture,release}; budget ∈ {short,normal,deep}; risk ∈ {low,medium,high} else InvalidParams (:501; test :1298).
- A claim is `verified_for_mission` ONLY when evidence grade == `direct`; graph-only/inferred → insufficient_evidence + required direct-read (:209-232; tests :1178,:1184). Confirmed via `classify_evidence`.
- Direct evidence must be RELATED to the claim — an unreferenced direct event does not upgrade a graph-only ref (event_is_referenced :979; test :1190).
- In bug_hunt mode, `next_move` forbids close after a verified claim until ≥1 coverage-sweep event exists; review/other may close immediately (tests :1311,:1335,:1356).
- 4 `DEFAULT_NON_CLAIMS` always attached; close merges+sorts+dedups extra caller non_claims (:22,:323).
- `mission_close` never fails on a light-memory write error — attaches `light_memory_error`, close still succeeds (:351-365).
- `budget_consumed` monotonic events/max_tool_calls clamped [0,1]; max floored at 1 to avoid div-by-zero (:1029).
- DOCTRINE (docs/skills, NOT code): `mission_*` is reserved for SubagentStop; the default Stop path is `cross_verify → memorize` directly (NEXTGEN-AGENT-PRD Correction 1, :428-432).

## Gaps
- **[medium] Mission write handlers absent from `READ_ONLY_DENIED_TOOLS`**: start/event/next/verify/handoff/close all `save_mission` (fs::create_dir_all + fs::write) yet a `--read-only`/`M1ND_READ_ONLY` attach can still create/mutate mission JSON. Confirmed: `grep mission … READ_ONLY` returns nothing; ingest/memorize/learn/xray_* ARE denied, mission_* silently is not. The auto-ingest exemption right beside dispatch (:3996) shows they were grouped but not added to the denylist.
- **[medium] No concurrency / lost-update protection** — **CLOSED** (hardening wave 4): every mutating handler (`mission_event`/`next`/`verify`/`handoff`/`close`) now holds a per-mission exclusive `flock` across the whole load→mutate→save, via `mission_lock` reusing the memorize `LockGuard` (generalized to `acquire_in` on `<runtime_root>/mission-control/.locks/<mission_id>.lock`). Two writers on one `mission_id` serialize instead of clobbering. RED: 40+40 concurrent event appends lost events without the lock and land all 80 with it. Residual: `save_mission` is still `fs::write` (truncate-in-place), not tmp+rename — a crash mid-write can still leave torn JSON; the lock addresses lost updates, not atomicity.
- **[medium] Keystone trigger discipline UNENFORCED in code**: nothing rejects/warns when `mission_start` runs on an ordinary Stop turn; drift back to mission-as-default-loop is prevented only by prose. Mechanizing hooks (SubagentStop → mission_verify → decision:block) explicitly deferred (NEXTGEN-AGENT-PRD Wave 6).
- **[medium] Evidence classification is lexical** — **CLOSED** (hardening wave 4): `classify_evidence` now requires a verifiable signal beyond the label. A bare direct label earns full `direct` ONLY when it cites a path that EXISTS under a repo root (`direct_ref_has_verifiable_path`) or is corroborated by a referenced recorded mission event; a label with neither grades the new `direct_unverified`, which `mission_verify` treats as `insufficient_evidence` and whose self-reported confidence is capped at 0.5. RED: a forged `file_read:src/nope.rs:1` graded direct/verified before and direct_unverified/insufficient after. The gate now checks the act, not just the label.
- **[low] No listing / expiry / GC / size bound**: mission JSON accumulates forever, `events` Vec uncapped; no enumeration, TTL, or delete verb.
- **[low] No Rust handler-level round-trip test** through real disk save/load — the 14 unit tests hand-build MissionState and call pure helpers; the only round-level test is Python (counts calls).
- **[low] Start-time snapshots never re-checked**: `graph_state_at_start` / `context_guard_at_start` captured once (:104-105) and echoed verbatim at close; a mission that started with node_count==0 or unverified binding carries the stale snapshot into the close packet.
```
