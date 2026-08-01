# m1nd-mcp-tool-surface-verb-budget

The MCP server's outward face: a static 117-verb catalog advertised through a two-tier verb-budget gate (41 essential vs 117 full), dispatched by a name-matched router to per-verb handlers, wrapped by injected `M1ND_INSTRUCTIONS` doctrine, a host-binding tool-surface contract, and read-only / proof-gate write guards.

## Class/Component

```mermaid
flowchart TB
    Host["Coding-agent host<br/>(MCP client)"]

    subgraph Server["m1nd-mcp/src/server.rs — the heart"]
        HMM["handle_mcp_method :4735<br/>(transport-agnostic)"]
        INIT["initialize → serverInfo + capabilities<br/>+ M1ND_INSTRUCTIONS :33-186 (doctrine)"]
        LIST["tools/list → tool_schemas() :4774"]
        CALL["tools/call → dispatch_tool :3922"]

        subgraph Catalog["Verb catalog (static registry)"]
            ALL["all_tool_schemas_inner :505-2597<br/>EXACTLY 117 name entries"]
            TIER["active_tool_tier()<br/>M1ND_TOOL_TIER: full | else→core"]
            CORE["CORE_TOOLS (12, owner-ratified)<br/>∪ HOST_BINDING_REQUIRED_TOOLS (8)<br/>= core_menu_tool_names() → 15 served"]
            TSFT["tool_schemas_for_tier<br/>core = strict subset of the registry<br/>+ annotate_core_menu_discovery on `help`"]
        end

        subgraph Guards["Pre-dispatch guards"]
            RO["read_only_denied :2633<br/>READ_ONLY_DENIED_TOOLS :2597 (~14 verbs)"]
            PG["proof_gate_enabled :457 (default OFF)<br/>PROOF_GATED_WRITE_TOOLS :2655<br/>apply/apply_batch/edit_commit"]
        end

        subgraph Router["Verb dispatcher (name-matched)"]
            DT["dispatch_tool :3922"]
            DCORE["dispatch_core_tool :4088<br/>106 arms + hidden 'resonate' :4226"]
            DPER["dispatch_perspective_tool :5002<br/>12 arms (perspective_ prefix)"]
            DLOCK["dispatch_lock_tool :5114<br/>5 arms (lock_ prefix) — UNCATALOGUED"]
        end
    end

    subgraph Tools["m1nd-mcp/src/tools.rs — contract + handlers"]
        HEALTH["handle_health → tool_surface_contract :3294<br/>full_registry_tool_count=live 117<br/>minimum_safe=HOST_BINDING_REQUIRED_TOOLS.len()=8"]
        HANDSHAKE["handle_session_handshake :3339<br/>degraded_host_tool_surface"]
        AGT["AGENT_TRUST_REQUIRED_TOOLS :24 (7)"]
        HBT["HOST_BINDING_REQUIRED_TOOLS :34 (8)"]
    end

    HELP["help_guidance.rs :73<br/>catalog_entries — DERIVED from<br/>tier-gated schema_tools() :444<br/>(sees advertised subset only)"]
    SHAPE["result_shaping.rs :80<br/>pack_to_budget — SEPARATE token budget<br/>estimate_tokens chars/4 (NOT BPE)"]
    HTTP["mcp_http.rs run_promote :434<br/>the one owner-level cross-store verb"]

    Host -->|initialize| HMM --> INIT
    Host -->|tools/list| HMM --> LIST --> TIER --> TSFT
    TSFT --> ALL
    TSFT -.reads.-> ESS
    Host -->|tools/call| HMM --> CALL --> DT
    DT --> RO --> PG --> Router
    DCORE -.calls.-> Tools
    DT -.error rewrite.-> HELP
    LIST -.self-tracks.-> HELP
    DCORE -.budgeted verbs.-> SHAPE
    HEALTH -.floor law.-> HBT
    HANDSHAKE -.compares host names.-> AGT
    DCORE -.promote stub errors to.-> HTTP
```

