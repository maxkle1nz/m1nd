# HOST-INTEGRATION-MATRIX — the ambient layer on every agent host

> The canonical reference for installing m1nd's **ambient orientation layer** on every
> agent host in the market: get a **north packet** in front of the model *before or at its
> first action*, on any host, with the strongest channel that host actually ships.
>
> **Provenance:** synthesized from a five-report research fleet run **2026-07-03**
> (`warp.md`, `cursor-opencode-aider-qwen.md`, `cli-hosts.md`, `ide-hosts.md`,
> `oss-spec.md`). Every verification label below (`[verified-docs]` /
> `[verified-hands-on]` / `[unverified]`) is **carried through from that research** — the
> matrix's credibility *is* the labels. Proof-grown throughout: **built** vs **unbuilt**
> vs **unverified** are never blurred.

---

## 1. Thesis — the universality ladder

The ambient layer is one idea: **the agent should not act cold.** It should meet a
computed **north packet** — task context, binding trust, prior memory, honest gaps — at or
before its first move. The engineering question is not *what* to deliver (the packet is
settled: it is `north`), but *through which channel*, because **hosts differ wildly in what
they let a server say and when.**

A server is not a free actor. **An MCP server speaks only when it is called** — the spec
limit: at `initialize` the server may return `protocolVersion` / `capabilities` /
`serverInfo` / `instructions` and *nothing more*; it **cannot inject context into the model
without being invoked** (`oss-spec.md` §B.5). That single constraint is *why* the ladder
has a floor that does not depend on hooks, rules files, or any host cooperation beyond the
one thing every MCP client already does.

**The universality ladder — strongest-that-exists, floor that always works:**

1. **In-band packet — the floor, works on EVERY host.**
   The one thing *every* MCP client does is **render a tool result.** So the universal
   channel is the *result of a `tools/call`* — deliver the north packet as the answer to
   the session's first call, governed by a "north first" contract. This is the
   **First-Contact Reception** — `docs/TWO-TIER-BRAIN-PRD.md` §9.5 (the packet
   `schema: m1nd-reception-v0`, returned as the result of the session's first tool call
   whatever verb the agent sends). It is the floor precisely because it needs nothing but
   MCP-result rendering — `_meta` is out-of-band (the model never sees it), but
   `structuredContent` of the *result* is seen (`oss-spec.md` §B.4). Use in-band when
   nothing stronger exists — which is most hosts.

2. **Out-of-loop hooks — gold, where they exist.**
   A session-start hook that fires *outside the model loop* and injects `additionalContext`
   before turn 1 is deterministic — no model steering required. This is TIER A. It exists
   on a minority of hosts (Claude Code, Agent SDK, Codex CLI, Qwen Code, Kiro, Cline,
   Continue-CLI, the Grok community fork). **One shim, N ports:** every hook recipe calls
   the same host-neutral CLI (§3).

3. **Always-on rules files — the broad middle.**
   `AGENTS.md` / `CLAUDE.md` / `WARP.md` / `.cursor/rules` / `.clinerules` / steering docs
   — auto-loaded doctrine that *tells the model to call `north` first*. Model-steered, not
   deterministic (a rule can be skipped or evicted after context fills), but nearly
   universal. This is TIER B.

4. **`instructions` field — bonus, never a dependency.**
   The spec says `instructions` **MAY** be added to the system prompt — no `MUST`, no
   `SHOULD` (`oss-spec.md` §B.1). Some hosts render it (Claude Code current, Gemini CLI,
   Codex, Goose); most receive-and-discard it. Because support is a coin-flip per host, it
   is a **belt-and-suspenders bonus that reinforces a real channel — never the
   load-bearing mechanism.**

**Design law:** pick the *highest* rung a host actually ships, and always lay the in-band
floor underneath it. A hook can drop `additionalContext` (a known Claude-family failure
mode); a rule can be evicted; `instructions` can be discarded. The in-band packet is the
one channel that cannot be silently swallowed, because rendering a tool result is the
definition of an MCP client.

---

## 2. The grand matrix — every host

Columns: **Session-start hook** (does a hook inject context before turn 1?) ·
**`instructions` rendered** (does the host surface the MCP `instructions` field to the
model?) · **Roots** (does the host serve the MCP `roots` capability?) · **Rules file**
(the always-on doctrine channel) · **TIER** · **Recipe** (§ pointer).

TIER legend: **A** = out-of-loop hook injects before turn 1 (deterministic). **B** =
rules-file doctrine only (model-steered). **C** = `instructions`-field render is the
strongest channel. **D** = a lazy/pre-computed injection (pre-tool-call context, or a
CLI-precomputed packet read from a file). Combined tiers (e.g. **B+D**) mean both apply.

