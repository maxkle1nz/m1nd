# The Organism From Inside + 360 — UML (arc sheet, DRAFT)

**The four new wires drawn against the machinery that exists** — the immune cycle, the
process-memory gauntlet, the presences, the provenance-honest portfolio, and the
auto-charter lifecycle with every refusal named.

> **Status:** the arc's DESIGN is RATIFIED (owner, 2026-07-12 — *"MUITO BEM! ratificado
> bora pra frente"*); this sheet stays the design-stage lens of `../ORGANISM-INSIDE-PRD.md`.
> Like `docs/uml/massif.md`, this sheet is a DESIGN-STAGE view: it draws intent against
> verified anchors; it does NOT claim the new wires exist. Grounded at `main` @ `7b3b665` + live probes of the served owner, the
> poold daemon, and the field spool (2026-07-12). Existing components carry their code
> homes; new components are marked **(new wire)**. Where this sheet and the PRD disagree,
> the PRD wins about the target and the code wins about the present.

**Code homes (existing):** the spool `~/.m1nd/field-reports.jsonl` + boxes/fates
`m1nd-mcp/src/mailbox.rs` · the letter chain `m1nd-mcp/src/mission_letter.rs` (7-phase
enum at `:83`) + `mission_letter_handlers.rs` · the spawner proxy + runnerd registry
`server.rs` / `http_server.rs` (~`:334`) / `m1nd-runnerd/` · the warm pool
`god-hud/h4nd-pool/poold.py` (judge arm-flag at `:660`) + `judge.py` · the landing gate
`m1nd-mcp/src/system_blocks_handlers.rs:499` (`RECEIPT_IMPORT_HUMAN_ORIGINS`) · the
delegation debrief `m1nd-mcp/src/delegation_handlers.rs` · the audited crossing
`m1nd-mcp/src/promote_handlers.rs` (C8.2/C8.3) · the instance registry
`m1nd-mcp/src/instance_registry.rs` · renders: the Hall (`m1nd-ui`), `cockpit.rs`, the
h4nd tray. **New wires (planned):** the charter composer, the mission-close debrief
step, the presence sidecar + collision derivation, the provenance render + portfolio
view + `federation_policy` + reuse meter.

---

## 1. The four connections at one glance

```mermaid
flowchart TB
    subgraph PAIN["THE PAIN RAIL (exists)"]
        SPOOL["field-reports.jsonl<br/>115 letters, append-only"]
        BOX["per-brain boxes + fates<br/>mailbox.rs"]
    end

    subgraph C1["Connection 1 — the immune loop"]
        COMP["charter composer (new wire)<br/>eligibility + caps + dedup"]
        BOARD["mission board<br/>m1nd-mission-letter-v0 chain"]
        RUN["runnerd + warm pool<br/>worktree-per-mission, never lands"]
        JDG["judge seat (exists, DISARMED)<br/>advisory parecer, NONE-escape"]
        TRAY["tray + bell (exists)"]
        HUMAN(["THE HUMAN<br/>one stamp, one second"])
        LAND["receipt_import<br/>human-origin gate #353"]
    end

    subgraph C2["Connection 2 — the process memory"]
        DBRF["mission debrief step (new wire)"]
        GAUNT["the gauntlet (exists)<br/>C8.3 verified-only + C8.4 curator"]
        MED["MEDULLA<br/>doctrine tier, Origin-Brain"]
    end

    subgraph C3["Connection 3 — presences"]
        PRS["presence sidecar (new wire)<br/>TTL + heartbeat"]
        COLL["collision derivation (new wire)<br/>pure read-time"]
        HALL["Hall · cockpit · tray team view"]
    end

    subgraph C4["Connection 4 — federate 360"]
        PROV["provenance render (new wire)"]
        PORT["portfolio view (new wire)"]
        POL["federation_policy (new wire)<br/>isolated = mechanical refusals"]
    end

    SPOOL --> COMP --> BOARD --> RUN --> TRAY
    RUN -.->|"merge_wait head"| JDG -.->|"advisory letter"| TRAY
    TRAY --> HUMAN --> LAND
    BOARD --> DBRF --> GAUNT --> MED
    MED --> PROV --> PORT
    POL --> MED
    PRS --> COLL --> HALL
    SPOOL --> BOX
```

