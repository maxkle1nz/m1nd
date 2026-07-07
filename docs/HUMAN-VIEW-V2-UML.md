# Human View v2 — UML atlas (systems and subsystems)

> Sisters: `HUMAN-VIEW-V2-PRD.md` (the contract) · `HUMAN-VIEW-V2-SCREENS.md` (the surfaces).
> Every diagram states what is NEW vs what EXISTS in the engine today. Mermaid throughout.

---

## 1. Where v2 sits in the organism

```mermaid
flowchart TB
    subgraph Human["HUMAN VIEW v2 - new front door"]
        BM["Build Map"]
        SC["Show Code"]
        RAT["Ratification"]
        RCP["Block Recipe"]
        ULM["ULM Generator"]
        AGT["Clients and Routing"]
        PIN["Pins and Missions"]
    end

    subgraph NewCore["NEW ENGINE ORGANS"]
        SBS["SystemBlock store<br/>contract, versions, drift"]
        SKE["Skeleton engine<br/>candidate pipeline"]
        RTX["Receipt taxonomy<br/>typed, expirable"]
        MIS["Mission layer<br/>packet modes and runners"]
    end

    subgraph Existing["EXISTING ORGANS - unchanged"]
        SNAP["graph snapshot with tags"]
        XRAY["x-ray paint and orient"]
        DEL["delegate and debrief"]
        LED["outcomes ledger"]
        MBX["mailbox and fates"]
        SURG["surgical context and impact"]
        LAY["layers and communities"]
        TREE["Living Tree - stays one click away"]
    end

    BM --> SBS
    BM --> SNAP
    RAT --> SBS
    SKE --> LAY
    SKE --> MIS
    SBS --> RTX
    SC --> SURG
    SC --> SNAP
    RCP --> SBS
    ULM --> SBS
    PIN --> LED
    PIN --> MBX
    MIS --> DEL
    AGT --> MIS
    XRAY --> SNAP
```

---

## 2. SystemBlock — the lifecycle (state diagram)

```mermaid
stateDiagram-v2
    [*] --> Candidate : skeleton engine proposes
    [*] --> Planned : ULM or Block Recipe creates contract
    Candidate --> Ratified : human ratifies names and boundaries - v1 signed
    Candidate --> Candidate : edit merge split before ratifying
    Planned --> Building : mission spawned from complete contract
    Building --> Scanned : code lands and scan attaches files
    Scanned --> Ratified : boundary confirmed by human
    Ratified --> Drifted : drift detection fires - new files, broken socket, vanished members
    Drifted --> Ratified : scoped re-ratification - version bumps
    Ratified --> Ratified : receipts earned or expiring - state recolors, boundary stable
```

Drift never silently re-clusters: `Drifted` reopens the ratification screen scoped to the drifted block.

---

## 3. Skeleton engine — candidate pipeline (component + sequence)

```mermaid
flowchart LR
    A["1 Scan repo<br/>EXISTS - graph ingest"] --> B["2 Cluster purpose<br/>NEW over layers plus communities"]
    B --> C["3 Name blocks<br/>NEW - agent naming via runner"]
    C --> D["4 Attach files<br/>NEW - membership many-to-many"]
    D --> E["5 Read receipts<br/>NEW rollup over existing evidence"]
    E --> F["6 Candidate map<br/>confidence, residue, seams"]
    C -.uses.-> R["agy runner<br/>fast cheap lane"]
```

```mermaid
sequenceDiagram
    participant U as Owner
    participant UI as Human View
    participant SK as Skeleton engine
    participant RN as Runner agy
    participant SB as SystemBlock store

    U->>UI: Run first scan
    UI->>SK: build candidate
    SK->>SK: cluster graph - communities, dirs, semantics
    SK->>RN: propose names and purposes for 12 clusters
    RN-->>SK: names with confidence
    SK->>SK: attach files, compute residue and seams
    SK-->>UI: candidate v0 - dashed map
    U->>UI: edit names and boundaries
    UI->>SB: ratify v1 - signed, versioned
    SB-->>UI: ratified map renders
```

---

## 4. Build Map render path (sequence)

```mermaid
sequenceDiagram
    participant UI as Build Map
    participant SB as SystemBlock store
    participant API as existing api - snapshot and tools

    UI->>SB: load ratified skeleton v1
    UI->>API: graph snapshot - nodes with tags
    UI->>UI: derive per-node states - absent tag means not scanned
    UI->>UI: rollup per block - written policy, never color average
    UI->>UI: stable layout - same block same place
    UI->>UI: render blocks, wires with edge beads, residue tray
    Note over UI: read-only path - zero engine writes to render
```

---

## 5. Receipt lifecycle (state diagram)