### CLI hosts

| Host | Session-start hook | `instructions` rendered | Roots | Rules file (auto) | TIER | Recipe |
|---|---|---|---|---|---|---|
| **Claude Code** | ✅ SessionStart → `additionalContext` `[verified-hands-on]` | ✅ **YES** — live-session proof 2026-07-03 (§5) `[verified-hands-on]` | ✅ **YES** `[verified-docs]` | `CLAUDE.md` (`settingSources`) | **A** 🥇 | §3.1 |
| **Claude Agent SDK** | ✅ SessionStart injects **+ `systemPrompt.append` (direct) + in-process MCP** `[verified-docs]` | not documented | ✅ **YES** `[verified-docs]` | `settingSources` loads `CLAUDE.md` | **A+** 🥇 | §3.6 |
| **Codex CLI** | ✅ **SessionStart GA**, `type:command` → `additionalContext` `[verified-hands-on ~/.codex/hooks.json, codex 0.142.5]` | ✅ **YES** — "reads the MCP instructions field … as server-wide guidance" `[verified-docs]` | ✗ | `AGENTS.md` (root→cwd merge) | **A** 🥇 | §3.2 |
| **Gemini CLI** | ✗ (no hooks) | ✅ **YES** — "appended to the system instructions" `[verified-docs]` | Partial (issue #5861) | `GEMINI.md` | **C** | §4 |
| **Qwen Code** | ✅ **SessionStart → `additionalContext` FUNCIONA** `[verified-docs]` | `[unverified]` — issue #733: model not seeing MCP context | not documented | `QWEN.md` (rename→`AGENTS.md` ignored, bug #727) | **A** 🥇 | §3.3 |
| **Goose** | SessionStart exists; **stdout→context NOT documented** (side-effect only) `[unverified]` | `[unverified]` (renderer per §5, but no live proof) | `[unverified]` | `.goosehints` | **B** (A unproven — needs probe) | §4 |
| **Crush** | ✗ (only PreToolUse today) | `[unverified]` | `[unverified]` | `CRUSH.md`/`AGENTS.md`/`CLAUDE.md`/`GEMINI.md` | **B+D** (PreToolUse `context` appended before first tool call) | §4 |
| **Amp** | `session.start` plugin, but ctx **only ui.notify/thread.append (visible)** — no covered injection | `[unverified]`; toolboxes deprecated | ✗ | `AGENTS.md` (cwd→$HOME + fallbacks, very reliable) | **B** | §4 |
| **Grok Build (official)** | ✗ — hooks only edit/command/error, **no SessionStart** | `[unverified]` | `[unverified]` | reads `CLAUDE.md` **natively** | **B** | §4 |
| **grok-cli fork** (superagent-ai) | ✅ **SessionStart** (`~/.grok/user-settings.json`) — verify `additionalContext` contract | `[unverified]` | `[unverified]` | `AGENTS.md` (`.override.md` wins) | **A** (community) | §3.7 |
| **Aider** | ✗ (none; issue #2045 stale) | **N/A — no native MCP** (issues #4506/#3314) | N/A | `CONVENTIONS.md` via `read:` | **B+D** (`load:` → `/run` precomputes packet) | §4 |
| **opencode** (sst) | `session.created` reacts, **does not inject turn 1** | not mentioned | not mentioned | `AGENTS.md`/`CLAUDE.md` + `instructions[]` | **B** | §4 |
| **Warp** | ✗ **none shipped** (issue #7834 open, `ready-to-spec`, unshipped) | ❌ **`instructions` appears ZERO times** in Warp MCP docs — treat unsupported `[verified-docs]` | ❌ zero mentions — unsupported `[verified-docs]` | `WARP.md` (ALL-CAPS; wins over `AGENTS.md`) — auto-applied to new conversations | **B** | §4 |
| **Continue (CLI)** | ✅ **SessionStart shipped** (PR #11029, Claude-compatible) | `[unverified→leaning-no]` | `[leaning-no]` | `.continue/rules/` (does **not** auto-read `AGENTS.md`) | **A (CLI)** | §3.5 |
| **Continue (IDE)** | unconfirmed | `[unverified→leaning-no]` | `[leaning-no]` | `.continue/rules/` | **B (IDE)** | §4 |
| **OpenHands** | ✗ hook API, **BUT repo skills always-on pasted into system prompt every conversation** (near-A without a hook) | `[unverified]` | `[unverified]` | `AGENTS.md`/`CLAUDE.md`/`GEMINI.md` auto-discovered + `.openhands/skills/` | **B (near-A)** | §4 |
| **Devin** | ✗ public hook; Knowledge (auto-recalled) + Playbooks | `[unverified]`; is an MCP client (Marketplace, 3 transports) | `[unverified]` | Knowledge + Playbooks (not a repo file) | **C** | §4 |

### IDE hosts

| Host | Session-start hook | `instructions` rendered | Roots | Rules file (auto) | TIER | Recipe |
|---|---|---|---|---|---|---|
| **Cursor** | ⚠️ `sessionStart` exists (beta v1.7+) but `additional_context` **BROKEN** (staff-confirmed 2026-04-20, no fix) | ✗ `instructions` never mentioned `[verified-docs]` | ✅ **YES** (explicit in capabilities table) `[verified-docs]` | `.cursor/rules/*.mdc` **Always** (`alwaysApply:true`) / `AGENTS.md` | **B** (A staged behind the bug) | §4 |
| **Windsurf** (→ Devin Desktop) | ✗ 12 hooks but stdout discarded (exit-code only); no `session_start` | `[unverified]` | `[unverified]` | `global_rules.md` (≤6k chars) + `.windsurf/rules` + `AGENTS.md` | **B** | §4 |
| **Antigravity** | ✗ none | ⚠️ **Windsurf fork, NOT the Gemini-CLI harness → appending does NOT transfer** `[unverified]` | `[unverified]` | `AGENTS.md` + `.agents/skills/`; **`GEMINI.md` prepended to every request** `[hands-on]` | **B** (C aspirational) | §4 |
| **Zed** | ✗ no hooks | no evidence (Tools+Prompts only) | absent from schema | `AGENTS.md`/`.rules`/`CLAUDE.md`/`copilot-instructions`/`GEMINI.md` — **first match wins** | **B+D** (context-server prompt) | §4 |
| **VS Code Copilot** | ✗ no chat-injection API | `[unverified]` (spec-listed, no render detail) | ✅ **YES** (new docs beat legacy mdx) `[verified-docs]` | `.github/copilot-instructions.md` + `*.instructions.md` + `AGENTS.md` | **B+D** (extension Chat/LM API) | §4 |
| **Kiro (AWS)** | ✅ **agentSpawn: exit 0 → STDOUT BECOMES CONTEXT** `[verified-docs]` | `[unverified]` | `[unverified]` | `.kiro/steering/**/*.md` auto-injected | **A** 🥇 | §3.4 |
| **Trae** | ✗ | `[unverified]` | `[unverified]` | `.trae/project_rules.md` + `user_rules.md` (init phase) | **B** | §4 |
| **JetBrains / Junie** | ✗ | `[unverified]` | `[unverified]` | `.junie/AGENTS.md` → root `AGENTS.md` → `guidelines.md` ("added to every task") | **B** | §4 |
| **Cline** | ✅ **TaskStart → `contextModification`** (v3.36) — **macOS/Linux only** `[verified-docs]` | ✗ (disc #3114 unshipped) | ✗ | `.clinerules` (file or dir) | **A** 🥇 | §3.4 (Cline) |
| **Roo Code** (→ Kilo, deprecated) | ✗ no hooks; **SHUT DOWN May 2026 → Kilo** | ✗ | ❌ broken (#9370 "Client does not support MCP Roots") | `.roo/rules/` | **B (deprecated)** | §4 |
| **Kilo Code** | ✗ no documented hooks | ✗ | `[unverified→No]` | `.kilocode/rules/` | **B** (living successor to Roo) | §4 |
| **Continue** | see CLI vs IDE split above (CLI = **A**, IDE = **B**) | — | — | `.continue/rules/` | **A / B** | §3.5 / §4 |

**Headline:** of ~29 host surfaces, **10 reach TIER A** (deterministic pre-turn injection):
Claude Code, Agent SDK (A+), Codex CLI, Qwen Code, Kiro, Cline, Continue-CLI, the grok-cli
community fork — plus Claude Code and the SDK counted once each. Everything else lands at
**B / C / D**, and *every* host is covered by the in-band floor (§1 rung 1).

---

## 3. TIER A recipes — full, copy-pasteable

**Every recipe below calls the existing host-neutral shim** — no new script required today:

```bash
m1nd agent first-minute --repo "<repo>" --query "<task>" --json
```

This is the shipped CLI escape hatch (`skills/m1nd-universal-agent-pack.md:160-176`): it
**scopes, trusts, ingests when needed, runs one bounded orientation pass, returns anchors,
and hands control back to direct proof** — without a live MCP session. It is the
host-neutral front for out-of-loop hooks; `north` remains the in-session front door.

> **The parse contract every hook shares.** A SessionStart-family hook must emit, on
> stdout, exactly:
> ```json
> {"hookSpecificOutput":{"hookEventName":"SessionStart","additionalContext":"<north packet as text>"}}
> ```
> The packet text opens with a served owner's `north` **voice card** (`human_view` lines)
> when one is reachable — the shim finds it the read-only way `--attach auto` does (instance
> registry, then a short probe of the default serve ports) and calls `north` over HTTP — and
> otherwise falls back to the human-readable rendering of `m1nd agent first-minute … --json`
> standalone. Wrap the shim in a two-line script that runs it, extracts the packet, and prints the
> envelope. This wrapper ships as the `m1nd-north-shim` bin (registered in `package.json`),
> so every recipe below invokes `m1nd-north-shim` directly — you no longer write it by hand
> (§6a, SHIPPED 2026-07-03).

### 3.1 Claude Code — **A** (shim shipped; you paste the hook)

The `m1nd-north-shim` bin ships in this repo; the SessionStart hook itself does **not** — this
repo's `.claude/settings.json` carries no hook, because Claude's settings file is host-managed,
so m1nd **prints** the block rather than writing it. `m1nd hosts apply --host claude` wires the
hosts it owns and prints the exact block below for you to paste into `.claude/settings.json` (or
`~/.claude/settings.json`) and then verify it fired:

```json
{
  "hooks": {
    "SessionStart": [
      {
        "matcher": "startup|resume",
        "hooks": [
          { "type": "command",
            "command": "m1nd-north-shim --repo \"$CLAUDE_PROJECT_DIR\" --query \"orient\"" }
        ]
      }
    ]
  }
}
```

- The shim prints the `additionalContext` envelope above; Claude Code injects it before
  turn 1. `[verified-hands-on]`
- **Verify it fired:** run the shim by hand — `m1nd-north-shim --repo "$PWD" --query orient` —
  and confirm it prints a `{"hookSpecificOutput":…}` line, or open a fresh session and look for
  the injected `[m1nd north]` orientation. A silent `exit 0` with no output is the fail-open
  default (an absent or broken runtime never blocks the session), not a hook failure.
- **Belt-and-suspenders:** Claude Code current **also renders the MCP `instructions`
  field** (§5, live proof 2026-07-03) — so a north-first line in `M1ND_INSTRUCTIONS`
  reinforces the hook. Bonus, not dependency.
- **Roots:** Claude Code serves the MCP `roots` capability `[verified-docs]` — m1nd can
  request the workspace root instead of relying on cwd.

### 3.2 Codex CLI — **A** (hooks.json + trust gate)

`~/.codex/hooks.json` `[verified-hands-on, codex 0.142.5]`:

```json
{
  "SessionStart": [
    { "matcher": "startup|resume",
      "hooks": [
        { "type": "command",
          "command": "m1nd-north-shim --repo \"$CODEX_CWD\" --query \"orient\"" }
      ]
    }
  ]
}
```

And in `~/.codex/config.toml`:

```toml
[features]
hooks = true
```

- **Trust gate (do not fight it):** on the first run Codex records the hook command's
  `trusted_hash` in `[hooks.state]` and prompts for trust. Change the command → it re-asks.
  **Never script `--dangerously-bypass-hook-trust`** — approve the prompt once.
- `notify` ≠ hooks: Codex's `notify` is an *external* event that injects nothing; it
  coexists with hooks, it does not replace them.
- **Belt-and-suspenders:** Codex **renders `instructions`** — its first ~512 chars are
  read as server-wide guidance, so make them self-contained and north-first `[verified-docs]`.

### 3.3 Qwen Code — **A** (SessionStart + extension packaging)

`.qwen/settings.json` (or `~/.qwen/settings.json`):

```json
{
  "hooks": {
    "SessionStart": [
      { "hooks": [
        { "type": "command",
          "command": "m1nd-north-shim --repo \"$PWD\" --query \"orient\"" }
      ] }
    ]
  }
}
```

The hook injects `{"hookSpecificOutput":{"additionalContext":"…"}}` before the first turn —
this is a Qwen divergence beyond upstream Gemini CLI, and it **works** `[verified-docs]`.

- **Package it as an extension.** A `qwen-extension.json` bundles `mcpServers` **and**
  `contextFileName: QWEN.md` into one git-installable unit — ship m1nd's MCP registration
  and doctrine together:
  ```json
  { "name": "m1nd-orient",
    "mcpServers": { "m1nd": { "command": "m1nd", "args": ["mcp"] } },
    "contextFileName": "QWEN.md" }
  ```
- **Do NOT rename `QWEN.md` → `AGENTS.md`** — it is silently ignored (bug #727). Keep
  `QWEN.md`.
- **`instructions` is `[unverified]` on Qwen** (issue #733 shows the model not seeing MCP
  context) — do not lean on it here; the hook is the real channel.

### 3.4 Kiro — **A** (agentSpawn + render-bug note) · and Cline — **A** (TaskStart)

**Kiro (AWS).** In the agent config, `agentSpawn` runs at spawn; **exit 0 → STDOUT becomes
context** (stdin carries `{hook_event_name, cwd, session_id}`) `[verified-docs]`:

```json
{
  "hooks": {
    "agentSpawn": [
      { "command": "m1nd agent first-minute --repo \"$PWD\" --query \"orient\" --json" }
    ]
  }
}
```

Register m1nd in `~/.kiro/settings/mcp.json` (hot-reload) and put doctrine in
`.kiro/steering/`.
- **Cosmetic bug #5372:** with `agentSpawn`, `kiro-cli` may not *display* the injected text
  — **the injection still works; it is a render bug, not a delivery failure.**

**Cline.** `TaskStart → contextModification` (v3.36) injects at task start — **macOS/Linux
only** (Windows unsupported today) `[verified-docs]`. Wire the hook to run the shim and feed
its output into the task's context modification, and keep `.clinerules` as the always-on
reinforcement. `instructions` is unshipped on Cline (disc #3114) — the hook is the channel.

### 3.5 Continue CLI — **A**

Continue's **CLI** shipped a Claude-compatible SessionStart (PR #11029) — the IDE build is
unconfirmed, so this recipe is **CLI-only**. Configure a SessionStart hook that runs the
shim and emits the `additionalContext` envelope, and mirror the doctrine in
`.continue/rules/`.
- **Gotcha:** Continue does **not** auto-read `AGENTS.md` — the doctrine must live under
  `.continue/rules/`, not a generic agents file.
- **Verify the `additionalContext` contract against the post-#11029 source before
  promising A in production** — it is `[verified-docs]` that the hook exists, `[unverified]`
  on the exact injection field name.

### 3.6 Claude Agent SDK — **A+** (three levers)

The SDK is the reference integration because it has **three** independent delivery levers —
use the strongest and let the others reinforce:

1. **`systemPrompt.append`** — the north packet goes *directly* into the system prompt, with
   **no hook and no tool round-trip.** Compose it once with the SDK preset:
   ```ts
   import { query } from "@anthropic-ai/claude-agent-sdk";
   const northPacket = runShim(repo, task); // m1nd agent first-minute … --json → text
   for await (const msg of query({
     prompt: task,
     options: {
       systemPrompt: { preset: "claude_code", append: northPacket },
       // in-process MCP server (lever 3):
       mcpServers: { m1nd: m1ndSdkServer },
       settingSources: ["project"], // loads CLAUDE.md (lever 2 reinforcement)
     },
   })) { /* … */ }
   ```
2. **`hooks.SessionStart`** — the same `additionalContext` injection as Claude Code, if you
   prefer the hook seat over `append`.
3. **m1nd as an in-process MCP server** — `createSdkMcpServer` runs m1nd *inside* the SDK
   process (no stdio bridge), so `north` is a same-process call.

**Roots:** the SDK serves `roots` `[verified-docs]`. With `append` available, prefer it —
it is the single most reliable channel of any host, because it bypasses both hook-drop and
result-render entirely.

### 3.7 grok-cli community fork — **A** (superagent-ai)

The **community** `superagent-ai/grok-cli` fork (not the official Grok Build) ships
`SessionStart` via `~/.grok/user-settings.json`. Wire it to the shim exactly as Codex/Qwen.
- **Name collision (the gotcha):** SessionStart is the *fork's* capability — the **official
  Grok Build has no SessionStart** (its hooks are edit/command/error only; it reads
  `CLAUDE.md` natively → TIER B). Confirm you are on the fork before promising A.
- `.override.md` wins over `AGENTS.md` in the fork's rules precedence — put the doctrine
  where it will not be shadowed.
- **Verify the fork's `additionalContext` contract** before production `[unverified]`.

---

## 4. TIER B / C / D recipes — compact

**Pattern for TIER B (rules file):** (a) register m1nd in the host's MCP config; (b) write
the host's always-on rules file with a first-line orientation gate — *"BEFORE responding to
the first user message or reading/editing any file, call the `m1nd` MCP tool `north` with
the current task and treat the returned packet as ground truth for this session"*; (c) let
the host auto-apply it. Model-steered, so also lay the in-band floor (§1 rung 1).

| Host | MCP config | Rules file | Extra channel |
|---|---|---|---|
| **Gemini CLI** (C) | `.gemini/settings.json` (`trust:true`) | `GEMINI.md` (B reinforcement) | **`instructions` is the primary channel** — north-first doctrine in m1nd's `instructions` field, rendered "appended to the system instructions". Roots partial (#5861). |
| **Goose** (B) | extensions (MCP) | `.goosehints` doctrine | If a live probe proves `additionalContext` from its SessionStart → **promote to A** (same shape as Codex). Until then, B. |
| **Crush** (B+D) | MCP config | `CRUSH.md` | **PreToolUse hook `context`** is "appended to what the model sees" → lazy injection *before the first tool call* (not turn 1). Use it as the D channel. |
| **Amp** (B) | `amp.mcpServers` | `AGENTS.md` (very reliable auto-load) | `session.start` plugin only as a **visible** nudge (`ui.notify`/`thread.append`) — noisy, not covered injection. |
| **Warp** (B) | `~/.warp/.mcp.json` or `.warp/.mcp.json` (`mcpServers.m1nd`, auto-spawn) | **`WARP.md`** ALL-CAPS at repo root (wins over `AGENTS.md`), first line = the north gate | Duplicate the directive in Global Rules (survives outside a repo). ⚠️ rules can be **evicted after context fills** (#7199). `instructions`/roots unsupported `[verified-docs]`. Gold requires Warp to ship #7834. |
| **opencode** (B) | `opencode.json` `"mcp"` | `AGENTS.md` + `"instructions":[globs/URLs]` merged | A path *aspirational*: a `session.created` plugin priming via `tui.prompt.append` is **not** guaranteed pre-turn — verify the API before promising A. |
| **Aider** (B+D) | **no native MCP** — `north` is not callable as a tool | `CONVENTIONS.md` via `--read` / `.aider.conf.yml` `read:` | **D (live-ish):** `.aider.conf.yml` `load: startup.aider` with `/run m1nd agent first-minute --repo . --query orient > .aider/north.md` then `/read .aider/north.md` — a **CLI-precomputed** packet, not a live tool. |
| **Devin** (C) | Marketplace MCP (3 transports) | Knowledge + Playbooks (not a repo file) | Put the north-first doctrine in a Playbook / Knowledge entry; `instructions` render `[unverified]`. |
| **OpenHands** (near-A) | MCP (mcp_location proposal #7547 **not planned**) | `AGENTS.md`/`CLAUDE.md`/`GEMINI.md` auto-discovered | **`.openhands/skills/`** are pasted into the system prompt every conversation → put an always-on north-first skill there (near-A without a hook). |
| **Cursor** (B, A staged) | `.cursor/mcp.json` / `~/.cursor/mcp.json` | `.cursor/rules/00-m1nd-orient.mdc` `alwaysApply:true` (keep ASCII paths; don't mix globs with `alwaysApply`) | **A is staged behind a confirmed bug** — a `sessionStart` hook emitting `{"additional_context":"<packet>"}` is written but Cursor drops it (timing bug, no fix). Activate when fixed. `.cursorrules` is deprecated *and ignored* in Agent mode. **Roots supported.** |
| **Windsurf** (B) → Devin Desktop | `mcp_config.json` | `global_rules.md` (condense the pack ≤6k chars) + `.windsurf/rules/m1nd.md` | Do **not** attempt A — hooks communicate by exit-code only (stdout discarded). **Rebrand:** Windsurf → **Devin Desktop** (Jun 2026; docs → docs.devin.ai; `.devin/rules/` preferred, `.windsurf/rules/` fallback). |
| **Antigravity** (B, C aspirational) | `~/.gemini/config/mcp_config.json` (attach `:1338`, hands-on) | `~/.gemini/GEMINI.md` (prepended to every request) | ⚠️ **shared-file collision:** Gemini CLI uses the *same* `~/.gemini/GEMINI.md` (issue #16058, not-planned) — doctrine leaks between the two tools. C is aspirational: it is a Windsurf fork, so Gemini-CLI's `instructions` appending does **not** transfer `[unverified]`. |
| **Zed** (B+D) | `context_servers` in `settings.json` | `AGENTS.md` (project + `~/.config/zed/AGENTS.md`) | **First-match-wins** across instruction files → ship exactly **one**. Rules Library is retired (→ Skills). D: a context-server prompt. |
| **VS Code Copilot** (B+D) | `.vscode/mcp.json` | `.github/copilot-instructions.md` + `*.instructions.md` + `AGENTS.md` | **Roots supported** `[verified-docs]`. D: an extension using the Chat / LanguageModel API to inject. `instructions` render `[unverified]` (spec-listed, no detail). |
| **Trae** (B) | `.trae/mcp.json` (project confirmed; global ambiguous) | `.trae/project_rules.md` + `user_rules.md` | Straight B. |
| **JetBrains / Junie** (B) | `.junie/mcp/mcp.json` (or `~/.junie/mcp/`) | `.junie/AGENTS.md` → root `AGENTS.md` → `guidelines.md` ("added to every task") | Straight B. |
| **Roo Code** (B, deprecated) | MCP config | `.roo/rules/` | **Deprecated — shut down May 2026 → migrate to Kilo.** Roots broken (#9370). |
| **Kilo Code** (B) | MCP config | `.kilocode/rules/` | The living successor to Roo. Roots `[unverified→No]`. |
| **Continue (IDE)** (B) | `.continue/` MCP config | `.continue/rules/` (not `AGENTS.md`) | IDE SessionStart unconfirmed → B until proven; CLI is A (§3.5). |

---

## 5. Spec-leverage — the honest support tables

This is where the ladder's rungs 4/5/7/8 get their exact, per-host truth. **The support is
thin and uneven — which is the whole reason the in-band floor (§1) exists.**

### 5.1 `instructions` field — MAY, not MUST (rung 4, bonus only)

Spec: *"a hint … that MAY be added to the system prompt"* — **no `MUST`, no `SHOULD`**
(`oss-spec.md` §B.1). So render support is per-host and cannot be load-bearing.

| Host | Renders `instructions`? | Evidence |
|---|---|---|
| **Claude Code** | ✅ **YES (current)** | **Orchestrator hands-on correction, 2026-07-03:** the live session renders the full `M1ND_INSTRUCTIONS` in the "MCP Server Instructions" section of context. The researcher's "No" (issue **#43749**, `getInstructions()` zero call sites) is **stale evidence or applies to Claude *Desktop***. Claude Code = **YES** `[verified-hands-on-live-session]` |
| **Gemini CLI** | ✅ YES | docs: "appended to the system instructions" `[verified-docs]` |
| **Codex** | ✅ YES | "reads the MCP instructions field … as server-wide guidance"; first ~512 chars `[verified-docs]` |
| **Goose** | ✅ YES | `prompt_manager.rs` (unicode-tag sanitization) — renderer confirmed in source |
| **VS Code** | ⚠️ listed, no detail | `[unverified]` — spec-listed, render unproven; probe hands-on |
| **Cline / Cursor / Continue / Roo / Kilo / Windsurf / Zed** | ❌ NO | `oss-spec.md` §B.1 |

**Rule:** `instructions` reinforces a real channel (hook or rule) on the four renderers
above — it is **never** the mechanism a host's coverage depends on.

### 5.2 `roots` — the workspace-root capability (rung 7)

Spec: root `uri` **MUST** be `file://`; an unsupported host returns `-32601`
(`oss-spec.md` §B.2).

| Host | Serves `roots`? |
|---|---|
| **Claude Code** | ✅ YES `[verified-docs]` |
| **Claude Agent SDK** | ✅ YES `[verified-docs]` |
| **Cursor** | ✅ YES `[verified-docs]` |
| **VS Code** | ✅ YES (new docs beat legacy mdx) `[verified-docs]` |
| Cline / Roo (broken #9370) / Kilo / Continue / Windsurf / Zed / Goose / Codex | ❌ NO — fall back to cwd / config |

**This RESOLVES the TWO-TIER-PRD `roots` `[unverified]` flag** at the *host-capability*
level: **four named hosts serve `roots`; everything else falls back to cwd/config.**
Cross-ref `docs/TWO-TIER-BRAIN-PRD.md` §9.5.4.

> **One honest reconciliation** (the reports and the PRD ask *different* questions, both
> true): §5.2 answers "does the host *declare* a `roots` capability" → four do. §9.5.4 of
> the reception PRD answers the narrower "is any of m1nd's *four live bridge hosts* proven
> to *serve `roots/list`* on the wire today" → **none is proven yet**, so reception treats
> roots as *a refinement to `resolved_via`, never the v1 truth source* (env + cwd is). No
> contradiction: capability-declared ≠ wire-proven-for-our-callers. The matrix records the
> capability; the reception protocol still rides env+cwd until a live `roots/list` is
> observed.

### 5.3 sampling / elicitation — post-north enhancement only (rung 8)

Server-initiated `sampling`/`elicitation` exist **only during an active request** — they
**do not exist before turn 1**, so they can never be a delivery channel (`oss-spec.md` §B.3).

| Host | sampling | elicitation |
|---|---|---|
| **VS Code** | ✅ | ✅ |
| **Cursor** | ✗ | ✅ (v1.5) |
| **Claude Code** | ✗ | ✗ |

**Legitimate use: *inside* the north flow only** — structured clarification or summarization
*after* orientation has begun. **Post-north enhancement, never the ambient channel.**

### 5.4 Discovery — registry LIVE vs `.well-known` unshipped (rung 5)

- ✅ **Official MCP registry is LIVE** — `registry.modelcontextprotocol.io`, `server.json`
  as `io.github.<user>/<server>`. **Publishing m1nd there is a cheap, real action** (§6b).
- ⏳ **`.well-known/mcp.json` is UNSHIPPED** — SEP-1649/1960. **Watch, do not build.**

---

## 6. What m1nd builds next — the honest gap list

Straight from the research, stated as **built / unbuilt / unverified** — never blurred.

### 6a. The `m1nd-north-shim` + teach `m1nd hosts` the TIER-A recipes  *(the gap, named plainly)*

**Built today:** `m1nd agent first-minute … --json` (the host-neutral orientation CLI).

1. **A canonical `m1nd-north-shim` script — SHIPPED 2026-07-03.** `npm/bin/m1nd-north-shim.js`
   is the thin, fail-open wrapper that runs `agent first-minute`, renders the packet to
   compact text, and prints the exact `{"hookSpecificOutput":{…,"additionalContext":…}}`
   envelope every TIER-A hook needs. Registered as the `m1nd-north-shim` bin; every §3 recipe
   now references it instead of asking the operator to write the two-liner by hand.
2. **Teach `m1nd hosts plan/apply` the TIER-A recipes — SHIPPED 2026-07-03.** `m1nd hosts`
   now resolves every covered host by name and, via `m1nd hosts plan`/`m1nd hosts apply`,
   emits the **Kiro `agentSpawn`**, **Qwen `SessionStart`**, **Codex `SessionStart`**,
   **Cline `TaskStart`**, **Continue-CLI**, and **Grok-fork** hook recipes plus per-host
   doctrine files — turning this document's §3 into `m1nd hosts apply <host>`. `plan` is pure
   print; `apply --yes` writes owned hook JSON (merge, never-clobber) and doctrine files, and
   PRINTS the Claude/Cline/Kiro host-managed hook blocks rather than writing them.

### 6b. Publish `server.json` to the official registry — **SHIPPED 2026-07-04**

Published: `io.github.maxkle1nz/m1nd` **v1.3.0** is live on `registry.modelcontextprotocol.io`
(device-flow auth by the maintainer as maxkle1nz; verified via the public search API). Ownership proof =
`mcpName` inside the npm package `@maxkle1nz/m1nd@1.3.0`. Gotcha learned: the registry caps
`description` at **100 chars** — the repo `server.json` carries the published 95-char form.

### 6c. The `[unverified]` probe list — with verification steps

These are the open questions the fleet could not close from docs; each needs a live probe
(`oss-spec.md` §"[unverified] remanescentes"):

| # | Probe | How to verify |
|---|---|---|
| 1 | **Gemini CLI `instructions`** | re-grep upstream source for the append site |
| 2 | **VS Code `instructions` render** | hands-on probe: register a server with a distinctive `instructions` string, inspect the model's context |
| 3 | **Continue `additionalContext` contract + IDE scope** | read post-#11029 source for the exact injection field + whether the IDE build ships the hook |
| 4 | **Kilo `roots`** | probe with a filesystem MCP server that requests `roots/list` |
| 5 | **OpenHands / Devin `instructions`** | grep source / live probe |
| 6 | **VS Code `roots`** | definitive wire probe (`roots/list` round-trip) |
| 7 | **Goose TIER-A viability** | probe whether its SessionStart stdout reaches context as `additionalContext`; if yes → promote Goose B→A |
| 8 | **Antigravity `instructions`→context** | the highest-value open question (docs JS-rendered, fetch empty) — likely NO by the Windsurf-fork divergence, but worth a live check |
| 9 | **grok-cli fork / Cursor(after-fix) `additional_context`** | verify the field name the hook must emit |

---

## 7. Provenance

Research fleet **2026-07-03** — five reports (`warp.md`,
`cursor-opencode-aider-qwen.md`, `cli-hosts.md`, `ide-hosts.md`, `oss-spec.md`).
Verification labels (`[verified-docs]` / `[verified-hands-on]` / `[unverified]`) are
**preserved from the source research** and must be maintained on any edit. The orchestrator
hands-on corrections (Claude Code renders `instructions`, §5.1; the roots capability
resolution, §5.2) are folded in and labeled `[verified-hands-on-live-session]`.
Cross-references: the shim (`skills/m1nd-universal-agent-pack.md:160-176`), reception
(`docs/TWO-TIER-BRAIN-PRD.md` §9.5 / §9.5.4).