## Sequence

```mermaid
sequenceDiagram
    participant H as Host (MCP client)
    participant M as handle_mcp_method
    participant G as Guards (read_only / proof_gate)
    participant AI as auto_ingest tick
    participant R as Router (dispatch_*_tool)
    participant HND as Handler (tools.rs / *_handlers)
    participant P as personality::build_m1nd_meta

    H->>M: initialize
    M->>M: negotiate_protocol_version :4711<br/>(prefer 2025-06-18)
    M-->>H: serverInfo + capabilities + M1ND_INSTRUCTIONS :4757

    H->>M: tools/list
    M->>M: tool_schemas() → active_tool_tier()
    Note over M: default essential = 41 of 117<br/>full = all 117 · unknown → essential
    M-->>H: advertised schemas

    H->>M: tools/call {name, arguments}
    M->>M: track agent_id · should_autotick_daemon :4846
    M->>G: dispatch_tool :3922
    alt read_only && read_only_denied
        G-->>H: InvalidParams (before any side effect)
    else proof_gate ON && write verb && targets not ready
        G-->>H: InvalidParams (unproven)
    else pass
        G->>AI: maybe_tick_auto_ingest (most verbs) :4007
        AI->>R: route by prefix
        alt name starts perspective_
            R->>HND: dispatch_perspective_tool (12)
        else name starts lock_
            R->>HND: dispatch_lock_tool (5, uncatalogued)
        else
            R->>HND: dispatch_core_tool (106 arms)
        end
        alt handler Ok
            HND->>P: attach additive _m1nd envelope :4059<br/>(if M1ND_RESPONSE_ENVELOPE on)
            P-->>H: content[].text (JSON + _m1nd)
        else handler Err
            HND->>HND: runtime_error_guidance_hint :405<br/>(corrective did-you-mean)
            HND-->>H: isError:true content (NOT JSON-RPC error) :4816
        end
    end
    Note over M,H: unknown method → JSON-RPC -32601 :4830
```

## State/Flow

```mermaid
stateDiagram-v2
    [*] --> Uninitialized
    Uninitialized --> Initialized: initialize (doctrine injected)
    Initialized --> Advertised: tools/list (tier-gated)
    Advertised --> Advertised: tools/call (repeat)

    state "tools/call pipeline" as Call {
        [*] --> ReadOnlyGate
        ReadOnlyGate --> Refused_RO: read_only && denied
        ReadOnlyGate --> ProofGate: allowed
        ProofGate --> Refused_Proof: gate ON && target unproven
        ProofGate --> Routed: allowed
        Routed --> Ok: handler success → _m1nd envelope
        Routed --> IsError: handler Err → guidance hint
        Ok --> [*]
        IsError --> [*]
        Refused_RO --> [*]
        Refused_Proof --> [*]
    }

    Advertised --> Call: dispatch_tool
```