```mermaid
stateDiagram-v2
    [*] --> Declared : block contract names the receipt type
    Declared --> Earned : emitter produces evidence - test run, paint, review, spans, spec
    Earned --> Fresh : within validity window
    Fresh --> Stale : members changed or window expired
    Stale --> Earned : re-earned by a new run
    Fresh --> Failed : emitter reports failure
    Failed --> Earned : fixed and re-earned
```

Counters on screen always read earned-fresh over declared — the auditable denominator.

---

## 6. Mission layer — packet, runners, pins (component + sequence + states)

```mermaid
flowchart LR
    CMP["Packet composer<br/>block scoped"] --> M1["clipboard<br/>markdown - universal"]
    CMP --> M2["direct<br/>EXISTS - mailbox inbox"]
    CMP --> M3["spawn"]
    M3 --> POL["Policy gate<br/>capabilities, workspace truth,<br/>isolated worktree, propose-only"]
    POL --> RC["codex runner<br/>one-shot"]
    POL --> RA["agy runner<br/>fast lane"]
    POL --> RL["l00p runner<br/>wave 2 - gated loop"]
    POL --> RG["gogod runner<br/>wave 2 - key moments"]
    RC --> DB["debrief<br/>EXISTS"]
    RA --> DB
    DB --> LG["outcomes ledger<br/>EXISTS"]
    LG --> PN["Pin on block"]
    PN -->|passes block receipt rules| RCPT["becomes a receipt"]
    PN -->|otherwise| HIST["stays history"]
```

```mermaid
sequenceDiagram
    participant U as Owner
    participant UI as Build Map
    participant PC as Packet composer
    participant PG as Policy gate
    participant RN as Runner
    participant AG as Agent
    participant DB as Debrief
    participant PIN as Pin

    U->>UI: Ask agent from block
    UI->>PC: compose - details, files, receipts, impact
    PC-->>U: preview with declared effects
    U->>PC: mode spawn, agent codex
    PC->>PG: check capabilities and workspace
    PG->>PG: create isolated worktree
    PG->>RN: launch with packet
    RN->>AG: execute mission
    AG-->>RN: proposal - diff, notes
    RN->>DB: debrief - classify touched paths
    DB-->>PIN: outcome to ledger, pin docks on block
    U->>PIN: view diff, land or reject
    Note over PIN: nothing auto-applies - propose only
```

```mermaid
stateDiagram-v2
    [*] --> Running : mission launched
    Running --> NeedsReply : agent asks a question
    NeedsReply --> Running : owner answers
    Running --> DoneUnverified : output landed, no debrief yet
    DoneUnverified --> DoneDebriefed : debrief classifies touched paths
    Running --> Failed : error or cancel
    DoneDebriefed --> Receipt : outcome passes block receipt rules
    DoneDebriefed --> History : informative only
```

---

## 7. Data contracts (class view)

```mermaid
classDiagram
    class SystemBlock {
        block_id
        name_ratified
        purpose
        membership many_to_many
        membership_source
        ratifier_version
        sockets internal_and_external
        receipts declared_and_earned
        unmapped_residue
        node_links
    }
    class Receipt {
        type test_structural_runtime_review_handoff_spec
        emitter
        scope
        earned_at
        validity
        state fresh_stale_failed
    }
    class MissionPacket {
        source_block
        message
        includes details_files_receipts_impact
        mode clipboard_direct_spawn
        declared_effects
    }
    class Pin {
        mission_id
        agent
        status
        progress
        outcome_ref
    }
    SystemBlock "1" --> "many" Receipt
    SystemBlock "1" --> "many" MissionPacket : packets are block scoped
    MissionPacket "1" --> "0..1" Pin : spawn creates
    Pin --> Receipt : promoted only via block rules
```

---

## 8. What is NEW vs EXISTS (the honest build list)

| Piece | Status |
|---|---|
| graph snapshot with tags, x-ray paint/orient, delegate/debrief, outcomes ledger, mailbox, surgical_context, impact, layers | EXISTS — consumed as-is |
| SystemBlock store (contract, ratification, versions, drift) | NEW — F0a |
| Receipt taxonomy + per-block contracts | NEW — F0a |
| Skeleton engine (cluster + agent naming + attach + residue) | NEW — F0c, uses agy runner |
| Packet composer (block-scoped, 3 modes, declared effects) | NEW — F2, wraps delegate |
| Policy gate + runners (codex, agy; l00p/gogod wave 2) | NEW — F2.5 |
| Pins projection | NEW thin — over existing ledger + fates |
| Build Map / Show Code / Ratification / Recipe / ULM / Clients screens | NEW UI — per screen book |
| TrustEnvelope UI type fix (add unprovable) | PRE-WORK — one line, exists as bug |