Reading order: the pain rail feeds connection 1; every closed mission feeds connection
2; every live session feeds connection 3; the medulla — filtered by the untouched
gauntlet — is the ONLY tissue connection 4 makes visible across brains.

## 2. Sequence — the full immune cycle (report → stamp)

The judge participates as a TRIAGER: its parecer is one more letter on the chain, phase
`judging` (the wire refused a gateless `merge_wait` parecer, live 2026-07-11), and the
bell keeps ringing on the head's `merge_wait` — smart-bells is the declared h4nd-side
dependency before the seat arms.

```mermaid
sequenceDiagram
    participant S as field spool (jsonl)
    participant CC as charter composer (new wire)
    participant OW as served owner (mission_post gates)
    participant RD as runnerd (worktree-per-mission)
    participant G as deterministic gate
    participant J as judge seat (advisory, armed by flag)
    participant T as tray + bell
    actor H as THE HUMAN
    participant RI as receipt_import (origin gate)

    S->>CC: sweep reads one letter (class, repo, tool, what)
    CC->>CC: screen — eligible class, brain resolves, gate derivable, caps, dedup
    alt refused
        CC-->>S: logged non-event (no_gate_derivable / cap_reached / duplicate_report)
    else chartered
        CC->>OW: mission_post seq 1 (judging, packet carries symptom VERBATIM + report_id)
        OW->>OW: gates — schema, unknown_block, brain_mismatch, head-CAS
        OW-->>CC: letter_id (the chain is open)
        CC->>OW: POST /api/tools/mission_spawn (owner-side secret)
        OW->>RD: /run with x-runnerd-secret (browser never holds it)
        RD->>OW: mission_post executing (seq+1, CAS)
        RD->>G: run the pinned gate_command in the isolated worktree
        alt gate exit 0
            G-->>RD: full-log hash + real execution window
            RD->>OW: mission_post merge_wait + COMPLETE receipt_candidate
        else gate fails
            RD->>OW: mission_post failed (honest, never folds)
        end
        J->>OW: read merge_wait head (sweep, rate 1 per sweep)
        J->>J: judge_verdict — NONE-escape total (engine failure = honest ABSTAIN)
        J->>OW: mission_post judging letter with verdict (advisory, never lands by guard)
        OW->>T: head still merge_wait — the bell RINGS (smart-bells keep it honest)
        T->>H: one card — candidate + the parecer gist
        H->>RI: Import this receipt (imported_via = human-ui)
        RI->>RI: origin allow-list + OCC + stale_scope + evidence + temporal gates
        RI-->>OW: store_version bumps — landed letter posted (§1d anchor)
        OW-->>T: bell silences — the cycle is CLOSED
    end
```

## 3. Flow — debrief → distillate → gauntlet → medulla

Nothing new after `memorize`: the distillate walks the SAME audited road every claim
walks. The arc adds the feeder, never a shortcut.

```mermaid
flowchart TB
    CLOSE["mission closes<br/>landed / failed / abandoned"] --> DB["debrief step (new wire)<br/>reads the REAL chain + gate artifacts"]
    DB --> DIST["distillate m1nd-mission-debrief-v0<br/>spec_quality + friction VERBATIM + ≤3 lessons"]
    DIST --> WD["memorize — the ONE write door<br/>orchestration brain, kind:process"]
    WD -->|"letter path in evidence"| REF1["REFUSED at the write door<br/>C8.5 letters are witness tissue"]
    WD --> PP["project-private claim<br/>State: authored — DECLARED tissue"]
    PP -->|"later measured confirmation<br/>or the owner's word"| VER["State: verified"]
    PP -->|"promote while unverified"| REF2["REFUSED — C8.3 gate<br/>verified-only crosses"]
    VER --> PRO["promote — the audited crossing<br/>Origin-Brain + Promoted-By + reason"]
    PRO --> REANC["C8.2 — evidence re-anchored origin-qualified<br/>or stamped evidence_unverifiable"]
    REANC --> MEDC["medulla copy — doctrine tier"]
    MEDC --> CUR["C8.4 curator consolidation<br/>evidence union · merge-and-recite · confidence cap"]
    CUR --> SEAT["seat check — grader NEVER author"]
    SEAT --> BEAT["every brain's default beat reads it<br/>the next packet carries the lesson"]
```

## 4. The presences — sessions become visible (class view)

