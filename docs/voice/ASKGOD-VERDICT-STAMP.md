# askGOD verdict — the low-friction sovereign stamp — 2026-07-12

> Seat: askGOD verdict (Fable), marcha medium, 10 files across both repos.
> Judged BEFORE implementation. Owner's pain: "ratify via chat / MCP command — opening
> the page every time is friction" + addendum G: "a ratify-all-candidates button on the
> h4nd tray menu". 3 legitimate receipts stuck in merge_wait for 2 days = sovereignty
> not exercised is theater by another road.

VERDICT: CHANGE
CONFIDENCE: alta

## The bombshell the oracle found

**`receipt_import` has NO origin gate at all** (system_blocks_handlers.rs:480-520 — pure
OCC + anti-poison; the `human-tray-landing` emitter is a CLIENT-composed string,
missions.ts:665, never validated server-side). Only `ratify` carries the origin token
(handlers.rs:441-456) — and its own comment confesses the token is forgeable on an
unauthenticated loopback ("closes the CHEAP vector, NOT a malicious same-UID process").
The sacred law is ~80% doctrine, ~20% mechanics. And on THIS host, agents provably hold
computer-use/Playwright — **every click-only gesture (including today's :1338 click) is
synthesizable**. Therefore Touch ID is not a nicety: it is the first real mechanical
elevation of the law since ARC-1 (LocalAuthentication; fallback = the OS password dialog,
NEVER a plain click).

## Judgments on the options

- F (MCP ratify tool): REFUSED — MCP is a model-composed channel by definition; any
  secret the agent transports, the agent controls. No defensible MCP-pure design exists.
- A (tray as stamp house): contradicts h4nd's WRITTEN laws 1-2 ("the tray is the BELL,
  not the hand"; laws "non-negotiable even by the new custodian") — requires a FORMAL
  constitutional amendment by the owner, never a silent bugfix. "The tray is the door"
  does not appear in h4nd docs.
- B (actionable notification): BELL ONLY — it opens the fact-prompt; direct-import button
  rejected (blind decision + stale OCC inside a notification + unproven display on an
  unsigned dev build).
- C (deep-link m1nd://): carries ONLY the sanitized mission_id; the prompt's FACTS come
  from a FRESH snapshot read by the tray-app at open (the agent must never frame the
  decision with stale facts); OCC re-verified at click.
- D (Touch ID): PROMOTED to structural piece of the native prompt. `ratified_via:
  "human-touchid"` distinct and auditable.
- E (ephemeral code in chat): PARKED unless the owner insists in writing — and then:
  code bound to (mission_id, store_version), single-use, TTL≤60s, rate-limited,
  constant-time compare, and the "screenshot is gesture-gated" premise declared in
  writing as a validity condition (unbound codes are replayable within TTL).
- G (ratify-all on the tray): batch YES, blindness NO — two sections/gestures NEVER mixed:
  skeleton ratify-all is the EXISTING single OCC call (block_ids omitted, server.rs:2749);
  receipts are N chained mutations (each import bumps store_version) needing a per-item
  ledger (landed/refused/stale_scope), honest partial completion, batch-size cap and
  rate-limit. Default pre-filter excludes: no receipt_candidate, judge-rejected,
  temporal incoherence (server gate inherits per-item). Checkbox exclusion; "never
  opened" items MARKED, not blocking (the fact-lines in the batch prompt ARE the seeing).

## The 10 required changes (condensed)

1. FIRST, before any new door: extend the origin gate to receipt_import — closed
   server-side allow-list of human origins (`human-ui`, `human-tray`, `human-tray-batch`,
   `human-touchid`…), const in code, new origin = code change + test, refusal by default.
2. FORMAL amendment to h4nd laws 1-2 (its own ritual, never silent): the spirit becomes
   "no MACHINE path lands; the human finger on native UI is the same class as the browser
   click"; the grep-guardian evolves to "callable only from the marked human-gesture
   module, with origin token" — new guardian test in the same commit.
3. D structural: LocalAuthentication; fallback = OS password dialog (SecurityAgent),
   never plain click.
4. C: id-only deep-link; fresh-snapshot facts; OCC at click.
5. B bell-only.
6. G two-section design as above.
7. Forensic batch record: ONE persisted line {ts, origin, exact id set, human-excluded
   ids, store_version span} — without it N same-origin imports look like a bot loop;
   with it the batch audits BETTER than N clicks.
8. Implementation order: (0) change 1 — closes today's open hole; (1) tray seal as a
   SIGNING/notarization problem, not just render; (2) single-item native fact-prompt +
   Touch ID reachable from the tray card — this alone kills the 2-day pain; (3) m1nd://
   scheme → same prompt (parallelizable if signing stalls); (4) B bell; (5) G on top of
   the same component — batch never before the single-item prompt exists.
9. E's conditions if it ever enters (above).
10. Validation: origin guard tests on BOTH seams (MCP wire AND REST — the #333 parity
    lesson); adversarial "agent composes native origin" test failing; test pinning that
    cockpit/menu never gains a write pointer; whole flow proven in a real tray GUI
    session; rate/TTL tested.

## Status
- Step 0 dispatched immediately (origin gate on receipt_import — closes the live hole).
- Steps 1-2 queued as the next burst; steps 3-5 after.
- The h4nd constitutional amendment (change 2) awaits the OWNER's explicit gesture —
  its laws declare themselves non-negotiable even by the custodian.