## Invariantes
- Catalog size == 117: `all_tool_schemas_inner()` has exactly 117 top-level `"name"` entries (server.rs:505-2597; verified in-session: grep of 8-space-indent names = 117; the 2 extra `"name"` hits are nested inside inputSchemas).
- Tier gate trims **advertisement only**, never handler existence: hidden verbs stay registered and callable (tests `all_tool_schemas_always_contains_all_tools_regardless_of_tier` :7352, `tier_gate_full_advertises_all_tools` :7270). Essential is a strict subset.
- Unrecognized `M1ND_TOOL_TIER` → the CORE menu, never full: `active_tool_tier()` matches only `"full"`, so a typo in an operator env fails closed on the SMALL surface (test `tier_gate_unrecognized_value_falls_back_to_core`).
- The served menu == `core_menu_tool_names()` == `CORE_TOOLS` ∪ `HOST_BINDING_REQUIRED_TOOLS` — DERIVED, never a second hand-typed list, so it cannot drift from the host-binding contract it has to satisfy (tests `core_menu_serves_exactly_the_ratified_core_plus_the_binding_floor`, `tier_gate_default_advertises_only_the_core_menu`). The historical `"essential"` token still resolves here.
- Menu size ≤ `CORE_MENU_MAX` (20; 15 served today) — the mechanical form of the owner's "the menu must fit on one screen". The bound itself is guarded at COMPILE time (`const _: () = assert!(CORE_MENU_MAX < 30)`).
- Every `CORE_TOOLS` name exists in the registry — a typo would silently shrink the menu (test `core_menu_stays_within_its_stated_bound`).
- The core menu carries its own map: `annotate_core_menu_discovery` appends the `[CORE MENU]` line to the `help` description with a COMPUTED hidden count, so verb #143 keeps the sentence true (test `core_menu_carries_the_line_that_leads_to_the_rest`).
- `full_registry_tool_count` in health is COMPUTED live from the schema array (tools.rs:3269) — self-corrects to 117 despite nearby "102" doc comments.
- `minimum_safe_tool_count` is the REQUIRED-verb floor (`HOST_BINDING_REQUIRED_TOOLS.len()` = 8), NOT total registry — deliberately so tiering never falsely reads as degraded.
- `degraded_host_tool_surface` fires iff any `AGENT_TRUST_REQUIRED_TOOL` (7) is missing from host-reported `available_tools`; a count-only handshake never invents missing tools (test :6291).
- read-only attach refuses every listed mutating verb BEFORE any side-effecting tick (read_only gate is first in dispatch_tool :3932); `delegate` is intentionally ABSENT from the denylist (it is read-only, ambient-legal).
- proof gate (when ON) treats an empty/unresolvable target set as unproven → refuses (:3957).
- tools/call handler errors surface as `isError` content, not JSON-RPC errors (MCP spec, :4816).
- `resonate` is a handler-only back-compat verb: dispatchable (:4226) but ABSENT from every advertised tier (test `resonate_schema_absent_from_all_tiers_after_surface_removal` :7391).

## Gaps
- **[medium] No parity/drift-guard test** between advertised catalog (122), the dispatch table, and `ESSENTIAL_TOOLS`. A verb can be added to one and forgotten in another with green CI. Live drift PRESENT: 5 `lock_` verbs dispatchable (server.rs:5114-5162) but NOT in the 122-catalog; `resonate` dispatchable-but-uncatalogued (by design).
- **[medium] `lock_` verbs are invisible verbs**: callable via tools/call but never advertised in ANY tier (not even `full`). `schema_tools()` now reads `all_tool_schemas()`, so `help` covers every REGISTERED verb — but the `lock_` family is not in the registry at all, so it remains unreachable by `help`/`find_similar_tool_names`. Fixing it means registering the schemas, not widening help.
- **[low] `help` + fuzzy suggestion saw only the advertised tier** — **CLOSED 2026-08-01**: `schema_tools()` reads `all_tool_schemas()`, so `help` and the did-you-mean matcher catalog the FULL registry at every tier (test `help_sees_every_verb_regardless_of_tier`). The gap was low severity while the default menu was 48; shrinking it to a core of 15 made `help` the door and promoted this to load-bearing.
- **[low] Stale count literals** — **CLOSED**: docstrings now state the registry-derived counts (122 full / 42 essential — the F0a slice-2 landing added the four system-blocks verbs).
  Original finding: in doctrine-adjacent comments: "all 102 tools" (server.rs:423,465,504 — confirmed in code read), "expose all 102 tools" (tools.rs:3301), "curated ~25 tools" for the 41-entry const (server.rs:367). Live values are correct; only the human-readable comments mislead.
- **[low] `session_handshake` degraded verdict trusts the caller**: the server cannot independently see which subset the host injected (non_claim, tools.rs:3323-3326). A host that omits verbs but passes no `available_tools` yields a false "ok".
- **[low] Naming hazard — two "budgets"**: the verb budget (tier gate) and the result token budget (result_shaping) are orthogonal; the token estimate is chars/4, explicitly NOT real BPE (result_shaping.rs:114), so packed budgets can drift.
```