```mermaid
classDiagram
    class PresenceRecord {
        <<m1nd-presence-v0, new wire — sidecar json>>
        +presence_id : prs_12hex
        +agent_id : String
        +kind : orchestrator|executor|pool-hand|runner|oracle|human-ui
        +brain : String (from the session's OWN binding)
        +theme : String (one line, free)
        +intent : read|mutate
        +working_set : paths and sb_ blocks (optional, honest-absent)
        +worktree : Option~String~
        +started_at +last_beat +ttl_s
        %% witness tissue — verifies nothing, gates nothing
    }

    class PresenceRegistry {
        <<new wire — the instance_registry sidecar pattern>>
        +register(record) upsert
        +beat(presence_id) refresh last_beat
        +list() only unexpired — expired = honest absence
        +gc() the existing dead-lease sweep pattern
    }

    class CollisionDerivation {
        <<pure read-time fn, new wire — never stored>>
        +derive(live) pairs where both mutate AND working sets overlap
        +advisory : warns, never blocks (the reception posture)
    }

    class session_handshake {
        <<EXISTS — tools.rs:3357, trust verb>>
        +candidate carrier of optional presence fields
        %% zero-new-verbs is the null hypothesis (C6.3)
    }

    class InstanceRegistry {
        <<EXISTS — instance_registry.rs>>
        +PID and heartbeat lease per runtime_root
        +instances json entries — the Hall's brains
    }

    class RunnerdRegistry {
        <<EXISTS — http_server.rs ~334>>
        +runner_id to port and last_seen — liveness only
    }

    class Renders {
        <<Hall strip · cockpit slot · tray team view>>
        +read-only, absent-honest, Budget Law
        +north honest-gap line on collision (both sessions)
    }

    session_handshake ..> PresenceRegistry : register + beat (reuse-first candidate)
    PresenceRegistry --> PresenceRecord : holds N
    PresenceRegistry --> CollisionDerivation : list() feeds
    CollisionDerivation --> Renders : collision blocks
    InstanceRegistry <.. PresenceRegistry : sidecar pattern borrowed
    RunnerdRegistry ..> Renders : runners join the team view
    PresenceRecord ..> Renders : roster rows (never invented)
```

## 5. Sequence — federate 360 with provenance (two brains, one medulla)

```mermaid
sequenceDiagram
    participant A as session bound to brain A (loja)
    participant PR as promote (C8.2 + C8.3, exists)
    participant M as MEDULLA (doctrine tier)
    participant POL as federation_policy (new wire)
    participant B as session bound to brain B (m1nd)
    participant RM as reuse meter (new wire)

    A->>A: claim reaches State verified (measured, June)
    A->>PR: promote {brain A, claim, reason}
    PR->>PR: C8.3 — verified-only gate passes
    PR->>PR: C8.2 — evidence re-anchored origin-qualified
    PR->>M: medulla copy + Origin-Brain loja + Promoted-By + reason
    Note over M: the project original stays — promotion ELEVATES, never moves
    B->>M: default beat — own store + medulla (pull law, unchanged)
    M->>POL: is brain A visible to this crossing
    alt owner-personal (default)
        POL-->>M: yes — all owned brains cross
        M-->>B: claim + the provenance line rendered VERBATIM
        Note over B: "promoted from loja · June · by agent — reason"
        B->>B: the pattern is APPLIED in brain B's work
        B->>RM: learn feedback not-wrong — ONE reuse event counted
    else brain A marked isolated (future client case)
        POL-->>M: refusal — excluded from recall, promote-in and portfolio
        M-->>B: claim absent + honest excluded_count (never a silent hole)
    end
```

## 6. State chart — the auto-chartered mission lifecycle (every refusal named)

Events are REAL only: a sweep read, a wire refusal, a gate exit, a parecer letter, a
human gesture. The judge's parecer moves NO phase — advisory is a self-loop.

```mermaid
stateDiagram-v2
    [*] --> spool_letter : agent appends (report-never-fix)
    spool_letter --> screening : SWEEP (composer reads)
    screening --> refused_ineligible : class not bug or honesty
    screening --> refused_no_gate : no gate derivable (NO GATE, NO CHARTER)
    screening --> refused_unknown_block : skeleton holds no such block
    screening --> refused_foreign_brain : repo resolves to no owned brain
    screening --> refused_cap : cap_reached (open autos at ceiling 3)
    screening --> refused_duplicate : fingerprint already chartered
    screening --> chartered : eligible — seq 1 judging posted
    chartered --> spawn_refused : daemon refusal (unpinned_runner etc)
    chartered --> executing : mission_spawn accepted (worktree opens)
    executing --> merge_wait : gate exit 0 — COMPLETE receipt_candidate
    executing --> failed : gate non-zero / NONE-escape abort
    merge_wait --> merge_wait : JUDGE parecer (advisory — phase unmoved, bell keeps ringing)
    merge_wait --> landed : HUMAN stamps (receipt_import, imported_via human-ui)
    merge_wait --> failed : human closes after a REJECT parecer or stale candidate
    merge_wait --> merge_wait : stale_scope on import — re-run the gate, nothing lands
    landed --> [*] : store bumped, bell silent, cycle closed
    failed --> [*] : honest, never folds
    refused_ineligible --> [*]
    refused_no_gate --> [*]
    refused_unknown_block --> [*]
    refused_foreign_brain --> [*]
    refused_cap --> [*]
    refused_duplicate --> [*]
    spawn_refused --> [*]
```

State notes: every `refused_*` is a LOGGED non-event (law 5 — refusals teach);
`refused_cap` and `refused_duplicate` are the anti-spam ceiling working, counted in §7
of the PRD; `landed` is reachable through EXACTLY ONE arrow, and it starts at the human.

---

## Invariants (the sheet's own, inherited from the PRD's laws)

- **One arrow lands.** In every diagram above, the only path into `landed`/store-bump
  passes the human's gesture through the origin-gated `receipt_import`. No component in
  any lane composes a landing (mechanical guards + grep + CI).
- **Advisory is a self-loop.** The judge's letter changes labels humans read, never the
  phase machine the wire enforces — drawn as `merge_wait --> merge_wait`.
- **The gauntlet is a straight line with two refusal exits.** C8.5 at the write door,
  C8.3 at the crossing — the process memory cannot shortcut either (diagram 3 draws both
  refusals explicitly).
- **Presences verify nothing.** No arrow leaves the presence layer toward any store,
  gate, or letter — presence renders and warns, only.
- **Provenance rides every crossing.** No arrow from medulla to a foreign session
  without the verbatim origin line (diagram 5); the isolated branch answers with a
  refusal + count, never a silent absence.
- **Real events only.** No transition in diagram 6 is driven by a timer fraction, an
  estimate, or an invented percentage — the scan-loading discipline applied to the
  charter lifecycle.

## Gaps / deferred (honest)

- **Smart-bells is not drawn as existing** — diagram 2's "smart-bells keep it honest"
  note is a DEPENDENCY on the queued h4nd slice; until it lands the judge stays disarmed
  and the sequence runs without the J lane.
- **The presence carrier is undecided by design** — diagram 4 shows `session_handshake`
  as the reuse-first CANDIDATE; the zero-new-verbs argument happens at implementation
  (C6.3), and the class relations survive either outcome.
- **The pool lane is simplified** — diagram 2 draws the runnerd lane; the h4nd poold
  claims only its reserved `sb_m1nd_pool_` namespace today (fail-closed anchor) and
  drains real missions through its own handoff spool. The charter routes through
  `mission_spawn`/runnerd in v1; pool-lane chartering is a later routing decision.
- **The reuse meter counts survival, not causation** — a learn-not-wrong event proves
  the crossed claim was not rejected, not that it caused the success; the PRD's §7 words
  the metric accordingly.
- **`judged` telemetry** (parecer counts, ABSTAIN rate, human-override rate) is drawn
  nowhere yet — it is a P2 landing requirement (PRD §7), not a diagram.

## Validation record

Every ` ```mermaid ` block in this sheet was validated with the REAL `mermaid.parse()`
(mermaid v11 under jsdom, node — the same harness pattern the 2026-07-12 atlas curation
used, generalized to all blocks per file). **Result: 6/6 blocks parse OK** (master
flowchart · immune sequence · gauntlet flow · presences class · federate sequence ·
charter state chart). House traps respected: no `;` inside sequence messages, no `.`
inside dotted-link labels.

*Authored at the Fable seat, 2026-07-12, beside `../ORGANISM-INSIDE-PRD.md`, ratified by
the owner the same day. Design-stage lens — deliberately NOT in the code-grounded atlas
index until the arc lands.*
