# PATHOS — m1nd agent handoff

> Read this first. Single source of truth for any chat / subagent / parallel
> session working on m1nd, so we don't re-derive state or contradict each other.
> Last checkpoint: 2026-08-01 (**checkpoint 39 — THE PIVOT: THE PRODUCT HAD NO FIRST-VALUE PATH AT ALL, AND THE LADDER STOPPED SO THE MENU COULD FIT ON ONE SCREEN**).
> Checkpoint 39, condensed: the owner turned the program around and the measurement backed him. **(1) m1nd could not be used by anyone new — measured, not suspected.** A virgin repo, empty graph, fresh agent calling `ingest`: `generic_action_authority_required, authority_floor=POSITIVE_SOVEREIGN`. And the human door, which this session had *pointed at without testing*, exited 0 and produced **0 nodes**: `m1nd init --birth` minted a project brain under `project-brains/<key>/` that only a served owner's HTTP routing reads, while the next stdio session read `runtime.session` — the ceremony filled one brain and the next session opened another. Fixed in #520 (home-birth gated on NODE COUNT, not root coverage, because a virgin runtime demotes `workspace_root` to `<repo>/.m1nd` and the coverage guard misses); `birth_produced_empty_graph` now refuses a ceremony that would exit successfully with nothing; every refusal on that path names `m1nd init --birth <repo>` — an experienced external agent had hit FOUR refusals, none of which mentioned the door, and concluded in a written report that the product could not be used. **(2) The measurement that turned the program.** Six weeks of host transcripts, deduplicated: **458 distinct calls / 157 sessions / 34 verbs of 141**. 57% of sessions call m1nd exactly ONCE and never again; 71% never make two calls in a row; the two commonest actions after a m1nd verb are Bash and Read — the agent goes and fetches by hand what m1nd just pointed at. Narrated in 76% of north calls, ACTED ON in 32%. The verbs that exist to close that second hop were at zero (`surgical_context`: 1 call in six weeks — and ranked top-3 by an external evaluator told to exercise everything). Answer shipped: #517 folds the code into `north` (5,576 → 8,116 chars, ~25% of the hard ceiling), #514 gives m1nd its first usage telemetry (verb names and counts only, privacy enforced by mechanism — caller text cannot reach the file), #518 makes the first minute find the served owner, #516 stops every ingest from erasing the Hebbian counters (73,332 edges, ALL zero in production — the signature feature had never accumulated anything because the graph is replaced wholesale and nothing re-imported the sidecar). **(3) The menu now fits on one screen (#521).** 141 advertised → 15 served (the owner's ratified 12 plus `help`/`doctor`/`recovery_playbook`, which the honest surface mechanically requires); 127 hidden, ZERO removed — proven on the wire by calling a hidden verb by name and getting an answer. Discovery three ways: `help` reads the FULL registry (it was reading the tier-gated list — a documented gap), the menu's help entry carries a computed hidden count, `tool_surface_contract` gained `hidden_tool_count`/`hidden_tools_are_callable`/`discovery_rule`. Mechanism not list: `CORE_TOOLS ∪ HOST_BINDING_REQUIRED_TOOLS` bounded by a COMPILE-TIME assert. **A confounder found in passing: the owner's serve carries `M1ND_TOOL_TIER=full`, which is exactly why the six-week audit observed 141 advertised — that number was an artefact of the development machine's config, not the product's default.** Ratified: the variable stays (the machine that builds the product is not an adoption sample). **(4) Floors, measured before touching:** the catalog holds **166 actions, 111 above Ordinary, and 3 with any written justification** — and the three are the same comment. Cross-referenced against 242 letters: **12 describe REAL damage**, and the ones that sustain a floor are one family — cross-brain writes ("moved this repo's memories into the WRONG brain's store"). That family is frozen forever. The rest were inherited BY NEIGHBOURHOOD IN THE CATALOG — the section dividers are visible in the source (`// Durable trails, perspectives, locks…`), so a section comment became security policy. Same defect shape as the first-value bug: `ingest` with no `mode` fell into `replace`'s classification by lazy classification, not by decision. Floors await small lots with a declared denominator per lot; nothing was touched. **(5) The custody chain closed at G9's door and stopped there.** The Apple wall fell (a Developer-ID bundle with an embedded provisioning profile launches with the restricted entitlement — measured, not inferred), the ceremony bundle ships (#512), and the owner's OWN first `provision-seats` found the bug no test could: the never-open-or-create duplicate guard treated `errSecItemNotFound` as fatal, so on a fresh ceremony — the only time provisioning is legitimate — the question "does this seat exist?" aborted. #519. The software fake returns `Ok(None)` cleanly; fake and Apple diverge at exactly that line. **The ceremony is unblocked but unrun: it needs a v1.6.3 to carry the fix into a signed, entitled bundle.**
>
> Previous checkpoint: 2026-07-30 (**checkpoint 38 — THE GATES REVEALED THEMSELVES AS ONE CHAIN, THE OWNER RAN HIS FIRST CEREMONY AND IT ANSWERED HONESTLY, AND THE PRODUCT'S BRAIN CAME HOME TO ITS OWN REPO**).
> Checkpoint 38, condensed: fifteen PRs merged since checkpoint 37, measured (#460–#466, #468–#472, #474, #477–#478), seven more armed in flight (#446, #467, #473, #475, #476, #479, #480). **(1) The ladder is a provenance chain, not a list.** The owner ran G7 LIVE with his own hand — the program's FIRST owner-performed ceremony — and it answered `NOT_PROVEN: manifest coherence is DRIFT` (receipt written to the owner's disk), which is the CORRECT answer: `organism_manifest.rs` holds G1-truth at DRIFT until the release/autonomy authorities exist, so the real order is G9 custody → G8 signed release → G7 LIVE → G10, and no ceremony below the chain's head can prove early. The prior claim in this program that G7 was "the only ceremony without external blockage" was wrong and is corrected here. **(2) Every ceremony the machine can stage is staged.** G6 formal: one owner command + preflight — 22 PASS / 0 FAIL / 15 OWNER_INPUT_MISSING, exit 3 `READY_PUBLIC_ONLY` (#472). G7: runbook + typed expectations script + digest discipline (#470 — and the runbook's own staleness was caught live: it cited a superseded bundle digest while the build demanded the post-#474 one, exactly the drift `build.rs` exists to refuse). G9: the ceremony runbook that MEASURED the custody floor as an unwired island (#468), the CLI door (#473: `--custody-ceremony`, `OwnerCeremonyIngressV1` as a private-field ingress constructible at exactly one site, the first non-test caller of `assemble_production_owner_authority_v1`), and the entitlement the shipped binary was structurally missing (#469: before it, `release.yml` signed with no `--entitlements`, so the notarized product COULD NOT run the owner's ceremony at all; AMFI rejects XML comments anywhere in an entitlements plist and `codesign` exits 0 while silently dropping the entitlement — both proven on a probe binary, so the release step now greps the signed output; rationale in `build/README.md`). **(3) The product's brain came home.** The product's own repo could not reach its own graph: the committed `.mcp.json` spawns stdio over an empty repo-local runtime while the served owner (18,084 nodes / 73,332 edges) ingests this very repo one port away, and the machine's user-scope attach entry both lacked the token env and LOST scope precedence to project scope. Machine half: a local-scope attach entry carrying `M1ND_HTTP_BEARER_TOKEN_FILE` — proven by the first live `north` through the bridge (full_trust, spread-activation reaching the week's own SPEC-1 battery). Product half, for every user (#480): `--attach auto` now asks the SECOND discovery question — which live serve ReadWrite owner declares an ingest root COVERING my caller root — coverage decided by the routing predicate (`covers_root`), never the authority-exclusive one; worktrees resolve to the main repository for discovery while the hop-2 header keeps the TRUE caller root (normalizing it would forge the exact-root claim SPEC-1 exists to refuse); ambiguity fails closed naming every candidate; a runtime-root match still wins (pinned by test); the bearer token follows the DISCOVERED owner's runtime root. Battery-first 8-case RED→GREEN, live-proven from a real worktree against the production registry. Four letters filed from that mission; the sharp one: `default_registry_root()` still cannot see this machine's owner (per-host registry split — owner's wiring call). **(4) The factory's own gates bit correctly all night:** the `brain_birth` advertised-vs-routed parity guard caught #467 (fixed with the comment saying WHY it routes); the agent-docs gate refused #446 for touching `server.rs` without teaching agents (paid: the plasticity wiki page now says restore lands in BOTH engines); the a11y lane's bite was proven honestly after a first false probe (an appended comment is stripped by the minifier — re-proven by removing the nav aria-label, 2/4 specs red, recorded in the PR); windows-required caught spec2's path-identity family the Unix legs are structurally blind to. Tonight's tray red is diagnosed three ways — #467 spec2 windows paths, #473 custody refusal precedence (unattended vs not_installed on non-mac), and the ABBA serialization flake — with three executors dispatched on the first two plus the #476 conflict, and the docs-gate fix landed inline. **(5) Dogfood is real and telemetried:** field reports include the first win through the repaired bridge, and a sibling agent's "are we even using m1nd in development?" report was answered by REPAIRING the bridge, not by rhetoric. **Open the owner must touch:** 2 missions in merge_wait (the bell has rung three times); #398 MANUAL public-boundary decision; #419; #423; canonical git email; bincode #431; metric spec v2 minting (custody-bound); the ceremony chain itself G9→G8→G7→G10. **Open machine-side:** the tray in flight; the shadowed-REST-verb table guard (deliberately waiting for #475 to land so its table includes `curation_spawn`); the G6 provider executable; the shadow/canary producer; the runtime half of the bundle blind spot; m1nd-ui eslint; refreshing the serve binary onto this arc's code once the tray lands, then the lifecycle re-proof.
>
> Previous checkpoint: 2026-07-29 (**checkpoint 37 — THE FACTORY NIGHT: LIFECYCLE GATE LANDED, GENESIS RATIFIED, THE LIVE BRAIN REFRESHED ONTO THE WEEK'S OWN CODE, AND 40 ADVERTISED VERBS STOPPED LYING**).
> Checkpoint 37, condensed: one owner-present night (00:30–03:30), eight parallel Opus missions, 8/8 returned, nine PRs merged in the arc (#444–#445, #447–#450 + earlier #441–#443) with three more armed (#446, #460, #461). **(1) The live brain runs the week's own code.** The serve owner and runnerd were both on `1.4.0` binaries built from working trees that never became commits (two DIFFERENT phantom shas across the two profiles); both now run `1.5.0 (e792b401)` clean — and the refresh was only safe because #441/#442 were already on main. The boot log is the proof the era needed: **21,885 nodes and 84,347 synaptic records imported clean through the #442 reader** (the owner's real graph carries parallel edges; the old reader would have refused its own file on the next boot). One honest wrinkle: launchd recorded `OS_REASON_CODESIGNING` kills on intermediate restart attempts (a race between kickstart and the binary swap) — both services stabilized; watch for restart loops. **(2) The lifecycle gate exists and bites (#447).** Designed by an askGOD verdict (CHANGE, applied — its own-find: `persist_runtime_root.rs` already drove the REAL binary over stdio JSON-RPC and was ~80% of Cycle A; in-process was structurally impossible, `pub(crate)`), it walks one runtime root across FOUR boots, covers BOTH durable-write families a plain client can reach (classified `memorize`, debounce `alerts_ack` — durable to nobody unless the shutdown checkpoint carries it), asserts zero sidecar refusals on captured stderr, and goes RED with the field's exact signatures when either boot fix is reverted. Cycle B (crash/kill-9) stays declared NOT_RUN. **(3) Genesis is RATIFIED (#448).** The confirmation verdict (glm full) caught the dossier's own stale premise — the `ingest project_root=` first-contact lie was ALREADY swept by #405 and guarded; what lives is narrower: the floor-gated verbs. The owner then answered all four §6 items individually (SPEC-1 into the queue at `ScopedGrantA2`; shrink-floor 60%; SPEC-2's `human-cli` minted only by the P2 ceremony) — implementation is now unblocked, battery-first. **(4) The honesty sweep (#461):** 40 of 141 advertised verbs (33 fully, 7 partially) refuse plain dispatch — measured live when the lifecycle build tried `antibody_create`. Verdict-ratified mechanism: ANNOTATE, never filter (filtering hides what P2/P3 will unlock and courts the cp31 class). Annotations derive from the SAME floor table the gate enforces, so a future verb cannot ship advertised-but-unannotated; the six typed G2/G3 consumers are excluded by documented constant because the mission wire intercepts them AHEAD of the gate. **(5) The one reachable mutating verb was silently lossy (#460):** the light-ingest merge behind `memorize` erased parallel edges — the exact content #442's positional binding depends on. Verdict (A): parallel edges are legitimate; the merge now preserves without breeding, proven end-to-end through the verb and against the lifecycle gate. Same defect class killed twice more: friendly-boot plasticity landing in one of two engines (#446) and the legacy-adoption path repeating it with a half-reset failure arm (#449 — built by an executor that died on session quota AFTER committing; orchestrator verified post-mortem and said so in the PR). **(6) sha2 (#450) closed as BLOCKED-BY-ECOSYSTEM, the right outcome:** `ed25519-dalek`/`p256` pin `sha2 ^0.10`, so a solo migration merely straddles majors (measured with `cargo tree`; 0.11 is ALREADY in the lock via rust-embed-utils); hash outputs proven byte-identical; the coherent move is the whole RustCrypto sweep as ONE security-reviewed PR (letter filed; dependabot's fresh #459 p256-0.14 belongs to it). A curated dependabot ignore stops the weekly re-litigation. Also corrected en route: **the lib suite is not flaky — it is SLOW** (>560s full; every apparent failure under load was a harness-timeout SIGTERM killing runners mid-green; chunked, everything passes deterministically). **(7) Coordination laws, each from a real bite:** per-checkout `CARGO_TARGET_DIR` mechanized (#445 — the false green reproduced LIVE: worktree A's gate ran a test that exists in no file of A and reported ok; helper + derived CI guard + doctrine + watchdog, since amended by the owner to a TOTAL threshold after 11 worktrees hit 47G with none over 15G alone); NEVER kill processes by name/pattern (an external SIGTERM took a sibling's suite mid-gate — second incident); full concurrent suites cap at 2; and the sneakiest: **monitors die silently when they arm inside a session-quota window** — two finished executors sat orphaned with green suites and uncommitted work until the owner's ping prompted a sweep; orphan-check-on-quota-return is now the orchestrator's duty. First complete inbox triage in project history ran the same night (170 lines = 99 letters = 60 defect reports = 42 groups; 32 already fixed on main; 24 live, ranked) and caught the PREVIOUS triage having ranked two already-dead defects as its top priorities from reading 74/168 lines. **Owner's grades, honest (03:31):** idea/graph-core 9 · continuity was-2-now-6.5 · honesty was-5-now-7.5 · width-vs-use 4 (138 verbs, 29 ever called — the product's realest number) · authority 8-design/6-integration · overall 7-and-rising-with-proof. **Open next:** genesis implementation (SPEC-1 battery-first → P2 → SPEC-2), the Windows flip to the required matrix, the RustCrypto sweep, Cycle B, the REST middleware-order high letter (`mission_spawn`/`candidate_naming` dead behind 403 on their designed path), dependabot batch #451–#459, bincode #431 (needs serialization-compat analysis), and the owner's queue: #398 · #419 · #423 · canonical email · two missions in merge_wait.
>
> Previous checkpoint: 2026-07-27 (**checkpoint 36 — WINDOWS PHASE-2 IS CLOSED, AND THE NUMBER THAT NAMED IT WAS THE LIE; THE OWNER'S BRAIN HAD BEEN SERVING ZERO OF 5540 NODES FOR FIVE DAYS**).
> Checkpoint 36, condensed: eleven PRs, two fronts, and one lesson that outranks both. **(1) Windows phase-2 is closed — and "~22 tests" was never its size.** The source-edit path-canon suite fell to a single diagnosis: every durable identity is written through `path_text()`, which STRIPS the Windows verbatim prefix (`\\?\C:\repo` → `C:/repo`), while every live path comes from `fs::canonicalize`, which ADDS it — and `Path` compares prefix components BY KIND, so `VerbatimDisk('C')` never equals `Disk('C')`. Two identity domains, identical on Unix and DISJOINT on Windows (#435; the containment test stays component-wise `strip_prefix`, never a string prefix). Then the front kept moving, because `cargo test` stops at the first failing binary: source-edit was MASKING a transplant-harness defect (#436 — where the product was correct and the harness's own path keys were not), which masked four runnerd fixtures spelling absolute paths the Unix way only (#437 — `/abs/repo` has a root but no prefix on Windows, so it is not absolute and the product is right to refuse it), which masked three `needless_return` (#438) and two dead functions (#440) inside `#[cfg(windows)]` blocks that a green Unix clippy is structurally blind to. **A count taken behind fail-fast measures the first layer and calls it the total**, so the advisory leg now runs `--no-fail-fast`: an advisory job exists to MEASURE debt, and one that reports a layer at a time measures nothing. Final state, measured not estimated: **63 test binaries green on Windows, clippy clean**. Method that paid: three Windows-only claims were proven by MIRROR PROBE — flipping `cfg(unix)`↔`cfg(windows)` locally compiles the exact branch Windows compiles — and a portability review refuted a claim this session had written into a PR body (a `PathBuf::push` that REPLACES the buffer when a `Normal` component carries a prefix, which the same session's fix had made load-bearing); the correction was stated out loud in the PR rather than edited in silently. **(2) The guardian spent the whole night developing m1nd while running a m1nd that could not test anything.** The runtime was `m1nd-mcp 1.4.0 (b41883c9…-dirty)` — built from a working tree that never became a commit — reporting `node_count: 0`. Every doctrine step that says "orient with m1nd first" was a silent no-op for hours, and nothing in the tooling said a word: **it surfaced only because the owner asked whether m1nd was working at all.** A stale binary does not fail loudly; it answers, plausibly, about code that is not yours. Owner's rule, now doctrine: *you do not work on what you cannot test, and keeping the binary current must be automatic, not remembered* — `scripts/m1nd_selfhost_refresh.sh` builds from HEAD, installs, and PROVES `binary sha == HEAD`; a `SessionStart` hook reports drift AND an empty graph together, because a current binary over an empty graph is equally blind and empty retrieval is NOT calibrated absence. **(3) What that unblocked is the real find: three independent defects, all in the boot path, all bricking.** (a) `BrainActorHandle::start` restores checkpoint `CURRENT` and rebuilds the whole `SessionState` from disk; this repo's `CURRENT` was dated Jul 22 and pinned `graph_snapshot.json` to the digest of the EMPTY graph — so **every boot loaded 5540 nodes and served 0, and overwrote the runtime copy with empty, for five days**. The 1.5 legacy adoption walked straight into it: it wrote the pre-1.5 graph before any actor existed, the actor reverted it on the same boot, and the journal still recorded `status: "adopted"` — **the rescue was permanently spent without ever having worked** (#441: adoption moved INSIDE the actor boundary so it commits through the checkpoint; the journal writes only after the ACK and a reverted adoption is re-adoptable; the rejected alternative — an uncommitted file outranking `CURRENT` — is pinned as CORRECT by its own test so nobody "fixes" this later by inversion). (b) plasticity `persist → import` was not a round trip: parallel edges were written as two rows with the same key and the reader refused them, so **a clean shutdown bricked the next boot** (#442). (c) a graph-stale co-change sidecar returned `SchemaDrift` through a `?` in `SessionState::initialize`, so `McpServer::new` died and all 48 MCP tools vanished — where every sibling sidecar degrades with "continuing without it" (#442; detection untouched, only the consequence changed). **None of the 1458 green tests caught any of the three**, because every one of them is a unit and nothing tests boot → serve → mutate → clean shutdown → boot again → still serves. **That is the completeness gap, stated as the next front:** m1nd has 138 registry verbs (48 advertised, 29 ever called in six weeks of real use) and no end-to-end lifecycle proof — and it is a CONTINUITY system, whose whole promise is that state crosses time. The single highest-leverage work available is a lifecycle gate in CI; that one test would have caught all three. Two method laws learned the hard way: a shared `CARGO_TARGET_DIR` across parallel executors is HAZARDOUS (two worktrees produced the same binary path — executors can validate each other's binary, and one killed the other's test process believing it was a duplicate), and do not run git in a worktree while its ingest tests are scanning it.
>
> Previous checkpoint: 2026-07-26 (**checkpoint 35 — THE RELEASE PUBLISHED BUT DID NOT INSTALL: macOS SIGNING + NOTARIZATION WIRED AND CREDENTIALED; THREE AUDIT CLAIMS GRADED**).
> Checkpoint 35, condensed: installing the PUBLISHED 1.5.0 the way a stranger would exposed the gap the release pipeline could not see. The npm updater behaved exactly as designed — `state: stale`, `1.4.0-dirty -> 1.5.0`, then a clean REFUSAL because `cosign` is absent (it never degrades to an unverified path). The GitHub asset verified against the release `SHA256SUMS`, but its FIRST run parked in uninterruptible `UE` state: the shipped macOS binary is ad-hoc signed only (`codesign -dv` shows no Developer ID authority), so Gatekeeper blocks every user's first launch of a 69MB unsigned binary. **The release published, but it did not install well.** PR #433 (merged) wires Developer ID signing (hardened runtime + secure timestamp) and `notarytool` notarization BETWEEN the build and the staging steps, so both the raw updater-facing binary and the tarball carry the signature; the honest posture is that absent Apple secrets the step SKIPS loudly (today's behaviour) while present secrets make any failure FAIL the build, with a final check proving an `Authority=Developer ID Application` on the shipped bytes (a raw executable cannot be stapled — Gatekeeper resolves the ticket online, stated not hidden). The credentials are now REAL: a `Developer ID Application: Max Elias Kleinschmidt (4KLJ4N9D5K)` certificate (G2 Sub-CA, valid to 2031-07-27, minted from a locally generated CSR so the private key never left the machine) and an App Store Connect API key scoped to `Developer` only; all six `APPLE_*` secrets are set and verified, and the key/cert pair was proven to match by modulus comparison. **The next tag ships signed and notarized.** Audit claims from an external reader were graded rather than trusted: the `sha256`-field-computed-with-`DefaultHasher` claim was REAL but had already been fixed on main by a parallel session (`content_sha256` now calls the true `sha256_bytes`, guarded by a 64-hex-char test); the `agent_id` falls-back-to-`unknown` claim is PARTIALLY REFUTED — 136 tool schemas declare `agent_id` required, handlers refuse without it, and the write path types it as non-optional `String`, so the `"unknown"` lives only in an SSE telemetry label, not an authority path; the root-orphans claim was PARTIAL (one untracked third-party web directory is a genuine leak-guard risk, two were inert). Method note that paid for itself twice tonight: verify before acting, and verify before DECLARING — a `find -newermt "-3 minutes"` window wrongly pronounced a downloaded API key lost when it had been in `~/Downloads` all along.
>
> Previous checkpoint: 2026-07-25 (**checkpoint 34 — THE GRAPH LEARNED TO WRITE: `transplant` promoted from an isolated prove-first lab into the main line, with its boundaries stated**).
>
> Previous checkpoint: 2026-07-24 (**checkpoint 33 — AUTO-MERGE RESTORED (PROVEN BY SELF-MERGE UNDER A RED ADVISORY LEG); THE FIRST-CONTACT INSTRUCTION WAS A LIE ON SEVEN SURFACES OF `v1.5.0` AND IS NOW GUARDED BY TEST**).
>
> Previous checkpoint: 2026-07-23 (**checkpoint 32 — THE GENESIS "GAP" IS THREE PROBLEMS, NOT ONE; PANEL-RATIFIED ORDER P1→P2→P3; AGENT-FIRST ARGUES *FOR* THE SOVEREIGN FLOOR**).
>
> Previous checkpoint: 2026-07-22 (**checkpoint 31 — 1.5.0 PUBLISHED BY OWNER OVERRIDE; A DOGFOOD NIGHT HEALED A P0 (MCP tools vanished) + WINDOWS SECURITY/PATH/HANDLE GAPS; A PRE-EXISTING WINDOWS SOURCE-EDIT PATH-CANON SUITE (~22) REMAINS RED, DIAGNOSED, TRACKED**).
>
> Previous checkpoint: 2026-07-21 (**checkpoint 30 — G9 PATH-B SECURE ENCLAVE CUSTODY FLOOR IMPLEMENTED IN AN OPEN PR (proof-grown, not merged; the owner's live custody ceremony is NOT_RUN)**).
>
> Previous checkpoint: 2026-07-20 (**checkpoint 29 — THE ERA LANDED: THE FIRST FROZEN CANDIDATE MERGED TO PUBLIC MAIN BY THE OWNER'S HAND; NATIVE WINDOWS IS DECLARED PHASE 2 OF G4**).

> Previous checkpoint: 2026-07-20 (**checkpoint 28 — ANTI-CEREMONY DOCTRINE RATIFIED: three owner-ratified rules now bound the program's rite — a meta-review ceiling, rite budgeted against real external exposure, and external gates before new internal gates**).

> Previous checkpoint: 2026-07-20 (**checkpoint 27 — THE BOUNDARY CLOSED HONESTLY: EVERY CHECKPOINT-26 REQUIRED CHANGE IMPLEMENTED, THE GOVERNED MIGRATION EXECUTED UNDER OWNER RATIFICATION, THE FRESH INDEPENDENT RE-REVIEW RETURNED `APPROVE`/NONE, AND THE OWNER AUTHORIZED FREEZE+PUSH+MERGE**).

> Previous checkpoint: 2026-07-19 (**checkpoint 26 — FUGU RETURNED `CHANGE`/HIGH: THE CANDIDATE-SOURCE ARCHITECTURE IS SOUND, BUT ITS POLICY IS NOT YET FAIL-CLOSED; REMEDIATION NOW BLOCKS CANDIDATE FREEZE**).

> Previous checkpoint: 2026-07-19 (**checkpoint 25 — THE ORIGINAL CANDIDATE-SOURCE PREFLIGHT PASSED ITS LOCAL TESTS, BUT ITS FAIL-CLOSED CLAIM IS SUPERSEDED BY CHECKPOINT 26 `CHANGE`**).

> Previous checkpoint: 2026-07-19 (**checkpoint 24 — THE G6 CORRECTIVE REVIEW IS CLOSED `APPROVE`, THE SCORER NOW FAILS CLOSED ON RAW FORMAL PROOF, AND THE AUTHORITY ROOT IS PINNED FOR ITS OWNER LIFETIME; THE FORMAL BLIND RUN, IMMUTABLE CANDIDATE, LIVE, RELEASE, AND AUTONOMY ACTIVATION REMAIN OPEN**).

> Previous checkpoint: 2026-07-19 (**checkpoint 23 — THE CURRENT DIRTY WORKTREE NOW HAS ONE GREEN LOCAL AGGREGATE: authority fixtures carry the exact caller root, checkpoint fixtures obey the managed runtime root, concurrency survives the workspace load, and every static/build lane completed; immutable-candidate, LIVE, hosted release, and autonomy activation remain honestly open**).

> Previous checkpoint: 2026-07-19 (**checkpoint 22 — THE G7 LOCAL CONTRACT IS COHERENT: organism/binary/bundle versions agree, private UI provenance remains separate, and the locked dependency set replays offline; browser LIVE remains honestly unrun**).

> Previous checkpoint: 2026-07-19 (**checkpoint 21 — THE G6 CORRECTIVE BURST IS LOCALLY GREEN: all five P1 and three P2/scorer findings are source-closed and adversarially tested, while independent review and the formal blind run remain honestly open**).

> Previous checkpoint: 2026-07-19 (**checkpoint 20 — M1ND-10 HAS A CONTROL PLANE, A DURABLE BRAIN, A CLOSED RELEASE PATH, AND AN HONEST HANDOFF: substantial source implementation exists; the remaining blockers are named, ordered, and forbidden from becoming a false 10/10**).

> Previous checkpoint: 2026-07-15 (**checkpoint 19 — THE HONEST FRONT DOOR, THE UNIVERSE, AND THE CI IS BORN: the critique became receipts, every project became a world, and the review tier grew its first judgment — with zero false approves**).

> Previous checkpoint: 2026-07-12 (**checkpoint 18 — THE VOICE: m1nd speaks to the human in the conversation; the pulse is stamped, the cockpit ships, the scan wait is made honest**).

> Previous checkpoint: 2026-07-11 (**checkpoint 17 — THE HAND CURATES: F12 ratified+implemented, the first autonomously curated map human-ratified, the proof system hardened**).

> Previous checkpoint: 2026-07-10 night (**checkpoint 16 — THE ORGANISM DEFENDED ITSELF: F11 shipped whole, the first HUMAN ratifies, the contamination cured with its vector proven, and the vital sign born**).

> Previous checkpoint: 2026-07-10 (**checkpoint 15 — ANY REPO GETS A MAP: two real baptisms, and F11 ratified — editing with minimum human friction**).

> Previous checkpoint: 2026-07-09 late (**checkpoint 14 — THE LOOP RAN WITH REAL HANDS: spawn → land, field-hardening, the log becomes the demo**).

> Previous checkpoint: 2026-07-09 (**checkpoint 13 — HUMAN VIEW v2 F2.5 WRITE MODE COMPLETE (a/b/c/d)**).
> Previous: 2026-07-08 (checkpoint 12 — HUMAN VIEW v2 specified; F0→F3b + F2.5a/b built and live).
> Previous: 2026-07-05 (checkpoint 11 — THE LADDER IS CLIMBED, the close-out burst).
> The entire ORGANISM ladder R0–R17 is landed on `main` and live on the served owner: medulla
> storage/tiers/promotion (R2/R3/R4), delegation (R6/R7), the per-project mailbox + UI (R8/R9),
> Pre-Flight (R10), reconnect-rebind (R13), per-brain counters (R14), eviction (R15), the soul
> (R16), seek-rerank conformance (R17) — and R11's deliverable, `docs/CASE-INTELLIGENCE-PRD.md`,
> shipped as the last rung's PRD (its implementation slices S0–S4 are the next construction seed).
> The close-out burst also wired the M5a migration CLI (`--medulla-migrate plan|apply|rollback`),
> made `restart --binary` honest (loud dry-run + codesign + kickstart), fixed the mailbox roster's
> duplicate-basename misrouting to the honest abstain, ran the first real `--inbox-sweep`
> (idempotence proven live), and brought `skills/` to the serve-attach/medulla/delegation/soul era.
> Checkpoints 33 → 11 above are title-only (condensed bodies pruned 2026-07-30 per the ≤5-checkpoint
> hygiene rule; full text in git history). Prior checkpoints (10 → 7) are summarized in **Prior Eras**
> below; full text in git history.

## HONEST HANDOFF — from the 2026-08-01 session to whoever comes next

Written on the way out, without defence. Declared self-criticism is a gift to the
next agent, not a confession. Read this before you trust anything else I wrote.

**The owner's standing definition of done for the current front, verbatim
(2026-08-01):** *"o m1nd tem que voltar a ser rápido, fácil, e que o agente tenha
VONTADE de usá-lo porque realmente é incrível e resolve seus problemas."* Judge
every change against that sentence. In proof terms: a virgin repo's first `north`
must delight in seconds, the menu must fit on one screen, and the telemetry must
show adoption moving — action rate (32% baseline) and live verbs are the needles.

### (a) Mistakes I made more than once

1. **I claimed a path worked without walking it.** Twice. I read `m1nd init --birth`
   in the source, saw it existed, and told the owner it was "the door" — it exited 0
   and produced an empty graph. Then I told an external agent to "re-ingest from
   scratch" without checking whether it *could*; it burned four attempts discovering
   it could not. Reading that a capability exists is not evidence that it works.
2. **I reported a number I had not measured myself.** I passed on "55 floors without
   justification" from a delegated sweep; the real shape is 166 actions, 111 above
   ordinary, 3 justified — the 55 was a coincidence with a different total. It was
   caught by a review gate, not by me. Any number that will steer work gets
   re-derived before it leaves my mouth.
3. **I truncated a delegated result and reported from the fragment.** A `tail -80`
   cut the head off a sweep's output; I synthesised from what survived and only
   noticed when the totals refused to add up.

### (b) Beliefs I carried too long before re-measuring

- That the product's first-value path worked, because the README said so. It had
  been dead for weeks, and the README sentence was simply false. **The docs are not
  evidence about the code.**
- That G9 was "one tag and a gesture away". Measuring the manifest showed the chain
  needs authorities that do not exist yet — the ceremony proves G9 and moves G7 not
  at all. I had implied otherwise for a day.
- That the six-week audit's "141 advertised" was the product's default. It was the
  development machine's `M1ND_TOOL_TIER=full`. **Measure the shipping default, not
  the machine you are typing on.**

### (c) Habits my successor should NOT inherit

- **Reporting an executor's result without re-running its proof.** I mostly verified
  with my own hands and it repeatedly paid — a subagent's diagnosis was wrong about
  a schema in a way that, if trusted, would have reproduced a known incident (a
  naked `oneOf` once wiped every tool from a live session). Where I skipped the
  check, that is where errors reached the owner.
- **Framing a refusal as a wall.** The most expensive defect of this session was not
  a crash: four correct refusals that never named the way out made a competent agent
  write "this product cannot be used". If you add a refusal, add the door.
- **Letting the ladder set the agenda because it has a scoreboard.** Gates, receipts
  and ceremonies produce visible progress; adoption does not. That asymmetry is why
  a month went into governance while the front door was broken. The telemetry
  (#514) exists now precisely so use has a scoreboard too — use it.

### (d) What I would do differently starting from zero

- **Write the path test before the capability.** The first-minute path had no
  end-to-end guard, so it died silently. One test that walks virgin repo → populated
  graph → first answer would have caught it the day it broke.
- **Ship the smallest surface that works, then widen on evidence.** 141 advertised
  verbs for 13 used is not generosity; it is a menu nobody can read. The core is a
  *ratified judgement, not a proven one* — let the needles confirm or refute it.
- **Never let a floor be inherited by neighbourhood.** Each one should carry, in
  code, the incident or the reasoning that justifies it. 3 of 111 do.

### (e) State the successor inherits, precisely

- **#521 (menu of 15)** — **RED at handoff on all three OS legs**:
  `cockpit::tests::cockpit_budget_holds_with_the_eighth_slot` panics at
  `m1nd-mcp/src/cockpit.rs:849` (1608 passed, 1 failed). Causally plausible and
  probably a real coupling rather than a flake: the cockpit budgets against the
  tool surface, and the surface just shrank from 141 to 15 — the eighth slot's
  arithmetic likely assumed the wide menu. Fix the coupling (or the test's
  premise) before merging; **do not merge it red**, and do not "fix" it by
  widening the menu back.
- **#520 (first graph can be born)** — **RED at handoff**: `Rust gates (ubuntu)`,
  `a_runtime_at_the_repo_root_is_home_too_and_its_empty_graph_is_filled` panics at
  `m1nd-mcp/tests/first_graph_is_born.rs:219` (2 passed, 1 failed). The fix is real
  and proven locally on a virgin repo; this third case is the one where the runtime
  sits AT the repo root rather than inside it. **Do not merge it red.**
- **Two bug executors were in flight** when the handoff order arrived and were not
  killed (their RED batteries were already committed; abandoning half-proven work is
  worse than letting a gated PR finish): `trace` path-identity, and `search` regex +
  the silent `perspective_inspect` schema. Check for their PRs before starting either.
- **Not started, by order:** `tremor`/`trust` empty-without-history — the external
  evaluator's suggestion (fall back to git log) is a *product decision*, not a bug
  fix, and belongs at the owner's table first.
- **Floors:** 111 above ordinary, 3 justified, cross-brain family frozen. Small lots,
  denominator declared per lot, gate per lot.
- **Custody:** unblocked, unrun — needs a v1.6.3 to carry #519 into a signed bundle.

## Open fronts — the declared debt (2026-07-24, delta 2026-07-30)

Everything promised or planned and not yet done, named here rather than left in a chat.
Update this list in the same PR that closes one; a front that dies silently is a lie.

**Delta 2026-07-30 (current; items below this block reflect 07-24 and several have since closed — see checkpoints 37–38):**
- **Owner's queue, unchanged and ringing:** 2 missions in `merge_wait` (bell rung three times through `north`); #398 MANUAL public-boundary decision; #419; #423; canonical git email; bincode — the serialization-compat analysis is DONE and the migration is open as its own PR (fixtures frozen from the pre-bump stack FIRST, `config::legacy()` at every call site, full-consumption armor), so what is left here is the owner's call on the superseded #431; metric spec v2 minting (custody-bound); the ceremony chain G9→G8→G7→G10 (each step is the owner's hand, in that order — see checkpoint 38 clause 1).
- **Tray in flight:** #446 (docs-gate fix pushed), #467 (spec2 windows paths — executor), #473 (custody refusal precedence — executor), #475 (ABBA flake rerun), #476 (conflict merge — executor), #479 (CI pages/i18n, its python-proof red under diagnosis), #480 (attach-auto second question, verified RED→GREEN by the orchestrator's own hands).
- **G9 custody, measured after the verb wiring:** all five `--custody-ceremony` verbs now reach the enclave floor instead of answering from a placeholder — `provision-seats` mints the four seats the owner's `IndependenceSpecV1` names (plus the ceremony's sealing seat), `owner-seat` mints the biometric seat through a new owner-only entry point that mirrors the agent one's refusal, `seal` builds/validates/binds the receipt and enclave-seals it, and `seal` + `assemble` open the ceremony root through ONE function so what assemble reads cannot drift from what seal wrote. The runbook's §4 table of zeros is closed and is now a test. **Still NOT_RUN and unfakeable:** no enclave key has been minted, no Touch ID answered, no `custody-ceremony.sealed.json` exists; on any local (unentitled) build every custody verb refuses naming P4, which is the correct answer. **Still open:** R3 (`assemble` is one-shot — nothing installs the assembly into a running owner), R4 (no sealed-pubkey ↔ verification-key-registry cross-check), R5 (owner-observed only). The battery is 14 door tests + 14 floor tests against the floor's own software key store, all in temp dirs.
- **Deliberately sequenced:** the shadowed-REST-verb table guard starts only after #475 lands (so its table includes `curation_spawn` and is born green).

**Delta 2026-07-31 — `north` returns the CODE, and width-vs-use gets its first real answer:**
- **The second hop is closed inside the verb everybody already calls.** Six weeks of host transcripts
  (458 m1nd calls across 157 sessions) measured the shape of the failure: 57% of sessions call m1nd
  exactly ONCE, 71% never make two m1nd calls in a row, and the two most frequent actions after a m1nd
  verb are Bash (167) and Read (46) — the agent goes and fetches by hand what m1nd just pointed at.
  `north` opened 105 of 157 sessions and answered healthily 86% of the time, yet it *narrated* its
  answer in 76% of calls while the agent acted on a file it newly named in only 32%; the verbs that
  exist to close that gap sat at **1 call (`surgical_context`) and 0 (`batch_view`)**. The fix is not a
  new verb and not a new slicer: `north` COMPOSES those two and returns the source of its top focus
  nodes — the symbol's own lines when the node names one, the file head when it does not — for up to 3
  distinct files under a caller-visible `code_budget_chars` (default 2,000 chars of source), every cut
  declared with honest `files_total` / `files_returned` / `files_omitted` and per-slice `total_lines`
  vs `lines_returned`. **Measured on a graph ingested from this repo's own source (9,281 nodes): 5,576
  chars = 1,394 tokens before → 8,116 chars = 2,029 tokens after.** §C1.3's 2,000-token pin is amended
  in the same PR with that number: the orientation half is unchanged and still under it, the packet as
  a whole now sits at ~25% of §O.12.4's 8,000-token hard ceiling, and `code: false` restores the
  pre-code packet exactly. This is the first move against the owner's harshest grade — **width-vs-use 4
  (138 verbs, 29 ever called)**: the answer to a verb nobody calls is composition into the verb
  everybody calls, not more documentation.
- **Debt this front declares rather than hides.** (a) A focus node whose symbol the Rust extractor
  cannot name — a method inside an `impl`, since `extract_rust_symbols` skips the whole block — falls
  back to the file HEAD, which for a deep symbol is the module preamble, not the code. The slice says
  so (`source: batch_view`, `line_start: 1`, `truncated: true`), so it is honest, but it is the weak
  answer; the real fix is the focus node's OWN recorded line span, which needs `orient` to carry
  `line_start`/`line_end` on `focus_nodes` — another verb's contract, deliberately not changed here.
  (b) `north` now pays `surgical_context`'s neighbour + heuristic pass once per served file. That cost
  is MOVED, not new (it is what the agent's second call would have cost), but it is unmeasured on the
  live 18k-node owner. (c) The measurement behind all of this is a read-only audit of host transcripts,
  not m1nd's own telemetry — m1nd cannot yet see its own usage shape, which is why the number took six
  weeks to surface.
- **The Hebbian layer had never accumulated anything in production (measured 2026-07-31, fixed on the ingest path).** The served owner's `plasticity_state.json` held 73,332 synaptic rows with **zero** carrying a `strengthen_count`, a `weaken_count`, an LTP/LTD flag or a `last_used_query`. Not dead code: `activate` reaches step 8 and writes them. The ingest erased them — `finalize_ingest_with_inventory` installs a graph whose `edge_plasticity` arrays are born zeroed, nothing on that path re-imported the sidecar, and the `state.persist()` at the end of the same function published the zeros. The mechanism to survive already existed and was already documented (label-triple matching, built precisely for a re-ingest that renumbers nodes); it was simply never called there. The ingest now carries the learning across the replacement, preferring the running session over the file and failing open on a bad sidecar. **Residual debt, named not fixed:** two other seams still install a graph without restoring learning — `AutoIngest::replace_graph` (`m1nd-mcp/src/auto_ingest.rs:499`, the document lane's own tick) and the `persist` `load` action (`m1nd-mcp/src/persist_handlers.rs:113`). And the deeper product question, filed as a letter, not decided here: only 2 of ~141 verbs (`activate`, `missing`) reach step 8 at all, so the graph learns from one retrieval path.
- **Machine-side residuals:** G6 provider executable; shadow/canary producer; runtime half of the bundle blind spot; m1nd-ui eslint PAID (ESLint 10 — the break was never eslint itself but the `brace-expansion@5` override from #418 landing under the CJS `minimatch@3` that eslint 9 pulled, and `npm run lint` was not a CI step so nothing saw it — now wired as its own `ui-gates` step, so it can go red again; one residual named in its place: the `eslint-plugin-react-hooks` 7 React Compiler family is held OFF at pre-migration strength with 31 findings open — `set-state-in-effect` ×21, `purity` ×5, `refs` ×5 — whose fixes change render behaviour and belong in their own proven change); the dependabot react 18→19 pair #453/#454 is mutually deadlocked — each PR is the other's missing half, so neither can ever go green alone and they need one combined React 19 PR or closure; serve binary refresh onto this arc's code once the tray lands (then the lifecycle re-proof); `default_registry_root()` cannot see a per-host registry (letter filed, owner's wiring call); PATHOS consolidation pass (the 07-24 list below + the checkpoint-27 Current State narrative both await it).

**Copy / positioning**
- **README + site copy rewrite (branch `copy/human-voice`, 2026-07-29) — LANDED HERE, awaiting owner review.**
  Full README rewrite in the owner's voice (definition at line 5, anti-pitch section, "If I disappear",
  "The human is the second reader", zero em-dashes, zero bold-first bullets, ~9 min read) plus site copy
  surgery: new hero thesis ("Memory that knows when it's wrong"), site version now read from the root
  package.json at build time (was hardcoded 1.3.1), FAQ fixed (no `m1nd warmup`, no phantom cloud beta,
  honest seek pipeline, honest language count), "formal verification" and "no file reads, ever" softened
  to what is true, /use-cases marked illustrative and its fabricated certainties rewritten, and every
  em-dash removed from reader-visible site text. Declared debt this branch does NOT close: the 7 i18n
  READMEs now lag the English rewrite (English declared canonical in the new Translations section);
  the MCP Registry entry still publishes 1.3.0 (owner publish gesture). Closed since first written:
  the wiki token-claim pages now carry the dated snapshot hedge, pt-BR was retranslated by hand from
  the owner-ratified copy, and `.github/workflows/i18n-translate.yml` re-translates the other six
  languages via GitHub Models on every README change to main, opening a review PR (English canonical).

**Blocking the product**
- **LIFECYCLE PROOF — BOTH CYCLES LANDED 2026-07-29 (clean shutdown AND crash).** m1nd is a
  CONTINUITY system, and until this slice the property *boot → serve → mutate → clean shutdown → boot
  again → still serves* had no proof anywhere — which is how three bricking boot defects (#441, #442)
  lived undetected under 1458 green unit tests until the OWNER asked whether m1nd was working. The gate
  now exists: `brain_serves_its_own_state_across_clean_shutdowns_and_second_boots` in
  `m1nd-mcp/tests/persist_runtime_root.rs`, driving the REAL binary over stdio JSON-RPC (the harness's
  own "faithful seam") across FOUR boots on one runtime root, covering BOTH durable-write families a
  plain MCP client can still reach (classified `memorize`, debounce `alerts_ack` — the ack is durable to
  nobody unless the shutdown checkpoint carries it), asserting zero sidecar refusals on captured stderr,
  designed by an askGOD verdict (CHANGE, applied), condition-based from birth, and **proven to bite**:
  temporarily reverting either boot fix turns it red with a message naming the regression. Cost measured:
  ≈ +2.6s. **Cycle B — crash/`kill -9` with no checkpoint — landed the same day**, as
  `brain_survives_a_kill_nine_between_boots` in the same file, reusing the same harness (one new spawn
  method, no second harness): three boots on one runtime root with the clean shutdown replaced by an
  uncatchable SIGKILL of a fully-serving owner — asserted through the exit status, not assumed, and
  delivered to the child pid the harness itself spawned and reaped, never to a name. It pins the two
  durability classes on opposite sides of ONE crash. The CLASSIFIED write survives, including the
  `memorize` committed seconds before the kill with no clean shutdown behind it; the DEBOUNCED
  `alerts_ack` does NOT, and that is the point — measured `acked: false` on the recovered boot, cleanly
  on the OLD state, which is the declared loss window demonstrated rather than described (Cycle A asserts
  the same ack SURVIVES a clean shutdown, so the pair brackets the window from both sides). It is held to
  COHERENCE and not to survival on purpose: pinning the loss would turn the gate red the day someone
  narrows the window. Also asserted: the recovery boot comes up AT ALL (the #442 class), never serves an
  empty graph (the #441 class), sweeps its own unpublished checkpoint temporaries, and prints none of the
  blind-boot signatures. **Proven to bite**: deferring the actor's publish decision to the debounce turns
  it red at `strict recovery refused non-current or lossy plasticity_state checkpoint payload` — and it
  takes Cycle A red with it, which is itself the finding: there is no weakening of publish-on-turn that
  leaves the clean cycle green, so the two cycles really are one property seen from two premises. Cost
  measured: the test itself runs ≈10.1s, but the binary's wall-clock goes 8.94s → 10.21s, so **≈ +1.3s**
  — it runs in parallel with the two tests already there and is simply the new longest pole. Two probes
  that did NOT bite are worth as much as the one that did, both recorded in
  the test: reverting #442's co-change `?` does not reach this path (the crash cycle never produces that
  drift), and dropping `memorize` from `READ_ONLY_DENIED_TOOLS` leaves the gate GREEN because the actor's
  graph-generation witness catches it anyway — the classification is NOT the load-bearing half for a verb
  that ingests, only for one that dirties a sidecar alone. **What remains NOT_RUN, said precisely: a kill
  DURING boot and a kill DURING the checkpoint write itself** — the latter belongs at the store's own
  `CheckpointFaultPoint` seam (`RenameCurrent`, `FsyncCurrentParent`), in-process and deterministic, not
  behind a sleep in an integration test; both were declared out rather than shipped as timing races.
  **G4's "fault injection" claim is now true for the crash class and still open for disk-full and
  corruption**, which are its own separate items. Two finds the build itself surfaced, both
  filed: under the M1ND-10 authority floors **~29 mutating verbs that `tools/list` still advertises are
  unreachable from a plain MCP client** (`generic_action_authority_required` — measured live, converges
  with the bootstrap-instruction front and genesis P2/P3); and the light-ingest merge behind `memorize`
  **silently erases parallel edges**, contradicting the premise of #442's fixture — needs a decision and
  a pinned test. Read next to the traction datum in checkpoint 33 — 138 registry verbs, 48 advertised,
  **29 ever called** — the system's problem is not width.
- **Windows phase-2 — CLOSED 2026-07-27.** 63 test binaries green on Windows, clippy clean (#435 · #436 ·
  #437 · #438 · #440). Never needed a Windows box: a CI-driven red→green loop plus mirror probes
  (flipping `cfg(unix)`↔`cfg(windows)` locally compiles the exact branch Windows compiles) closed it.
  The lesson worth keeping: **"~22 tests" was a count taken behind fail-fast** — `cargo test` stops at the
  first failing binary, so each fix only revealed the next masked layer. The advisory leg now runs
  `--no-fail-fast`, because a job that exists to MEASURE debt and reports one layer at a time measures
  nothing. **The flip is DONE (2026-07-29):** `windows-latest` is back in the required matrix and the
  `rust-gates-windows` advisory scaffold is deleted — made against a fully green advisory run on main
  (run 30426528487, carrying SPEC-1 + the RustCrypto sweep + Cycle B), not a hopeful one. Windows red
  now blocks merge again, six days after it was demoted for holding the queue hostage — this time with
  the debt paid instead of overridden. One latent item still filed, not fixed: `config::workspace_allowed`
  carries the same verbatim-prefix split `d8668591` fixed in source-edit (fail-closed, so the same input
  is refused on Windows and allowed on Unix).
- **Genesis — CODE-COMPLETE with this PR.** P1 (medulla-only read fallback) landed in #403. SPEC-1
  (the freshness door, `graph.ingest.refresh_declared_root` at `ScopedGrantA2`, shrink-floor 60%)
  landed in #463. **P2 + SPEC-2 land HERE**: `brain.bootstrap.birth` at `PositiveSovereign`, admission
  by owner-stamped human origin — THE STAMP IS THE BINARY'S OWN CLI FLAG (`m1nd-mcp --birth <repo>`,
  human-facing form `m1nd init --birth <repo>` relayed by the npm CLI); no MCP or REST payload can
  forge it, and every wire client is refused `human_gesture_required`. Battery-first (18 tests born
  RED, §5.7–§5.8), single-flight per canonical root, empty-destination defined on disk, whole-or-
  nothing birth, migration-vs-birth separation pinned. The prose swept in the same PR: five surfaces
  that taught "no way forward" now teach the OFFER (agents offer the exact ceremony command and stop
  — running it is the human's). What remains of genesis is USE, not code: the owner running the
  ceremony where he wants brains born.

**Honesty defects found and not yet all closed**
- `sha256` field carrying a non-cryptographic 64-bit `DefaultHasher` value (`m1nd-mcp/src/tools.rs`
  `simple_content_hash` → `session.rs` `pub sha256`). The real `sha256_bytes` already exists in
  `m1nd-ingest/src/ownership.rs`. Fix in flight.
- `agent_id` defaulting to `"unknown"` at `m1nd-mcp/src/http_server.rs:1272` while the doctrine says every
  call carries one — needs an owner decision (enforce globally / write-verbs only / rewrite the doctrine),
  not a unilateral patch.
- Port `1337` vs `1338` disagree across surfaces (README attach example vs the owner AGENTS.md names).
  Unify or document which is which.
- `m1nd-viz` was listed as a workspace member in the repo's agent guides — `AGENTS.md` (corrected in
  PR #407, in queue) and the local gitignored build notes (corrected the same day) — but no such crate
  exists anywhere in the tree and `Cargo.toml` never declared it.
- **Durability is now a WINDOW for verbs the witness cannot see (2026-07-25, PR #426).** The brain actor
  stopped answering "did this turn change durable state?" with a SHA-256 of the whole ~100 MB state and
  now answers with `DurableWitnessV1` (`Graph::generation` + session generations, O(1)) — the fix that took
  a warm `seek` from 5.0s to 0.40s. The witness sees graph STRUCTURE and session generations, nothing else,
  so a verb that writes only a durable SIDECAR is invisible to it. Two declared routes now carry those:
  a mutating classification (`READ_ONLY_DENIED_TOOLS` → published on the acked turn — `antibody_create`,
  `ingest`, `learn`, `daemon_start`, `auto_ingest_start`), or a persist choke point that enters the
  staged-persist debounce. **Debounce route = a real `kill -9` loss window of up to
  `auto_persist_interval` (50) deferring turns**: `alerts_ack`, `daemon_stop`, `daemon_tick`,
  `boot_memory`, `antibody_scan`, `calibrate_envelope`, `calibrate_predict`, `document_bindings`,
  `document_drift`, `document_resolve`, `auto_ingest_tick`, `auto_ingest_stop`. An acked `daemon_stop` or
  `auto_ingest_stop` inside that window can therefore RESURRECT as running after a hard kill — note that
  `daemon_start` IS a classified mutation while `daemon_stop` is not, an asymmetry that predates this
  change and now has a durability consequence.
  **Worse for pure learning drift:** plasticity raises no persist request at all, so it does not even
  advance the debounce counter — on a read-only workload with the daemon stopped and auto-ingest idle,
  a `kill -9` loses the entire session's learning. The only backstop is the graceful shutdown checkpoint,
  which is **single-attempt and refusable** (`m1nd-mcp/src/project_brains.rs::shutdown` — terminal
  lifecycle, refuses a second attempt, and can time out with 0 checkpoint ACKs). Not a guarantee.
  Open: decide whether plasticity drift should tick the debounce (bounded loss) or stay free (cheapest
  reads), and whether the debounce-route verbs above deserve promotion to classified mutations. The
  classification itself is mechanically guarded — `session.rs` freezes the sidecar inventory and scans
  the shipped source, so a new sidecar writer that declares no route fails CI loudly. Doctrine in
  `docs/UML-ORGANISM.md` § "Who pays the checkpoint".
- **The strict `read_snapshot` still deep-clones the graph on EVERY transport call.** Brain resolution asks
  the actor whether the bound brain covers the caller root, so `read_snapshot` runs per call, and it ends
  with an UNCONDITIONAL `rebind_after_callback` — a full `encode_graph_json` + `decode_graph_json` of the
  whole graph. Read from the code, NOT measured here (the perf lab was deleted); the PR's own
  `M1ND_BRAIN_TIMING=1` prints `rebind_detached_graph` as its own stage, so the number is one lab run away.
  The deferring `execute` branch already proves the cheap shape: rebind only when a second owner of the
  graph Arc exists (`strong_count > 1` or a live `Weak`), i.e. only when a callback actually kept a handle
  (`execute_read_without_an_escaped_arc_does_not_rebind_the_graph`). Applying the same gate to the strict
  path is deliberately NOT done here — it is the hardened fence and deserves its own verdict and its own
  A/B, not a drive-by. Measured claim, unmeasured fix.

**Process debt**
- The PR queue needs review, not just rebasing — `#401` ("the graph learned to write") is substantial and
  unread.
- Registry↔dispatch parity oracle: a structural test that every advertised verb reaches a live handler,
  proposed to kill the whole class of "advertised but unreachable" defects (of which the bootstrap prose
  was one instance).
- CI tiering and graceful attach, both declared in checkpoint 31 and untouched since.
- v1tals checkup is due (merge count passed its trigger).

## Inherited lessons — custody from the June era (2026-06-23 → 06-27)

Taken into the repo's own memory on 2026-07-24 so they survive independently of any external
note. Each was paid for once; none is derivable from the code or the git log.

- **One writer per runtime root is a design conclusion, not an accident.** Persistence is
  load-all → dump-all, last-writer-wins, with no WAL, CAS, or merge — so concurrent writers on
  one runtime root cannot be made safe by care. The safe shape is one owner + N clients, with
  real isolation via `--runtime-dir`. Anyone proposing multi-writer must first replace the
  persistence model.
- **Never accept a subagent's green.** A registry slice was reported "9 tests ok" while a
  coexistence test failed even in isolation: instance ids collided (no nonce), entries
  overwrote each other, and discovery would have broken in production. The orchestrator found
  it only by running the tests itself.
- **Distinguish a race from a bug with a decisive test, not a verdict.** A push-relay measured
  22/30 looked like a defect; a long-window run proved it was a race with no replay after
  subscribe. Condemning the code would have been wrong.
- **Measure honesty in the units of the decision.** The sufficiency signal computed mass and
  marginal score in `base_score` while the kept/dropped partition ran on `combined`
  (= base × trust/tremor). On a graph with tremor history a strong dropped node looked retained,
  producing a false `sufficient`. Two adversarial lenses caught what synthetic tests could not.
- **Run a real smoke with the real model.** A non-total-order comparator (an intransitive
  score/specificity switch) sat latent since v1.0 and panicked Rust's `sort_by` only once
  default embeddings clustered scores into near-ties on real graphs. Synthetic tests never
  produced the tie density that triggers it.
- **Model size claims: check `content-length`, never infer from parameter count.** The `potion`
  "NM" suffix names PARAMETERS, not megabytes — a mistake made three times. The shipped choice
  (`potion-base-8M`, ~29MB, MIT, pure-Rust, off by default) was made for one embedding space
  serving both code and prose; the recorded successor for when nodes carry real bodies and an
  ANN index exists is `jina-v2-base-code` (Apache-2.0, pure-Rust via candle, no ONNX).
- **Self-updating architectural knowledge is intrinsic, not a chore — and the README holds only
  the contract, never the mechanism.** Grown context that merely accumulates measurably degrades
  an agent; the reflexion model must confront the code rather than mirror it, which is why its
  first useful output is "where your code already violates your own rules".
- **Re-ground `file:line` in the right tree.** A review once declared a shipped verb "fictional"
  because it read a stale checkout. Verify which tree you are standing in before you call
  something absent.

## North Star
m1nd = operational intelligence for coding agents. The bar: genuinely BEAT plain
`rg`/Read in the inner loop, measured honestly — not tie, not "feels useful".
Run a continuous, chained improvement engine: measure (battery) → fix+test the
real defect → checkpoint → seed the next cycle. Never sugarcoat results.

**The arc now:** the verifiable trust substrate (answer + map + trust receipt) is the released,
live FLOOR. On top of it, six PRDs describe one organism — a per-project brain, an antifragile
shared memory, a native and verified handoff soul, a human-legible tree, a two-tier routing
backend, and a delegation layer. The design era spent its final stretch making those six cohere
into ONE constitution with ONE build order. The work ahead is CONSTRUCTION: climb the ladder,
rung by rung, each slice proof-grown and degrading to UNPROVABLE rather than a fake green — the
same bar, applied to building the organism outward.

## Current State (2026-07-20, checkpoint 27 — boundary `LOCAL_PROVEN` after `APPROVE`/NONE; freeze+push+merge owner-authorized; candidate ceremony is the active front)

> **STALE — kept for history until the consolidation pass.** This narrative reflects checkpoint 27
> (2026-07-20). For currency read checkpoints 34–38 at the top of this file; where they disagree with
> this section, the checkpoints win.

### 2026-07-20 — Checkpoint-26 remediation closed and independently approved (BOUNDARY `LOCAL_PROVEN`)

**What closed.** All five required changes of the checkpoint-26 verdict are implemented and
adversarially tested in `scripts/m1nd10_candidate_source_guard.py` (casefold matching; credential/
SSH/key-store denial; `opaque_archive`; default-on public content gate scanning only path/metadata
survivors; fail-closed unreadable handling), the 265-file governed migration executed under the
owner-ratified plan (246 retired, 18 scrubbed/redacted, one digest-bound PRD exception), and the
G6 gitignore contract was amended to the two-layer law (public never ignored; operator-only always
ignored) — a design call the re-review explicitly confirmed. Focused gates and the current
aggregate lanes are green; frozen PRD/UML hashes exact.

**Review binding.** `docs/proofs/m1nd10-candidate-source-boundary-askgod-rereview-20260720.md`:
`APPROVE`, alta, `REQUIRED_CHANGES: NONE`, oracle-owned re-runs of the decisive gates, identical
pre/post status-shape fingerprints. Voice note: Fable seat, because the Fugu route failed on a
revoked codex CLI OAuth token (Sakana API verified alive; fix is an interactive `codex login`).

**Open owner decision (risk 1).** The retired benchmark files remain published in the public
origin/main history; retirement protects future candidates only. Accepting that exposure or
rewriting public history is a separate owner ceremony, deliberately outside this cut.

**Authorized next.** The owner authorized freeze + push + merge
(`docs/proofs/m1nd10-candidate-freeze-authorization-20260720.md`): freeze one immutable candidate
from this reviewed tree, re-run the matrix bound to its digest, then push, PR, and merge with
green 3-OS CI. Publication, installation over the served owner, activation, and G10 remain
separate authorities, untouched.

### 2026-07-19 — M1ND-10 candidate-source review and canonical handoff (CHANGE REQUIRED; CANDIDATE FREEZE BLOCKED)

**What the review proved.** The exact-commit identity, non-mutating worktree projection, Git-entry
metadata checks, pinned Gitleaks workflow, and proof-level separation are sound. The surrounding
policy is not yet fail-closed. Raw case-sensitive comparisons let trivial case variants bypass
private, cache, generated, and secret names. Common environment/package-manager/cloud credentials,
SSH private keys, and additional key-store extensions are not covered. Opaque archive/container
formats can carry private material while neither the guard nor Gitleaks opens them. Finally, the
guard never scans candidate blob text, so the public-no-leak law has no mechanical path-content
enforcement. These are reproduced defects, not speculative risks.

**Review binding.** The valid isolated Fugu retry returned `CHANGE`, high confidence, after the
broader first process was stopped for repeated compaction without a verdict. It ran read-only,
contacted neither the installed owner nor port 1338, opened no private benchmark content, and left
all three source fingerprints unchanged. The public transcript is
`docs/proofs/m1nd10-candidate-source-boundary-askgod-review-20260719.md`; its one machine-local path
is redacted without changing the finding. Frozen PRD/UML hashes remain exact.

**Continuation law.** Before any candidate freeze: casefold relevant comparisons; expand the
credential, SSH, and key-store deny classes; deny opaque containers unless an enforced unpack-and-
scan path exists; scan exact-candidate and worktree-projection public text for personal absolute
paths. A post-review census found 509 occurrences of the current machine-local prefix in 143
candidate-visible, non-private files: 134 historical benchmark files dominate the set, while the
frozen PRD contains three. Scrub or retire noncanonical artifacts through a reviewed migration;
do not silently edit the frozen canon or hide it behind a blanket allowlist. Any PRD amendment or
digest-bound exception requires explicit owner ratification. Add adversarial tests for every
demonstrated bypass and semantic workflow tests for the
content gate; run the focused guard/CI suite, Gitleaks, actionlint, Ruff, diff-check, frozen hashes,
and affected aggregate; then submit the corrected diff to a fresh independent review. Only a green
review may restore `LOCAL_PROVEN` for this boundary. Immutable candidate, hosted enforcement,
formal blind G6, real G7 LIVE, G8 release, G9 activation, and G10 remain open.

### 2026-07-19 — M1ND-10 G6 corrective re-review and authority-root pinning (LOCAL_PROVEN; FORMAL RUN NOT RUN)

**What changed.** The scorer-side bypass identified by the first Fugu `CHANGE` verdict is closed.
`scripts/benchmark/m1nd10_g6_retrieval.py` now owns a self-contained derivation of formal proof;
it does not import or call the runner validator. It binds every owner readiness row to the exact
candidate binary, requires the sealed corpus repository set across topology/cleanup/ingest, checks
source revision and file-set identity, owner/session/lifecycle cleanup, each governed-ingest
authority proof row, pre/post source equality, blind-boundary coherence, and canonical disjoint
path topology. `score_eligible`, `diagnostic_only`, `proof_state`, `formal_preflights.complete`, and
per-stage summaries cannot authorize anything; any divergence from the derived proof blocks scoring.
On Unix, `authority_runtime` also pins the authority root directory descriptor and device/inode
identity for the owner's lifetime and refuses symlink, rename/recreate, and in-process second-owner
replacement. This is a deterministic replacement defense, not an OS same-UID isolation claim.

**Proof and boundary.** G6 runner/scorer is 85/85 (60 + 25), the Rust authorization verifier is
8/8, and `m1nd-control` is 149/149. The final read-only Fugu re-review used an isolated profile with
zero MCP servers, touched neither the installed owner nor port 1338, and returned `APPROVE`, high
confidence, `REQUIRED_CHANGES: NONE`; the verbatim record is
`docs/proofs/m1nd10-g6-corrective-askgod-final-20260719.md`. The focused authority group is 29/29,
the full workspace aggregate records `m1nd-mcp` 1399 PASS with 15 ignores, repository Python is
174 PASS, and the current release build passed in 3m38s. This approves the corrective source only.
The 220-task formal blind run, operator labels, immutable candidate, live owner/browser/h4nd,
release, publication, installation, activation, and G10 receipt were not exercised. G6 remains
`SOURCE_IMPLEMENTED + LOCAL_PROVEN + COMPONENT_PASS`, not cumulative PASS.

### 2026-07-19 — M1ND-10 G7 version and offline dependency closure (LOCAL_PROVEN; LIVE NOT RUN)

**What changed.** The bundle projection now uses `CARGO_PKG_VERSION`, matching source and binary at
the organism layer. The private UI package remains `0.1.0` and stays independently bound by the UI
bundle provenance together with the package-lock digest. No manifest field or drift rule was
weakened: an injected private-package version still produces visible source/binary/bundle drift.
The local npm cache was prepared outside the formal gate in disposable workspaces with lifecycle
scripts and browser downloads disabled. A second clean workspace then installed the same current
lock using only `npm ci --offline --ignore-scripts`: 285 dependencies, including `zwitch@2.0.4`
and `zustand@5.0.11`, produced installed-lock SHA-256
`b90246cb0dc6bd2ff7d6e6fd1c045f36f4e91a5dc155542bcc2cab9a72dbeef1` from source lock SHA-256
`cd84302b4f5f39106cb8cf2d05c16032f55c07a8fb867f77a2fddce47e07a2ca`.

**Proof and boundary.** The current local slice passes 49 Python G7/bundle/release/CI tests, 18
additional release-contract/crate-upload tests, 15 Rust attestation/manifest tests, 8 UI live
contract tests, 646 UI unit tests, TypeScript, both UI linters, `cargo check -p m1nd-mcp`, scoped
all-target clippy with `-D warnings`, Cargo fmt, scoped diff/no-leak, and frozen PRD/UML hashes.
The source package and lock bytes were unchanged by cache preparation and all temporary workspaces
were removed. The exact isolated browser, candidate binary, owner, and h4nd path was not run; the
dirty working tree is not an immutable candidate. G7 is therefore
`SOURCE_IMPLEMENTED + LOCAL_PROVEN + COMPONENT_PASS`, not cumulative PASS.

### 2026-07-19 — M1ND-10 G6 corrective closure (LOCAL_PROVEN; REVIEW APPROVED; FORMAL RUN/LIVE NOT PROVEN)

**What changed.** The eight findings recorded at checkpoint 20 are implemented in source. Production
receipt proof now crosses the exact candidate binary's exclusive offline Ed25519 verifier and a
separately pinned assembly/key registry; no ambient Python crypto can mint the production label.
Owners launch on port `0` and are accepted only after the fresh private registry binds the spawned
PID/start/root/endpoint, the bearer is captured once through a bounded no-follow private-file
contract, and authenticated `/api/instance/self` plus `/api/manifest` bind the same owner and binary.
The runner recomputes the typed Rust digest projections instead of trusting declared ownership,
lineage, resolution, pipeline, or outcome values. The external authority provider executes from a
fresh private cwd behind macOS `sandbox-exec` or Linux `bwrap` deny-default filesystem isolation;
formal mode refuses when that proof is unavailable. The source snapshot is proved again immediately
after governed ingest. Provider/verifier timeouts are finite and capped. The results-v2 validator
closes top-level, measurement, trust/sufficiency, run-metadata, provenance, and score-proof coherence.

**Proof and boundary.** `python3 -m unittest` over the corrective runner and scorer is 85/85; the
offline Rust verifier is 8/8; `m1nd-control` is 149/149; workspace `cargo check`, scoped all-target
clippy with `-D warnings`, `cargo fmt --check`, Ruff check/format, `git diff --check`, scoped public
no-leak, and an isolated-HOME invalid-request smoke all pass. The frozen PRD/UML hashes remain exact.
The final independent re-review returned `APPROVE` with no required changes, closing the corrective
review only. No production assembly/signature ceremony, 220-task blind score, immutable candidate,
installed-owner contact, release, activation, or G10 receipt was run. G6 is therefore
`SOURCE_IMPLEMENTED + LOCAL_PROVEN + COMPONENT_PASS`, not cumulative PASS.

### 2026-07-19 — M1ND-10 canonical implementation handoff (WORKING TREE; NOT RELEASED)

**Canonical continuation.** The complete current map is
`docs/M1ND-10-HANDOFF-20260719.md`: authority boundary, read order, architecture, subsystem/file
map, G0-G10 status, exact local evidence, independent-review findings, the G7 version defect,
safe rechecks, prohibitions, and the only admissible continuation order. Its current-state claims
supersede older M1ND-10 paragraphs below when they differ; the older paragraphs remain as the
historical evidence trail.

**What now exists in source.** M1ND has a 169-action canonical control catalog; canonical identity
and causal envelopes; Ed25519/P-256 authority contracts; a protected owner configuration seam;
challenge/authenticate, one-shot leases, signed AuthorityWAL, typed MissionService and elevated
consumers; per-brain actors, runtime jobs, OCC, persistence fencing, checkpoint ACK and recovery;
universal ingest ownership; an evidence/receipt correlation spine; calibration receipts; a blind
G6 runner/scorer; an isolated, supply-chain-attested G7 live-browser orchestrator; canonical
release/update/rollback machinery; and A0-A5 constitutional admission, RED, grants, epochs, quorum,
sentinel and activation contracts. Generic elevated dispatch stays closed. The served owner was
not upgraded and is not candidate proof.

**What current local evidence says.** `m1nd-control` is 149/149 with all-target clippy green;
`m1nd-core` unit is 182 green; `m1nd-ingest` records 299 pass, 6 ignored, plus one integration;
current G4 focus is 16 graph-ingest + 15 checkpoint + 4 Windows source-contract tests; the isolated
Windows harness is green and the final review returned `APPROVE`/medium; G6 targeted runner/scorer
tests are 85/85 plus 8/8 exact-binary verifier tests; G7 orchestrator/live-contract tests are 18+8
with TypeScript green; G8 focused release tests are 25/25. The current full workspace records
`m1nd-mcp` 1399 PASS/15 ignored, every executed external integration green, repository Python
174 PASS, UI 646 + 8 PASS, strict static gates green, and a release build PASS. These are dirty-tree
local receipts and must be repeated after one immutable candidate is frozen.

**What blocks an admissible candidate.** The earlier G6 `CHANGE` findings are corrective-source
implemented, locally green, and the final corrective re-review is `APPROVE`/none. The formal
immutable-candidate blind run is still `NOT_RUN`; G6 therefore remains `COMPONENT_PASS`.
G7's organism-version and offline-cache defects are locally closed at checkpoint 22, but its real
isolated browser/owner/h4nd execution remains `NOT_RUN` and cannot be promoted from this dirty tree.

**Nonclaims.** Native Windows/NTFS and physical power loss are `NOT_RUN`; a complete cross-target
build is blocked by missing cross compiler/sysroot headers. The real blind benchmark, real isolated
browser/h4nd path, hosted OIDC/Sigstore, registry publication, installed update/rollback, production
hardware custody, quorum/sentinel/actuators, shadow/canary, and activation are not proven. There is
no immutable candidate digest, no common G0-G10 receipt set, no `AutonomyActivationReceiptV1`, and
no final authority ratification. Active mode remains `HUMAN_GATED`; `FULL_AUTONOMY`, G10, and a
10/10 claim remain forbidden.

### 2026-07-18 — M1ND-10 G8 verified release updater (WORKING TREE; NOT PUBLISHED)

**What is implemented.** A GitHub-release install no longer trusts a raw HTTPS asset. The npm
updater requires `cosign`, downloads `CANDIDATE.json`, its Sigstore bundle, and the selected raw
runtime from the same immutable tag, verifies the exact
`maxkle1nz/m1nd/.github/workflows/release.yml@refs/tags/v<version>` identity and GitHub OIDC
issuer, then binds schema, candidate id, version, tag, target, asset name, SHA-256, and byte size
before any backup, journal, or managed-runtime mutation. The old raw-asset test bypass is gone.
Local fixtures may replace download transport and the cosign executable only through explicit
source-checkout test seams; production mode rejects both ambient overrides. Trusted update tools
resolve only through fixed, canonical, regular, executable, non-world-writable locations.
Redirects remain HTTPS-only on the exact GitHub source family, and every downloaded release
object is byte-capped before parsing or mutation.

**Rollback and promotion are fail-closed.** The local journal is a closed
`prepared → installed → rolled_back` phase machine. Rollback validates the current target digest
before restoration, recovers a crash between install and the `installed` journal write, is
idempotent after completion, and refuses unknown phases, target drift, tampered backups, and
legacy journals it cannot classify. The ambient Cargo-registry fallback has been removed:
production refuses `runtime-release-unavailable` rather than executing `cargo install`.
Automatic npm-package mutation is also locked until a candidate-bound multi-surface transaction
and rollback rehearsal exist; legacy Cargo-like journals remain refused. Release CI signs the
candidate first, then runs the real public apply/rollback command on all four hosted targets with
the signed bundle; those post-sign receipts are CI-only promotion gates, deliberately outside the
already-signed candidate and published release file set.

**Honesty boundary.** npm, Python candidate, syntax, workflow-static, and no-effects fixtures are
locally runnable. GitHub OIDC issuance, Sigstore services, the four hosted updater jobs, live
release download, tag promotion, registries, and host rebind remain `NOT_RUN`/`NOT_PROVEN` until a
real immutable tag workflow succeeds. `workflow_dispatch` from a branch is deliberately refused
by the tag guard because its signing identity cannot satisfy the client tag identity.
Final askGOD review of the real G8 diff returned `APPROVE` with high confidence and no required
changes; that source verdict does not promote any hosted or publication claim above.

### 2026-07-18 — M1ND-10 G2→G3 authority bridge (WORKING TREE; NOT PUBLISHED)

**What is implemented.** The new `m1nd-control` authority contracts and the served-owner
integration now form one fail-closed G2→G3 seam: strict session challenge/authenticate
ceremony, exact action-policy authorization, one-shot durable authorization leases, typed
`MissionService` ingress, and a signed AuthorityWAL whose durable `COMMIT` is the sovereign
mutation commit point. REST exposes `/api/authority/session/challenge`,
`/api/authority/session/authenticate`, `/api/authority/authorize`, and
`/api/tools/mission_service`; Streamable-HTTP MCP exposes the equivalent four tools. The
selected brain, owner time, key registry, and ingress-context digest come from the owner/transport
— never from request-body authority claims. REST/MCP session ids are correlation labels, not
authentication; subject identity comes only from the signed capability and owner-pinned key.
`LandIntent` is an
authorized canonical READ that returns the digest later bound into `Land`.

**The production assembly is explicit, not ambient.** `OwnerSecurityConfigV1` carries public
trust anchors and relative durable roots only; it cannot contain private keys. Its canonical
digest and monotonic epoch are pinned by a separate protected root, and config/root symlinks,
rollback, tamper, non-extending updates, overlapping roots, or software-test assurance at the
production loader are refused. `assemble_production_owner_authority_v1` requires injected
hardware-protected config/runtime epoch providers, a hardware-protected broker/WAL journal head,
and an injected production AuthorityWAL signer/verifier, then installs authorization issuance and
MissionService consumption together. Required HTTP boot without that complete assembly returns
`NOT_INSTALLED` before bind.
No environment-derived or fabricated signing key is selected.

**What focused fixtures prove on this macOS checkout.** Real Ed25519 fixture capabilities prove
challenge → authenticated authority session → Ordinary `LandIntent` authorization → canonical
read → Positive `mission.service.land` authorization → MissionService → exact AuthorityWAL
`COMMIT` → lease `CONSUMED`. Negative batteries refuse wrong-wire and expired challenges,
role drift, replay after restart, stale leases, revoked/rotated/wrong-subject keys,
freeze/RED/epoch drift, symlinked or overlapping security roots, config rollback/tamper,
valid-prefix broker/WAL rollback, corrupt/torn journals, and crash boundaries. The complete sealed
outer AuthorityTransaction, authorization receipt, ExecutionResult, ReviewResult, and WAL record
are independently domain-signed with explicit non-circular subsets and fixture-proven tamper
refusal. The broker state machine is
`UNUSED → RESERVED → FINALIZATION_PREPARED → CONSUMED|ABORTED`; consumption revalidates the
current freeze/RED/mode/policy/epoch/expiry state at the finalization boundary. GC requires both
the configured retention floor and an external proof that no durable artifact references the
terminal lease.

**Honesty boundary / current deployment truth.** Concrete hardware-protected config, runtime-epoch,
broker/WAL-head, attestation, platform-signer, and production WAL-crypto adapters are
`NOT_INSTALLED`; no real-machine owner key or live authority session exists. The assembly/install
seam is wired fail-closed, but no live CLI/LaunchAgent platform adapter can supply the required
hardware assembly. The running owner has not been upgraded by this working tree and therefore
remains `HUMAN_GATED`/fail-closed; h4nd must remain visibly locked and must never invent keys,
session ids, receipts, or authority. Sessions are process-memory state and require
re-authentication after restart; the durable replay ledger still rejects a consumed capability
nonce. Hardware/production assurance flags remain provider declarations until concrete adapters
and attestation verification land. macOS focused gates are PROVEN. An earlier wide run recorded
`1036 PASS / 3 FAIL / 1 ignored` and preserved all three cross-lane failures in the proof; after the
audit/G4 repairs, the superseding full m1nd-mcp library run is `1049 PASS / 0 FAIL / 1 ignored`, and
strict `--all-targets -D warnings` clippy is green. The earlier G2-only bounded Fable/Fugu review
was unavailable and remains recorded as such; a later narrow review of the integrated G2/G9
admission path returned `APPROVE` with high confidence and no required changes. Linux and Windows
3OS execution, publication, live hardware ceremony, and real operator acceptance remain open;
this slice is not a claim of FULL_AUTONOMY or production readiness.

### 2026-07-18 — M1ND-10 integrated G2/G9 constitutional admission (WORKING TREE; NOT ACTIVATED)

**What is implemented.** Autonomous positive authority has one explicit served-owner path. The
generic positive branch rejects every non-human authority variant. A sovereign request must carry
`PositiveSovereign`, an autonomy capability, the exact G2 decision/capability/session/policy
bindings, an exact G9 projection and evidence set, and the same organism/repository/brain/action/
payload/mission/head/epoch/grant tuple at every boundary. G9 admission occurs before G2 positive
authorization; a final G9 witness is checked after authorization and before a receipt or lease can
escape. Witness unavailability or state/root drift freezes positive issuance and safety globally.
The production assembly accepts and retains one `ProtectedProduction` autonomy owner; software
test assurance is refused. Bootstrap remains `HUMAN_GATED`, issuance-frozen, safety-frozen, and
does not claim multi-artifact physical atomicity.

**What is proven.** Focused Rust fixtures cover bootstrap, exact protected-owner synchronization,
generic bypass refusal, missing/foreign evidence refusal, pre/post admission drift, full mirror
binding, frozen liveness on integrity loss, and one-shot transport issuance. The final bounded
askGOD/Fugu review returned `APPROVE` / high confidence / `NONE` required changes and independently
confirmed the no-bypass and no-overclaim invariants. The durable receipt is
`docs/proofs/m1nd10-g9-g2-constitutional-admission-20260718.md`.

**Honesty boundary.** The G9 consume/project step is serialized under one G9 store lock, but G2
and G9 are not one physically atomic storage transaction. Production hardware custody, live
Touch ID, protected monotonic roots, real external verifier independence, hosted release,
activation, and recovery from physical power loss remain `NOT_PROVEN`/`NOT_RUN`. This is secure
fail-closed integration, not `FULL_AUTONOMY` activation.

### 2026-07-18 — M1ND-10 elevated typed-consumer boundary (PREFLIGHT APPROVED; NOT IMPLEMENTED)

**Current truth.** The generic MCP/REST dispatcher deliberately admits only exact `Ordinary`
actions. `ScopedGrantA2`, `PositiveSovereign`, `ServiceIdentity`, and `SafetyOnly` routes are
refused before legacy handlers can mutate state. The typed `MissionService` is the one installed
elevated consumer. This is a valid no-confused-deputy boundary, but it means ratify, promote,
source-edit, general service, and safety workflows outside that service are securely unavailable;
therefore G10 and `FULL_AUTONOMY` cannot be claimed.

**Approved next seam.** A read-only askGOD/Fugu preflight returned `APPROVE`, high confidence, and
no required changes for a closed owner-derived mutation envelope, broker-resolved signed receipt,
one-shot reserve/finalize, typed transactional adapters, and an exhaustive action/ingress/consumer
matrix. Generic elevated dispatch remains closed. The first slice is ratify, promotion, and one A2
edit; handler reuse may occur only behind the transactional boundary with no early visible effect.
CLI/hooks/jobs/recovery/migrations require equivalent consumers or explicit `policy_disabled`.
A2 filesystem writes also require target digest, proof mark, OCC, rollback/conservation, crash, and
same-UID tamper evidence. The durable preflight is
`docs/proofs/m1nd10-typed-consumer-preflight-20260718.md`; it approves architecture, not a gate,
activation, or release.

### 2026-07-18 — M1ND-10 G4/R6 runtime isolation and durability (WORKING TREE; NOT PUBLISHED)

**What is implemented.** Every bound or hosted brain now has a bounded serial actor. A
`BrainSessionCell` checks the complete `SessionState` out of a short-held storage mutex, releases
that mutex before dispatch, persistence, checkpoint, or long analysis, and restores ownership by
RAII. REST, stdio, Streamable-HTTP MCP, bootstrap, tier recall, promotion, shutdown, and eviction
use the same actor seam. Health reads an independent cached snapshot. Mutating success crosses
persist plus content-addressed checkpoint ACK; persistence failure keeps reads live, fences new
mutations as `degraded_persistence`, and only a real retry ACK clears the fence. Dirty eviction
requires the exact ACK. Runtime jobs expose bounded admission, deadlines, cancellation state,
`running_after_timeout`, proposal preparation, and actor-side OCC commit.

**What is mechanically proven on this macOS arm64 checkout.** The superseding full m1nd-mcp
library run is `1049 PASS / 0 FAIL / 1 ignored`; strict serve/all-target clippy is green. The
R6 battery measured 601 health samples during a real `31.277137958 s` owner stall: p99
`0.838625 ms`, max `3.003625 ms`, and zero samples at or above 100 ms. The versioned 10,000-op
workload across eight brains observed zero lost writes and zero cross-brain reads. Checkpoint
fault injection, disk-full, corruption, concurrent GC, fallback, degraded-read/write-fence, stale
proposal, overload, shutdown, and eviction-ACK fixtures pass. The conservative source audit scans
73 production files and 23 remaining SessionState lock scopes with zero forbidden long/blocking
operations; it remains lexical evidence, not a Rust model checker.

**Honesty boundary.** This is `MACOS COMPONENT PASS`, not full G4. Windows and Linux were
`NOT_RUN`, physical power loss was `NOT_RUN`, and frozen G4 explicitly requires Windows; the gate
therefore remains `NOT_PROVEN_WINDOWS_NOT_RUN`. The tree is dirty/uncommitted and publication was
not attempted. Bounded independent review was exhausted without a verdict: Fable returned
`Credit balance is too low`; full Fugu and its single narrow retry each crossed the 10-minute bound
without the required verdict contract. askGOD is recorded `UNAVAILABLE`; partial progress output
was discarded and no approval or rejection was inferred. Canonical evidence lives in
`docs/proofs/m1nd10-g4-r6-runtime-isolation-20260718.md` and the three
`docs/benchmarks/m1nd10-g4-r6-*` artifacts.

### HANDOFF — the next agent starts HERE (2026-07-15, the whole-system panorama)

**The two-house organism.** `m1nd` (this repo, `github.com/maxkle1nz/m1nd`, anchor checkout
`<repo-root>` on `main`) is the MIND: graph, brains, board, UI. `cockpit`
(`<h4nd-root>`, LOCAL — no remote, its own `docs/PATHOS.md`) is the HAND: cockpit
`:3000`, warm pool + `poold` daemon, the Touch ID tray (`~/Applications/h4nd.app`), and now the
CI Reviewer (`h4nd-pool/reviewer/`). Law of the pair: agents propose and prove; NOTHING lands
without the human's origin-gated gesture.

**The live owner.** Binary `~/.m1nd/bin/m1nd-mcp` served at `http://127.0.0.1:1338` (LaunchAgent
`com.local.m1nd-serve`; runnerd `:1339` = `com.local.m1nd-runnerd`). The embedded UI is the
product: `#/universe` (the panorama, 11 worlds), the Landing, Build Map (live, deep-linkable
`#/world/<basename>/map?block=…`), Hall, Tree, tray. Runtime root
`~/.m1nd/runtimes/claude/` (graph_snapshot.json + `project-brains/<hash>/project_brain.json`
manifests). REST law: every call routes with `?brain=<abs root>`; the mission board reads
`GET /api/mailbox?kind=mission`. Deploy runbook: UI dist is committed by UI PRs → `touch
m1nd-mcp/src/http_server.rs && cargo build --release -p m1nd-mcp` (always
`CARGO_TARGET_DIR=$HOME/.m1nd-build-cache/target`) → `cp` to `~/.m1nd/bin/` → `xattr -cr` →
ad-hoc `codesign -f` → `launchctl kickstart -k` both agents.

**Operating law (non-negotiable).** Worktree-per-mutating-agent off the anchor; RED-first for
every fix; askGOD verdict before BIG; bursts = 1 PR with the doc-gate inside; commits authored
Max Kle1nz in English (never an AI identity); tests NEVER touch the live owner (ephemeral
owners/fixtures only — the letters/cartas are the sanctioned exception); m1nd misbehavior →
append `~/.m1nd/field-reports.jsonl`, never mid-mission surgery. Durable side records live in
`<historical-scratchpad-root>` (verdicts, `PRD-M1ND-CI-SOL.md`, `FIELD-TRIAGE-2026-07-15.md`);
Reviewer runs in `~/.god/runs/`, its ledger in `~/.god/reviewer/`, its isolated pen-free
CODEX_HOME in `~/.god/reviewer-codex/`.

**The Claude Code dev setup (this host — how the next agent's environment is wired for m1nd).**
- **MCP (user-scope, `~/.claude.json`):** `m1nd` → `~/.m1nd/bin/m1nd-mcp --attach http://127.0.0.1:1338`
  (attach-mode, no lock — the host bridges onto the served owner). Alongside: `context7` (version-
  pinned lib docs before coding), `semgrep`, `playwright`. m1nd tools drop mid-session = the owner
  was kickstarted (rebuild/deploy) — new bridges self-recover; a persistent "Failed to connect" =
  owner down → kickstart. Proceed without it, never insist, report to the box.
- **Hooks (`~/.claude/settings.json` → `~/.claude/hooks/`), all fail-open:**
  `SessionStart → m1nd-north.sh` (attaches :1338, calls `north`, injects the VOICE CARD + orientation
  as ambient context — v3 shim opens with `human_view.lines`); `PostToolUse → m1nd-ambient-ingest.sh`
  (auto_ingest_tick so the graph eats edits unasked); `PreCompact`+`SessionEnd → m1nd-trail-save.sh`
  (persist). Universal guards on the same rail: `PreToolUse → git-guard.sh` (BLOCKS any commit carrying
  a Claude/AI identity — authorship is Max Kle1nz, mechanically); `PostToolUse → verify-edit.sh`
  (gitleaks secret-scan + ruff on every edit).
- **Skills — the m1nd doctrine (user-scope mirrors of the repo's `skills/`, repo wins on divergence):**
  `m1nd-first` (m1nd is the FIRST investigative layer before grep/glob/manual reads) and `m1nd-operator`
  (the full operator surface: routing/reception, L1GHT ingest, risky-edit prep, delegate/debrief,
  medulla tiers, trails, daemon alerts). The mother-workflow skills used ALL era for m1nd dev:
  **`askgod`** (the oracle — every BIG change this era passed a `verdict`; Fable seat via express order;
  it caught the freshness-theater trap, the bell-silence hole, the bounce illegality — read the
  verdicts in the f0a scratchpad), **`gogod`** (the executing hand when implementation itself is the
  bottleneck), **`bugboo`** (adversarial bug hunts with proof), **`pathos`** (this continuity system),
  **`uiproof`** (deterministic frontend Q&A — Playwright specs, not an agent in the loop). Model
  doctrine: Opus main orchestrates/verifies/voices and DELEGATES substantive work to Opus subagents
  in isolated worktrees; Sonnet only for light synthesis; Fable is main-only / askgod-gogod / express
  order — never inherited by a subagent (pass `model:` explicitly on every delegation).

**What waits, and for whom.** The OWNER's hand: archive the two stale merge_wait letters
(`msn_17a1d1f9b013` + the p0 smoke) → the armed `first_real.py --ingest` mints the fresh
boundary-v3 letter → his Touch ID lands the first full mirror cycle; sample the Reviewer's runs
(gate S1→S2: ≥3 runs, ≥30 verdicts, zero reverted approves — run 1 at
`~/.god/runs/20260715T004123Z-review/`); curate/ratify survival-game's 10-block candidate skeleton.
THE NEXT AGENT's queue, in order: S2 of the CI (bounce rail + one-pen wrapper — the PRD v2 in
the scratchpad is binding, the F25 §5e amendment rides it); alerts two-store unification (L130,
battery spec in the triage report); Universe v1.1 (unlit worlds, the m1nd world in its own
panorama via a served-brain manifest, refetch-error keeps last-good); the UML Atlas re-ground
(master at `docs/UML-ORGANISM.md` is grounded pre-era) + Universe/CI sheets; the chain-write
authorization slice (the bell CAN still be silenced by a stranger's seq+1 — verdict-gated
contract work); then Pista C — the calibration study (14 honesty entries are the dataset), the
**npm/crates 1.5.0 release** (this whole era is invisible to installers until published), the
HN launch (dossier ready). Read the dated era-log blocks below for every mechanism's file:line.

> **2026-07-15 — "THE LIVING MAP: graph_changed covers the block verbs, the Build Map subscribes" on `feat/living-map` (PR 2 of the Pista-A pair; builds on the router #374).**
> The Build Map was the last read surface without a live wire — it reloaded on its OWN writes (reconcile/ratify/scan)
> but stayed a PHOTOGRAPH when an agent, the CLI or another viewer mutated the store. Authored from the askGOD CHANGE
> verdict (required changes are law). **Commit 1 (Rust) — the classifier honestly covers the map.** The browser
> (`browser_graph_changed_event`) and MCP (`graph_changed_notification`) relays share ONE predicate,
> `mcp_http::graph_mutation_event_name`, gated by `GRAPH_MUTATION_TOOLS` — a set that listed only
> ingest/apply/edit_commit/memorize/learn/daemon_start/auto_ingest_start, so NO verb that draws the map emitted a change
> (live for ingest, frozen for ratify/reconcile/scan/paint). Extended it with the nine SystemBlock/skeleton/X-RAY writes
> (`system_blocks_seed_import`/`_ratify`/`_reconcile`/`_archive`/`_delete`, `skeleton_candidate`, `receipt_import`,
> `xray_paint`, `xray_retag`) — each CONFIRMED present in `server::READ_ONLY_DENIED_TOOLS`. The drifted "mirrors
> READ_ONLY_DENIED_TOOLS" comment is corrected to the truth: a **curated subset** (mailbox writes like `mission_post`,
> advisory leases, the activation overlay stay OUT — they change nothing a viewer draws; invariant
> `GRAPH_MUTATION_TOOLS ⊆ READ_ONLY_DENIED_TOOLS`). Tests: the nine verbs now relay + name themselves; a mailbox write
> stays read-only-denied yet never masquerades as a graph change. **Commit 2 (UI) — the map breathes.** `BuildMapView`
> subscribes the house live pattern (`useLiveRefresh`: graph_changed, debounced ~500 ms, **scoped to the brain in view**
> §4A.9.6) and feeds the idle `refreshKey` of `useBuildMap`. Composed with the stale-while-revalidate of #372, the re-read
> opens as `refreshing` — the map never unmounts, so the human's selection/scroll/open block panel survive a live update.
> ONLY this surface subscribes (never the App-level front-door read — the oracle flagged the double refetch), so a write
> triggers exactly one refetch. **Proof:** Rust `cargo test`/`fmt`/`clippy` green; UI 635 unit + **29 e2e** green (new
> `living-map.spec.ts` on a deterministic FakeES — a viewed-brain graph_changed refetches once with the panel kept open
> and the loading screen never shown; a foreign-brain event refetches nothing); tsc/eslint/violet-lint/icon-lint clean;
> embedded `m1nd-ui/dist` rebuilt. Doc-gate in-PR: the SSE-events table + classifier note (`docs/wiki/.../mcp-server.md`),
> `SseGraphChangedData` (types.ts), the map's F0-TECH §8 ("live, not a photograph"), and this block.

> **2026-07-15 — "HASH ROUTER: deep links and a real back" burst on `feat/url-router` (PR 1 of the Pista-A pair; the live-map arc is a SEPARATE PR).**
> The SPA got the URL router F30 §1 explicitly deferred — a **hash router** (zero server change: the SPA is rust-embed
> from one `index.html`, so a `#` fragment never reaches the server) that is a THIN URL⇄state sync over the EXISTING
> Surface machine. Authored from the askGOD CHANGE verdict (its five required_changes are law): a new pure lib
> `m1nd-ui/src/lib/router.ts` (parse/serialize/resolve, DOM-free) + one `navigate()` in App.tsx. **R4 — one writer:**
> a single `navigate()` wraps `setSurface`+`setViewedBrain`+`setMapTargetBlock` (+ the transient hall-alerts flag) and is
> the ONLY code that writes history (sprinkled `pushState` banned); all ~10 call-sites + `popstate` route through it —
> landing writes `replace` (a baseline so the FIRST Back works), user nav `push`, popstate no-write. **The scheme:**
> `#/universe` · `#/hall` · `#/tree` · `#/map` (bound; `?block=` rides the map) · `#/world/<key>/tree` ·
> `#/world/<key>/map?block=sb_x` (hosted). **Deep-link BEATS landing** — the hash seeds the surface before the
> `surface == null` landing gate (a `deepLinkPending` guard stands the landing effect down while a world key resolves);
> **no hash → byte-identical landing** (just a `replaceState` baseline added); `landAndOrient` is SUPPRESSED under a hash
> (a deep link skips the 3-beat orientation — no bootstrap↔router race). **R3 — the brain key is a BASENAME, never the
> absolute root (no-leak law):** the world `key`/`root` both ARE the canonical abspath (they leak), and `instance_id` is
> unstable across restarts (`generate_instance_id` hashes pid+clock+seq — ephemeral by construction, instance_registry.rs);
> the basename is stable + non-leaking. Serialize = basename of the viewed root; resolve = match against the worlds
> panorama (`name` is the server basename) then the Hall registry; an unresolvable/ambiguous key (evicted, or a basename
> collision) falls back to the landing rule and popstate-to-evicted falls to the universe — NEVER a stranded empty map.
> **The addressable boundary (half the design):** only durable LOCATION is in the URL (surface + viewed brain +
> tray-seeded `?block=`); transients stay OUT — the ingest modal, the Cmd+K palette, the 3-beat orientation,
> `hallOpenAlerts` (how you ENTERED the Hall), and the Build Map's own ad-hoc card selection (a Back clears `?block=`
> while the live panel, closeable via its ✕, is transient). **R5 declared:** a Back that swaps the viewed brain shows the
> map's honest 'loading' BY DESIGN (`nextReadStatus` holds no last-good across a brain change, useBuildMap.ts) — declared
> in the F30 doc + an e2e comment so it never reads as a regression. **Proof:** 18 router unit tests + 4 Playwright flows
> (`e2e/url-router.spec.ts`: deep-link map+block beats landing, real Back world↔universe, tray `?block=` Back, evicted-key
> fallback) + the 23 existing e2e all green (27/27); `node --test` 635, tsc + vite build + eslint/violet/icon clean; the
> embedded `m1nd-ui/dist` rebuilt + committed; no personal path in any URL/spec (neutral `/work/repo-alpha` fixtures).
> Docs same PR (F30 "§ The hash router" amendment + this block; corrected the stale "NOT yet landed" markers on the landed
> Universe arc #371/#372/#373). Next: land this PR, then PR 2 of Pista A (the live map arc) — separate.

> **2026-07-15 — "GUARDS AT THE WRITE DOORS" burst on `fix/write-door-guards` (branch, NOT yet landed).**
> Field triage 2026-07-15 ranked three write-door guards; two were real and open, one was a stale triage claim.
> **Commit A (mission_post boundary):** the mission-letter contract checked a `receipt_candidate`'s evidence for
> COMPLETENESS but never that its `scope.boundary_version` still matched the LIVE block — so a candidate proving a
> boundary the block had moved past was appended silently (how the orphan `msn_17a1d1f9b013` was born) and only
> caught later at import. `handle_mission_post` now compares the candidate's boundary against the live block
> (reusing the store it already loads for the `unknown_block` guard) and refuses `stale_scope` at post, naming both
> versions — dead evidence declared at the door. **Commit B (persist shrink):** defense in depth behind #370 —
> `snapshot::save_graph` peeks the existing on-disk node count and, when the incoming graph holds under 20% of a
> non-trivial prior snapshot, renames it to `<path>.bak-<unix_ts>` before overwriting (fail-open — a legitimate
> shrink is never blocked, the large snapshot is never lost in silence; the incident was a 10573→704 overwrite).
> Both RED→GREEN. **The triage's P0 was already CLOSED:** "skeleton write-verbs refuse under `caller_root_mismatch`"
> landed 2026-07-11 in **#340** (`skeleton_write_needs_root_gate`, `server.rs:4773`, covering all eight verbs) — the
> triage checked `GRAPH_MUTATION_TOOLS`/`READ_ONLY_DENIED_TOOLS` and missed the dedicated gate; no new code, the
> existing tests already cover it (a verify-before-declare catch). **A fourth guard (a chain has one writer, from an
> askGOD verdict) was OPT-OUT:** the bell-silencing hole is real (a stranger can move a `merge_wait` head with a
> seq+1 non-archived letter; the landing bell counts only `merge_wait` heads), but a letter chain is multi-writer BY
> DESIGN (F25-TECH §4a/§5b: the oracle posts seq 1, **runnerd** streams seq 2+ on the same chain under a different
> identity), and the board's transitions past `archived` are explicitly "left undesigned" — closing it correctly
> needs the chain-write AUTHORIZATION model designed (a contract slice with its own verdict), not a mechanical
> `agent_id` gate that would break the dispatch→execute loop.

> **2026-07-15 — "VITALS NEVER BLOCK THE PANORAMA" fix on `fix/panorama-never-waits` (#373, LANDED — origin/main HEAD).**
> A field report (2×, live) caught `/api/universe` stalling for 15-20s after a kickstart — it read as a deadlock
> (CPU 0%), but was transient contention: `universe_body` took `state.session.lock()` on its FIRST line (just for
> `alerts_pending` + the presence registry root), and the gardener/daemon tick holds that SAME lock for minutes
> across a re-ingest + `rebuild_engines`. A sidecar-only READ surface was queuing behind graph work — against the
> F30 spirit; every boot the Home Universe was unreachable and the SPA fell back to the old doctrine. Fix (minimal,
> in the house style — fail-open + honest omission): `universe_body` now **`try_lock`s** the session — lock free →
> the real vitals (byte-identical); lock busy → the panorama serves WITHOUT them, an honest omission
> (`owner: { alerts_pending: null, note: "owner busy — vitals omitted" }`, `totals.pending` never inflated, the
> presence roster degrading to the immutable boot registry hint `AppState.registry_dir`). The disk-sourced worlds
> need no lock, so the spine is always served. RED-first proof (`universe_endpoint.rs`
> `universe_never_queues_behind_a_held_session_lock`): a held lock made the read take 2.001s pre-fix (ceiling
> 500ms), well under after. UI degrades honest (`buildLandingItems`: a null vital → no owner chip; +1 spec).
> Green: `m1nd-mcp --features serve` suite (40 bins) + `m1nd-ui` `node --test` (617) + fmt + clippy `-D warnings` +
> tsc + eslint, all clean. Docs same PR (F30 §3a + this block). Related ARC (registered, NOT in this PR): the same
> `session.lock()`-for-`registry_root` pattern is in `handle_presences` (owner-wide), `handle_health`,
> `handle_instance_self`, `handle_instances` — each a read surface that would also queue behind the tick; a
> follow-up should give the registry root a lock-free owner-level source and `try_lock` the live vitals there too.
> Next: land the burst PR.

> **2026-07-14 — "HONEST DOORS AND EXITS" burst on `fix/honest-doors-and-exits` (#372, LANDED).**
> A UI-only burst (zero engine change) closing eight honesty gaps a hands-on sweep + an askGOD full cut found
> across the Universe/Hall/map/tray surfaces: **(1)** the map's **Reconcile** asks first — a two-step confirm
> mirroring the tray's import/archive, the read-only seal set apart from the one write button ("the map reads;
> this button writes"); **(2)** the Landing's **owner alert item finally lands somewhere** — the Hall gained an
> owner-alerts panel (bound-session `alerts_list`/`alerts_ack`, deliberately NO `?brain=` — the same stock
> `/api/universe`'s `owner.alerts_pending` counts), per-alert ack + "acknowledge all"; **(3)** a **stagnant**
> `judging`/`executing` head (>24h unmoved) is presentation-dismissable behind an honest confirm (a labelled
> PALLIATIVE — the letter stays on the box, a contract transition is future work; the dismiss lifts if the head
> wakes); **(4)** `useUniverse` stops silencing errors — a 404 still degrades to the empty sky, a real blip after
> a good read keeps last-good + a "read failed — retrying" note, and a FIRST non-404 failure is an honest
> `error` that does NOT decide the landing; **(5)** a rootless Landing world item renders **disabled** ("world
> root unknown — refresh"), never the wrong room; **(6)** `useBuildMap` is **stale-while-revalidate** — a reload
> keeps the map mounted (a discreet "refreshing"), preserving selection/scroll/modal (a write no longer erases
> the human's context); **(7)** the SSE base follows the API base (`API_BASE`, no hardcoded loopback that
> cross-origins a retargeted dev owner); **(8)** the **block detail panel finally closes** — a header ✕, a scoped
> capture-phase ESC (fires only while the panel is open, no modal up, stops propagation so it never also ascends
> a surface), and a re-click toggle, all deselecting WITHOUT losing scroll (coherent with #6). Proven: the full
> `node --test` suite green (567 → 616, +49 specs), tsc + vite build + eslint/violet/icon clean, and a **5-flow
> Playwright browser proof** (`e2e/honest-doors.spec.ts`, owner mocked in-page) — which caught a real robustness
> bug live (the owner-alerts read crashed the Hall on a partial owner body; guarded + regression-tested). Docs
> updated same PR (F30 · F25-TECH · this block). Next: land the burst PR.

> **2026-07-14 — F30 "THE UNIVERSE" built on `feat/universe-home` (#371, LANDED).**
> The SPA gained its L0 HOME — a per-world **Universe** panorama that becomes the entry door when the
> owner serves ≥1 project brain (amends §4A.1; zero brains keeps the first-run Threshold EXACTLY as
> today, Build-Map front door included). Authored from the askGOD CHANGE amendment
> (`docs/HUMAN-VIEW-V2-F30-UNIVERSE.md`, six binding changes applied), it INVENTS no state and no write
> verb: one read-only aggregate — `GET /api/universe` (`m1nd-universe-v0`) — reads the project-brain
> manifests (`disk_roster`), the P1 presence dir (grouped per world), each world's mission-letter box +
> SystemBlock store, and the OWNER's own `daemon_alerts` (owner-scope). **Sidecar-only, proven by an
> executable RED-first HARD LAW** (`m1nd-mcp/tests/universe_endpoint.rs`): serving the endpoint never
> inserts into `ProjectBrainRegistry.brains` — the warm map is byte-identical before/after, and a
> dormant/evicted fixture brain is served purely from its manifest (never hydrated). Navigation is
> **state-zoom** (a new `Surface` variant, no URL router this slice — that is its own future slice). The
> canvas is calm-tech (paper/observatory, zero neon): worlds are textured circles sized ∝ node_count (log),
> lit ∝ `updated_ms` freshness WITH the age shown honestly, satellites = live presences, an amber dashed
> ring = pending gestures; a click opens that world's existing map/tray room. The L0 header is a
> client-composed serif sentence of UNIVERSE FACTS (worlds · awake · await-your-hand) — **never a
> cross-brain pulse** (the pulse stays PER-BRAIN by ratified law). **The Landing** is the unified gesture
> queue: reads aggregated (merge_wait stamps, candidate ratifies, owner alerts), every WRITE still through
> the existing per-type verb (origin gates untouched), the badge labelled "await your hand" — never a bell.
> Honest omissions declared in the F30 doc: `pending.archives` (no distinct cheap queue — archival is an
> alternative gesture on a merge_wait receipt) and per-brain daemon alerts (would need hydration). Out of
> scope: the Atrium (v2), unlit worlds + ingest-click (v1.1), URL router, batch ratify (h4nd G).
> Next: land the burst PR, then the live browser dogfood by the orchestrator (the real-flow proof).

> **2026-07-13 — F2.5e "ARCHIVE A SUPERSEDED RECEIPT" built on `feat/archive-superseded-receipt` (branch, NOT yet landed).**
> The mission-LETTER board gained its 8th, TERMINAL phase — `archived` — the human's set-aside of a
> stale `merge_wait` receipt (askGOD verdict `docs/voice/ASKGOD-VERDICT-ARCHIVE.md`, APPROVE, 6 binding
> changes, all applied). Archiving posts a seq+1 `archived` letter that EXTENDS the merge_wait head, so
> the design reuses everything: **the landing bell drops itself** (it counts `merge_wait` heads only, so
> a moved head simply stops ringing — no bell logic changed), **history is free** (the superseded receipt
> stays on the prior letter with its `boundary_version` forever), **the OCC is free** (the head CAS makes
> an archive×import two-tab race a `stale_head`, nothing appended). Three laws bind it, all proven RED→GREEN:
> **(a)** `validate()` refuses `receipt.imported==true` on an `archived` letter — never a `landed` in
> disguise; **(b)** the board's FIRST transition rule — `archived` may only supersede a `merge_wait` head
> (`invalid_transition` for a fresh/landed/failed/in-progress head), checked in `post_mission_letter` where
> the head is already held for the CAS; **(c)** posting `archived` is a HUMAN-ONLY gesture — `handle_mission_post`
> requires `archived_via:"human-ui"` (the INPUT, never the letter schema), the closed allow-list `receipt_import`
> carries, refused `human_gesture_required` with nothing appended (the product's first SILENT-burial verb, so an
> agent can never bury its own unproven work — the forged-origin test proves the box stays byte-intact). The tray
> gained a discreet **"Archive — superseded by newer boundary"** beside "Import this receipt" on a merge_wait+candidate
> card; its confirm fetches a FRESH snapshot at click and shows the live two-boundary comparison ("proved at boundary
> v1 — the block is at v3"), saying "still importable — archive anyway?" aloud when the boundary has not moved (no
> frozen field — derived at read); `landErrorToast` now recognizes `stale_head` (reload + "state moved"). **Known
> behavior registered (downgrade skew):** an old owner binary that cannot parse `archived` drops the line and the
> bell RE-RINGS — no corruption, honest in the amendment. Gates green: `cargo fmt --check`, `clippy --all-targets
> -D warnings`, the full `m1nd-mcp` suite (1123 passed, 0 failed, incl. 6 new archive tests), the UI suite + build.
> Docs in the SAME burst: `HUMAN-VIEW-V2-F25-TECH` §1h + § archived, AGENTS.md (the third human write), and
> `docs/voice/ARCHIVE-DIVERGENCES.md` (auto-archive/bulk/un-archive + the pre-existing `failed`/`executing` holes,
> all deliberately OUT of v1). **Not landed** — awaiting the burst PR; the 3 real superseded receipts on the
> production owner stay for the owner's own hand (archiving is human) + the Rust-embedded dist restart (orchestrator's step).

> **2026-07-13 — ORGANISM-INSIDE P1 "PRESENCES" (server lane): the control room can see the team.**
> The P1-SERVER lane of the presences arc (askGOD verdict 2026-07-13 APPROVE, binding changes 1–3
> BINDING — `docs/voice/ASKGOD-VERDICT-P1.md`). What landed on `feat/p1-presences-server`: a NEW
> module `presence.rs` — durable session-presence sidecars (`m1nd-presence-v0`) molded on
> `instance_registry` (files in the shared registry dir, minutes-scale TTL, `is_stale` filtered at
> read so a dead presence DISAPPEARS rather than lying, boot-GC reclaims orphan sidecars after an
> owner restart). The BEAT is a THROTTLED, fail-open hook inside `track_agent` (session.rs:2235 —
> the single choke point all four dispatch seams funnel through; ~1 disk write per 5s per session,
> forced promptly only when a signal changes). Enrichment is measured or declared, never invented:
> `brain`/`caller_root` from the session's own binding, `task_ref` MEASURED from the agent's own
> open `mission_start` charter (`mission_handlers::latest_open_mission_for`), and the DECLARED
> fields (`kind`/`theme`/`intent`/`worktree`/`working_set`) ride NEW optional `session_handshake`
> fields. The OBSERVED mutation level is stamped in `dispatch_tool` off the single `read_only_denied`
> classifier. **Collision is DERIVED at read, never materialized**, with the verdict's exact
> predicate: SAME brain AND (same caller_root/worktree OR working-set overlap) AND BOTH mutating —
> same-brain alone NEVER warns (three executors in isolated worktrees is the normal burst shape, and
> the anti-test pins it). **Surfaces:** the cockpit gains its **8th collection** `presences` (ONE
> root line — the collision warning rides the LABEL, no new schema field — with a capped in-place
> drill scoped and labeled "this brain"); `north` gains ONE collision honest-gap on the EXISTING
> `honest_gaps` mechanism, present only on a real collision, derived per-agent so it lands on BOTH
> colliding sessions' packets; and the Hall's CONTRACT endpoint **`GET /api/presences?brain=`**
> (`m1nd-ui docs/voice/P1-UI-CONTRACT.md`) serves `{presences, collisions, served_brain?}` — absent
> `brain` = owner-wide (the Hall's scope), present = that brain + the §4A.9.4 echo, unknown = honest
> 404; `collisions` always present (server-authoritative); `/api/health` keeps an owner-wide
> diagnostic block beside `agent_sessions`. **Budgets RE-PINNED and now MECHANICALLY enforced** by the new battery
> `cockpit_budget_holds_with_the_eighth_slot` (`chars/4`, worst-case loud fixture): cockpit root
> ~574 tokens, presences-drill ~567 (the new largest drill), both ≤800; north unchanged (the gap
> line is present only on collision). **The written LIMITATION** (the verdict's flagged inverse-TTL
> lie): *presence == activity VISIBLE TO m1nd* — an executor compiling for 20 min makes no calls and
> expires from the roster; the roster answers "who is talking to m1nd, on what, since when", never
> "who is alive". Proven: `cargo test -p m1nd-mcp` lib **870/0** (9 presence + the cockpit budget/8th-slot
> tests among them), `fmt --check` clean, `clippy --all-targets -D warnings` clean. Presence is
> WITNESS TISSUE — it gates nothing, ratifies nothing, lands nothing (laws 5, 10 hold). Lanes kept
> apart: no m1nd-ui/ touch, no G1 tick region, no rootfix territory. Next (other lanes): the Hall
> strip (P1-UI, gate-material) + the live two-session collision gate on the served owner.

> **2026-07-12 — THE ORGANISM FROM INSIDE + 360 arc RATIFIED (§C11-style amendment); P0 "WEAR THE WIRE" in flight.**
> A new off-§C10 front, opened by the owner's order and ratified the same day (verbatim:
> *"MUITO BEM! ratificado bora pra frente"*) — registered here as the ladder ritual demands,
> never a silent fork. The spec is versioned beside its sisters: `docs/ORGANISM-INSIDE-PRD.md`
> (the PRD, Fable seat) + `docs/uml/organism-inside.md` (the UML — a design-stage lens like
> `massif.md`, deliberately OUT of the code-grounded atlas until the wires land; its 6/6 mermaid
> blocks re-validated with real `mermaid.parse()`). **The mother-confession it answers:** on
> 2026-07-12 the guardian ran the largest single-day burst in the repo's history — 8 PRs merged
> (#347–#354), six executors, two oracle seats — and the m1nd mission board registered NONE of it
> (measured live: 21 letters, zero from that day). The organism's agents are invisible to the
> organism itself; the pieces exist as verbs — what is missing is the WIRES. **Four connections
> make the organism see, heal, learn, and span itself:** the immune loop (a field report becomes
> a mission charter automatically → pool executes → gate proves → the HUMAN stamps), the process
> memory (every closed mission debriefs into the medulla's untouched C8.3/C8.4 gauntlet), the
> presences (sessions become visible in the Hall/cockpit/tray, and collisions surface BEFORE they
> cost two hours — they have, twice), and Federate 360 (the owner's brains cross doctrine with
> provenance always visible, under the pull-only medulla law, never across a client boundary).
> **The laws that do not bend** (PRD §4): the human stamp is the only landing — no auto-ratify
> exists, ever, under any flag (#353's human-origin gate is the arc's model); nothing this arc
> touches composes a `landed` letter; the Budget Law holds on every packet; a refusal teaches,
> never silently skips. **The phases, proof-gated, each closing with live proof on the served
> owner:**
> - **P0 — WEAR THE WIRE (this block's PR; doctrine + docs, near-zero engine code).** The
>   guardian's own workflow starts speaking the rails that already exist: an orchestrator that
>   dispatches executors in a burst opens ONE mission card (`mission_start`), the burst posts
>   progress (`mission_event`), and closes it honestly (`mission_close`) — the board becomes the
>   day's truth. Doctrine landed on all agent surfaces (`M1ND_INSTRUCTIONS` §4 + the three skills,
>   same PR — the agent-docs gate arms on server.rs + skills together). DOGFOODED on its own card
>   `msn_1783893555531_claudeguardianp0we`. Gate: a real burst (≥3 executors) fully visible on the
>   board as it happens (baseline: 0 letters on the 8-PR day).
> - **P1 — PRESENCES (medium).** The presence sidecar + collision derivation + Hall/cockpit/tray
>   renders. Gate: two real mutating sessions visible with themes + ages; an arranged same-block
>   collision surfaces on both norths BEFORE either lands; TTL expiry proven (a killed session
>   disappears, never lingers as a ghost).
> - **P2 — THE IMMUNE REFLEX (medium-large).** The charter composer + eligibility + caps + dedup on
>   the existing sweep; the judge armed as an advisory triager once h4nd's smart-bells land. Gate:
>   one real spool report (bug|honesty) becomes a charter → spawned → gate green → `merge_wait` →
>   the owner stamps → `landed`; a `cap_reached`, a `no_gate_derivable` and a `duplicate_report`
>   each proven a logged non-event; zero auto-landings by construction AND by grep.
> - **P3 — THE PROCESS MEMORY (medium).** The mission-close debrief → distillate → `kind:process`
>   write-through → the packet feedback row. Gate: a process-fed packet must not lose an A/B on the
>   same task class, and ≥1 lesson (e.g. the #331 instruction-in-packet class) demonstrably rides a
>   packet instead of a human memory. Promotion stays manual through the untouched gauntlet.
> - **P4 — FEDERATE 360 (medium).** The provenance render + portfolio view + the isolation
>   allow-list (`federation_policy`, default-permissive, test-proven) + the reuse meter. Gate: a
>   claim promoted from brain A surfaces in a session bound to brain B with its full origin line
>   rendered; ≥1 reuse event counted; the leak-permutation battery extended with the `isolated` case.
> **Next:** the orchestrator lands the P0 PR (this burst), then P1 begins. Nothing here grants an
> agent a new write power; the carimbo stays the owner's — one second per decision.

> **2026-07-12 — GARDENER v1 built on `feat/gardener-v1` (branch, NOT yet landed): the organism moves when seen.**
> An off-§C10 front under its askGOD verdict (CHANGE — the law, versioned at
> `docs/voice/ASKGOD-VERDICT-GARDENER.md`; design + measured cost in `docs/voice/GARDENER-V1.md`,
> residue in `GARDENER-DIVERGENCES.md`). The seven changes, built exactly as judged: **fail-open
> first** (a background vigil can never fail an agent's tool call — the violable `?` at the inline
> auto-ingest tick is now log-and-continue, RED-proven); **the code leg is the per-brain DAEMON**
> (opt-in per brain in its own store dir, default OFF; watch set = the brain's ingest roots; resume
> rides the registry warm-boot/resolve — the persisted mid-tick `tick_in_flight:true` that WEDGED
> every resume is sanitized on load, RED-proven; survives restart AND LRU eviction, both pinned);
> **auto_ingest stays the documents lane** (plus the cheap guard: a manifest-bound workspace root
> can no longer be demoted — the #326 class); **honesty by traffic** (v1 freshness = "when seen";
> a resumed `watch_backend:"native_fs"` downgrades to `polling` — no surface may claim a notify
> consumer that does not exist; the "continuous monitoring" wording was purged from the tool
> schema, help guidance and wiki); **burst coalescing** (window 75 ms→500 ms + 5 s cap, registered;
> the tick detects ONCE per burst into a persisted FIFO backlog drained `max_files`/tick — the old
> truncate-then-advance hole LOST every file beyond budget on the git backend, RED-proven with a
> 20-file burst); **auto-reconcile with cedência** (45 s quiet window pushed by every activity
> tick — one window per burst; voluntary yield to a live candidate_lease; fresh OCC key, 1 retry,
> then an `auto_reconcile_conflict` alert on the existing lane; candidate skeletons skip — their
> freshness is another cycle); **intocáveis intactos** (no north fields, human ratify untouched,
> alerts ride the existing 500-cap lane). **Cost measured before defaults** (bench, release):
> ~36 ms/file at N≤100, 59.6 ms/file at N=1000 (detection 8.7 s, full drain 59.6 s over 31 ticks)
> — the number that JUSTIFIES default OFF. Upgrade-safe: pre-gardener `daemon_state.json`
> deserializes armed (serde defaults, pinned). v2 registered: zero-traffic alerts (per-brain tick
> task on HTTP, lock contention measured first) and the detection walk's per-tick re-hash.
> **Next:** orchestrator lands the PR, then arms the daemon on the hot brains (m1nd + game) via
> explicit opt-in on the served owner.

> **2026-07-12 — the human-layer burst SEALED (checkpoint 18): the VOICE arc + the SCAN-LOADING arc landed as four PRs, then a curation closed the burst.**
> Four PRs landed the same day and the organism wears all of it. **The VOICE arc** (its two
> slices detailed in the blocks below): **#348** `human_view` v1 — the north packet's ≤4-line
> card in the SPINE grammar; **#349** the PULSE + the `map <N> blocks` fact + the navigable
> `cockpit` verb. **The SCAN-LOADING arc** (its own sheet `docs/uml/scan-loading.md`, no separate
> era block here): **#347** dressed the held `skeleton_candidate` scan wait — a synchronous POST
> the owner legitimately holds up to ~2 min with a live naming runner — as an honest client state
> machine (idle→submitting→clustering→slow; REAL events only — response/error/gesture/1 s tick; NO
> fabricated %; a TOTAL reducer that cannot wedge; an HONEST ABORT that stops the browser's fetch,
> never the owner's work, so the store may still land); **#350** (its slice 2) had the owner
> narrate the real phases (`file_list→clustering→naming→persisting`) on the EXISTING `/api/events`
> SSE, so the panel shows the server's actual phase and degrades byte-clean to the static label
> when the channel is silent. **The owner stamped the PULSE:** the five-cell `╷`/`│` row
> (`trust · graph · focus · bell · coherence`, fixed-forever order, read as an EXPRESSION not
> cell-by-cell, dropped WHOLE under `caller_root_mismatch`) is now the OFFICIAL signature of the
> voice — the mark was the one thing the arc waited on. **Verified at the close:**
> `cargo test -p m1nd-mcp` **1,069 / 0** (844 lib + 36 integration binaries; re-run on this
> docs-only curation branch to prove nothing broke, exit 0), the burst's **489 UI + 5 e2e** green
> at landing; the budgets hold — `north` ~1,404 tokens (≤2k, with the pulse + map mounted) and the
> `cockpit`'s own ~695 root / ~430 drill (≤800). **Honest divergences, all recorded**
> (`docs/voice/V1-DIVERGENCES.md` + `SLICE2-DIVERGENCES.md`): the ratified-maps segment was OMITTED
> from line 1 in slice 1 (the packet carried no ratified-map count) and DELIVERED in slice 2 as
> `map <N> blocks`, measured from the SAME `system_blocks_snapshot` read — no invented number; the
> scan-SSE design imagined a `naming_wave i/N` counter, but `run_scan_naming` makes ONE opaque
> daemon call for all packets (the `div_ceil(4)` "waves" only size the timeout budget), so per-wave
> splitting would change the verb's SEMANTICS — REFUSED, and the `naming` phase emits one boundary
> event carrying the wave ESTIMATE, never fabricated sub-progress (naming waves declined on the
> same principle the whole voice runs on — narrate a real number, never a manufactured fraction).
> **The curation itself (this checkpoint's own act):** the slice-1 `DIVERGENCES.md` was moved off
> the repo root to `docs/voice/V1-DIVERGENCES.md` (beside the arc, content untouched); the UML
> atlas indexed the two new code-grounded leaves (`scan-loading`, `cockpit`) and was re-grounded at
> `c1ba801` with every Mermaid block re-validated by real `mermaid.parse()` — 78 across the master
> + the twenty-two sheets, 0 failures. That re-validation caught THREE blocks silently broken and
> un-renderable on GitHub — the master flowchart's dotted-link label (a `.` inside a `-.text.->`
> label) and two sequence messages carrying a `;` (mermaid's statement separator) — each fixed in
> its own commit, never silently; `docs/uml/massif.md` stays a design-stage lens under `docs/uml/`,
> deliberately NOT in the code-grounded index. **Next, REAL and queued:** the cockpit's per-item
> drill (`impact`/`why`/`trace` against a selected item, depth 2) — explicitly DEFERRED by the
> verdict (amendment 4), not forgotten; exposing the menu inside the rich widget as a first-class
> product surface; the marketing pitch, HELD by the owner's order until he stamps it; and a
> visual-refinement pass on the widget template WITH the owner. Nothing here is done the owner has
> not seen.

> **2026-07-12 — the HUMAN-LAYER VOICE arc opens (§C11-style amendment): slice 1, `human_view`, lands.**
> An off-§C10 front, opened by the owner's order and registered here as the ladder ritual demands
> (never a silent front). The arc: m1nd gains a VOICE the human sees in the conversation. Slice 1
> (this landing): the north packet carries `human_view` (`m1nd-human-view-v0`) — a server-composed,
> already-mounted ≤4-line card in the SPINE grammar (the `m1nd` wordmark hung on the margin, `│`
> gutter fixed at column 6), five states (clean/bell/coherence/mismatch/needs_ingest), a mechanical
> `state_sig` anti-repetition key. Its law is the askGOD verdict "human view" (CHANGE, 10
> amendments — versioned with the design docs under `docs/voice/`): composed AFTER reception (under
> `caller_root_mismatch` the card IS the warning, zero statistics — they would describe the wrong
> brain); one sentence per fact (signal lines reuse the `honest_gaps` strings VERBATIM; a line that
> cannot fit falls whole, never truncated); brand law G1 written as the field's law (only measured
> facts already in the packet); fail-open (north never errors over its own voice). The render side
> is doctrine on ALL agent surfaces in the same burst: `M1ND_INSTRUCTIONS` §7 (the NEGATIVE-default
> cadence verbatim, the translation duty, the agent-owned deep rung R2, the 1:1 ASCII fallback, the
> counterfactual attribution law) + the three skills (same doctrine + the treacherous lexicon + the
> ten verb families). Proven: 11 unit + 5 north integration tests (the mismatch and needs_ingest
> shapes mandatory per the verdict), suite 829 lib tests green, budget re-pinned ~1,391 tokens
> (≤2k; the field costs ~174 chars ≈ 43 tokens on a clean beat). HONEST residue: the ratified-maps
> segment is OMITTED from line 1 (the packet carries no ratified-map count today — recorded in the
> slice's DIVERGENCES.md; exposing it is a slice-2 decision); the PULSE mark (`╷╷╷│╷`) and the
> pitch AWAIT the owner's explicit stamp (the mark is pluggable through `compose_voice_signature`
> alone); slice 2 (the navigable cockpit verb — its verdict is already emitted, also under
> `docs/voice/`) queues AFTER this v1 lands and the owner stamps the provador.

> **2026-07-12 — the HUMAN-LAYER VOICE arc, slice 2: the PULSE is stamped, the map fact lands, and the navigable `cockpit` verb ships (§C11-style amendment).**
> The owner stamped the mark: the **PULSE** (`M1ND-VOICE-ALIEN.md` §5 variant C) is now the
> OFFICIAL signature of the voice. Line 1 hangs `m1nd ` + a FIVE-cell pulse row (calm `╷` / raised
> `│`) in a FIXED-FOREVER order — `trust · graph · focus · bell · coherence` (the anti-equalizer
> law, pinned by test); read as an EXPRESSION (all low = calm, one stem up = look), never
> cell-by-cell; DROPPED whole under `caller_root_mismatch` (the plain spine returns — the vitals
> would read the wrong brain); the cells join the `state_sig` (`…|pulse:╷╷╷│╷`). **Slice-1's honest
> residue is resolved:** line 1 gains a `map <N> blocks` segment (the served brain's ratified
> SystemBlock count, PER-BRAIN, omitted when zero) measured from the SAME `system_blocks_snapshot`
> read that feeds coherence — no new read, no invented number — and the packet carries a small `map`
> field (`{ratified_blocks, coherence}`, present iff a store exists, mirroring `landing_bell`). **The
> navigable cockpit shipped:** `cockpit` (`m1nd-cockpit-v0`) — a DEDICATED read-only verb, a SIBLING
> of north (breaks alone, never a north field), the human's ON-REQUEST router over m1nd's read
> surfaces. Its law is the askGOD verdict "the navigable cockpit" (the 10 amendments,
> `docs/voice/ASKGOD-VERDICT-COCKPIT.md`): seven stable-slot collections (the tray + missions are
> POINTERS — no verb; the map/health/trust/memories/drift are argument-less reads); the read-only
> law is DERIVED (every routed verb filtered against `READ_ONLY_DENIED_TOOLS`, pinned by
> `cockpit_read_verbs ∩ deny = ∅`); `menu_sig` on every response (the short reference a widget button
> carries back, never free text/never a write); a drill re-asserts `store_version`/`state_sig` and
> says "state moved" when the caller's snapshot diverged; the `why` text is lifted from the ONE help
> catalog (never a parallel one). The official **widget template** is versioned
> (`docs/voice/WIDGET-TEMPLATE.md` — the h4nd skin, STATE-JSON + render, no-write law, links-by-origin,
> modal-in-normal-flow, payload = number + menu_sig). Render doctrine is doctrine on ALL surfaces in
> the same burst: `M1ND_INSTRUCTIONS` §7 (the pulse, the deep-rung legend + proof glyphs `⊢`/`∎`, the
> cockpit's on-request-only default, the extended ASCII map `╷`→`.`) + the three skills. Proven: 20
> new unit tests (14 human_view incl. the pulse anti-equalizer + map segment + mismatch-drop; 6
> cockpit incl. the read-only derivation + stable slots + menu_sig + state-moved), full
> `m1nd-mcp` suite green, **two budgets re-pinned live** — north ~1,404 tokens (≤2k, fresh ingest
> 9,178 nodes) with the pulse + map mounted; the cockpit's OWN budget ~695 tokens root (≤800), ~430
> drill. HONEST residue (slice-2 DIVERGENCES): the cockpit is a ROUTER in v1 (presents the read to
> run, like the help overview; its output carries the receipts — never fabricated), not an inline
> executor; per-item drill (impact/why/trace, depth 2) is deferred by the verdict.

> **2026-07-11 — ARC-1 + ARC-2 closed: the proof system hardened, F12 ratified and implemented, and the FIRST AUTONOMOUS CURATION ratified by the owner.**
> The integrity burst (#342) closed all four open field reports: the field spool is runtime-scoped
> (an ephemeral owner can never read or write the production box again), receipts refuse temporal
> incoherence at import (fabricated timestamps die with a lesson — both guards live-fire proven in
> production), the bare-name roster miss was proven already-cured and pinned with an end-to-end
> regression, and the shadow project-brain was FUSED (15 unique construction-era memories migrated
> alive into the bound brain; full backup preserved). An askGOD verdict (CHANGE, high confidence)
> reshaped Arc 2 before a line was written — its gravest finding: "no agent ratifies, ever" was
> paper (`system_blocks_ratify` sat behind the `?brain=`-bypassable gate with a free-string
> ratifier); the mechanical guard (`ratified_via:"human-ui"`) and verb-level o5 shipped as
> prerequisites in the same burst. **F12 — the curation lane — was authored, owner-ratified
> (#341) and implemented (#343) the same day:** the runnerd serves `/curate` in the image of
> `/name`; the pinned hand-runner PROPOSES candidate_edit ops as data; the owner validates,
> sanitizes (o5) and applies them itself, seat runner, under OCC, then posts the summary letter.
> The agent never holds a write surface. **The arc gate was met exactly:** a fresh 35-block
> candidate (the live-collaboration repo) was curated by a real hand-runner mission in ~80s — four
> architectural merges, all 31 surviving blocks renamed in domain language, deliberate non-merges
> JUSTIFIED in the report — and the owner ratified touching only the review screen and the ratify
> button (the THIRD human ratify; the first over an autonomously curated map). The same hour gave
> the counter-proof: a second mission's proposal referenced a hallucinated block id and the
> preflight refused the WHOLE batch atomically, on screen, nothing persisted — propose-apply
> contains a hallucinating hand mechanically. Ambient hooks run host-side on Claude (SessionStart
> north / PostToolUse ingest-tick / PreCompact+SessionEnd persist) and Codex (notify wrapper),
> each proven by planting a symbol and watching the graph eat it unasked. Next: the take (the
> hand stands by; the owner's bell), R12-as-product (the Stop distiller), R11/R17 (the last
> ladder rungs), the revision-promotion ceremony, and the total m1nd–h4nd symbiosis.


> **2026-07-10 — F11 shipped whole; the first HUMAN ratify in the product's history; the seam-fix series.**
> The F11 amendment (candidate editing with minimum human friction) landed as ONE burst PR (#330: the
> `candidate_edit` engine + `candidate_lease` + the naming-runner wire + the drawn Edit-Names-&-Boundaries
> screen), followed the same day by the live-fire fixes only real usage reveals: #331 (the owner batch
> budget was sized to a 20s runner while a real CLI runner measures ~50s/call; AND the naming packet
> carried no task instruction — a generic LLM runner wandered to timeout until the instruction traveled
> INSIDE the packet), #332 (ingest overlap guardrail: parent/child/worktree twin brains refused honestly
> with an `allow_overlap` escape, born from a live twin-brain incident), #333 (the REST tool route
> bypassed that guardrail — route parity through one shared `run_bootstrap_core`), and the curation-letter
> fix (the UI composed `brain_ref` from the seed-form skeleton id only; the F25 §1f contract is
> display-name = basename; plus the §1g block guard now recognizes the store's current skeleton id as the
> whole-skeleton mission anchor). **The milestone:** the owner ratified the production monorepo's skeleton
> through the real screen — 43 blocks, store v1→v46, 42 names stamped by hand + 1 by the live runner
> (`named_by:"runner"` in production). **Honest telemetry from that first human run:** the batch
> "Name with runner" was never invoked (0 daemon calls during the session) — the zero-touch lane exists
> and is proven by machine, not yet exercised by a human; investigate reachability/latency of the batch
> button. **New standing doctrine (owner, 2026-07-10):** a UI function is only REAL when driven in a real
> browser by the agent itself — screenshot as proof; suites green ≠ use proven (four same-family holes
> caught in one day only by live smoke). **Queue:** field-report triage (4 open: runtime-scoped spool,
> receipt execution-identity, memorize bind-drop, inbox bare-name), keyvault revision-promotion ceremony
> (ratified pre-o6 with provisional names), hand-runner capability (curation spawn), bound-map display-name
> fallback (declared residual).

> **2026-07-09 — F2.5 pre-work: the write-mode runner connected and proven (external hand-runner exercise).**
> The full receipt cycle ran live against an ephemeral owner (`--serve --port 1399 --runtime-dir mktemp`,
> production `:1338` untouched): seed import (12 blocks, store v1) → the block's REAL gate
> (`cargo test -p m1nd-core`, 25 suites / 257 passed / 0 failed) → `receipt_import` under OCC → store v2
> with the receipt attached (`emitter {kind: runnerd}`, full-log sha256, honest excerpt). Both guards
> exercised on purpose: stale `expected_store_version` → `Conflict` (nothing applied); wrong boundary in
> scope → `stale_scope`. **Transport recon for the mission tray:** there is no letter-post verb today —
> letters are born by appending to the spool; `GET /api/inbox_sweep` distributes (append-with-dedup) into
> per-repo `.m1nd/inbox.jsonl` + the medulla box for KNOWN roots only; `GET /api/mailbox?brain=` reads with
> fates and refuses unknown brains (consent law). Mission letters CAN ride the existing rail (free-form
> `class` + the `answers[]` reply graph = state updates and fates for free), but the F2.5 spec should bless
> a direct post verb into the target brain's box instead of squatting the field-report spool. **Field
> finding (filed as a `friction` letter in the spool):** `spool_path_for_runtime` falls back to
> `<home>/.m1nd/field-reports.jsonl` when the runtime dir lacks a `.m1nd` component — an ephemeral owner
> sweeping READS the production spool (verified read-only this time: no known roots ⇒ no box writes) and
> would write real per-repo boxes if any root were known; spools must be runtime-scoped. Adopted doctrine
> for every future receipt: read the snapshot immediately before the import (the boundary may bump between
> the gate and the stamp; the reconciliation slice was still an open PR at exercise time).

### The design era CLOSED — six PRDs on `main`, one organism
The blueprints are complete and, as of this checkpoint, reconciled into a single constitution.
Each PRD, one line:

- **`docs/ORGANISM-PRD.md` — THE CONSTITUTION (the capstone).** One spine (the north packet),
  four grammars (the trust ladder · the belief lifecycle · provenance/no-leak · attention), one
  ritual (pre-orient → act → capture), and **THE LADDER (§C10): a single cross-PRD build order,
  rungs R0–R17.** It was adversarially verified (a critic pass whose corrections are folded back
  as amendments, §C11) — the constitution wins ties between the other five PRDs and carries the
  pointer that says so. This is the last blueprint of the design era; everything after it climbs.
- **`docs/MEDULLA-PRD.md` — antifragile memory across per-project brains.** The memory state
  machine (per-brain storage · `Origin-Brain` labels · tier recall with no cross-brain leak ·
  promotion into a shared doctrine tier), designed to get stronger under churn rather than drift.
- **`docs/SOUL-PRD.md` — PATHOS native, verified, curated. [R16 S0 + S1 substrate SHIPPED
  2026-07-05]** This very handoff is now a first-class m1nd type: `soul_check` parses THIS file into
  anchored claims and returns a freshness receipt (last run on cp10: 13 fresh · 14 stale · 61
  declared); `soul_read` is the explicit pull; `soul_update` (a `memorize` mode with `Soul-Source`
  provenance) + the §C8.4 curator seat check (grader ≠ author) are the curator's substrate. The
  automated curator sweep + the north-packet soul beat (S2) + the skill call-through (S3) are the
  honest residue. The soul rode LAST on the ladder as designed.
- **`docs/HUMAN-LAYER-PRD.md` — the human face.** The Hall (projects area) · the Living Tree
  (memory-decorated filetree) · the mailbox · the precision system (iconography, lenses, honest
  search) · the Pre-Flight card — an agent's memory made legible to a human.
- **`docs/TWO-TIER-BRAIN-PRD.md` — per-project brains + reception + cwd routing.** Each repo gets
  its own brain inside one served owner; reception tells a caller honestly when it is wearing the
  wrong brain; cwd routes each call to the right one.
- **`docs/NEXTGEN-AGENT-PRD.md` §O.12 — the delegation layer.** A parent hands a child a grounded
  packet and reads back a debrief; a parent that cannot ground the child honestly declines
  (delegation-abstain). OMEGA's reach extended from one agent to a tree of agents.

### The construction era OPENED — R0/R1/R5 shipped (#275)
The first three ladder rungs landed together — small, live-defect fixes that make the flagship
packet honest and lean before the medulla state machine builds on top of it:

- **R0 — packet honesty (MED-INV-6 false-absence fix).** A `north` beat over a **non-empty**
  memory store used to emit "No durable memory yet" whenever recall found nothing for the task —
  a false absence (reproduced live: ~20 claims on disk, `memory: []`, and still the empty-store
  line). Now `SessionState::light_memory_count()` reads the ground-truth `.light.md` count, the
  packet stamps `memory_exists = n`, and the false line fires ONLY when the store is truly empty;
  over a non-empty store the gap honestly says the store holds claims that did not match this task.
- **R1 — the packet diet (Budget Law).** The binding blew its token budget two ways, both live:
  the `ingest_roots` array was serialized twice byte-identically, and the memorize write-path
  minted a per-file ingest root for every memory sidecar. Fix: `graph_runtime_summary` carries only
  `ingest_root_count` (the full array lives once, in the fingerprint), and a `.light.md` written
  into the `agent-memory` store collapses to the single store-dir root. **Measured: the packet is
  battery-pinned at ~1,419 tokens** (budget ≤2k), with CI failing on dup-arrays / sidecar-roots /
  >2k growth.
- **R5 — separator-agnostic `display_name` (Windows CI honesty triage).** `basename_of()` assumed
  `/` separators, so the brain name misfired on Windows backslash paths — the chronic red Windows
  CI test. Now it splits on both `/` and `\` (trailing-sep, UNC, mixed, POSIX all covered): a gate
  described as blocking now blocks, and Windows CI is green.

Each rung shipped RED-first (a failing test that pins the defect) → GREEN, with the doc pass in the
same PR.

### The ladder is the build order (ORGANISM §C10, R0–R17)
An implementer reads §C10 alone and knows what to build next. The spine of the order, past the
shipped R0/R1:

> **R2** (M5a — storage split + `Origin-Brain` + migration + brainless-root refusal) → **R15**
> (the eviction gate: LRU + persist-on-evict; a HARD pre-condition for the next rung's
> `all-brains` half) + **R3** (M5b — `tier` recall + no-leak proven + `all-brains`) → **R4** (M6 —
> the `promote` verb with its provenance riders) → delegation (R6 `delegate`/`debrief`) → mailbox
> (R8/R9 boxes + view) → **R10** the Pre-Flight Card → **R16** the SOUL PRD + slices, LAST, bound
> by the constitution's seven soul constraints.

The two integration points every rung composes over are the **write door** (§C4) and the **packet
spine** (§C1). No rung lands without its battery case first (RED), its doc pass, and the landing
gate. R5 (Windows) and R17 (a conformance-boost rerank that lets X-RAY steer attention) sit off
the critical path.

### Runtime reality
The served owner warm-boots multiple **per-project brains** inside one process. Per-brain **Open**
works end-to-end (a hosted project's tree opens by name, not by plumbing path). **Reception is
honest** — a caller in repo X wearing repo Y's brain is flagged, not silently served. The **Hall**
renders every brain the owner holds as a named project, freshest-first, with absent-honest counts.
Activating a new UI or a new binary needs a served-owner restart (the dist is rust-embedded; the
binary is warm-booted) — note it honestly at each cut.

## Operating Doctrine
Proof-grown: measure before claiming; verify work yourself (re-run the battery / a probe), never
trust a report. Battery-gate risky core changes. Fix AND test every defect (RED-first: a failing
test that pins the defect before the fix). Commit+push always (PR → CI → merge). Never bypass branch
protection (admin-merge is blocked by design). Land deep changes with a tight, source-grounded spec
+ a battery gate; verify on the REAL diff. Update this file at big checkpoints.

**The ladder is doctrine now:** the next rung is whatever §C10 says is next — read the constitution's
build order before opening a new front, and climb it in dependency order (R15 is a hard
pre-condition for R3's `all-brains`; the soul rides last). Divergence ripples out through §C11-style
amendments to the constitution, never by silent contradiction.

**Universal field-telemetry doctrine.** Every agent, every repo, is a sensor. When m1nd misbehaves
during ANY mission — even on another repo — the agent REPORTS, it does not fix: append one JSON line
to the machine-global mailbox `~/.m1nd/field-reports.jsonl`
(`{ts,agent,repo,tool,class:"bug|honesty|friction|win",what,expected,snippet}`) and keep working.
Report-never-fix mid-mission is the rule. The `honesty` class is the most valuable — it is
calibration ground truth (m1nd overclaimed and was wrong). When retrieval was simply right/wrong,
prefer the built-in `learn` verb (correct/wrong/partial). Triage closes the loop: every improvement
session STARTS by sweeping the mailbox (+ `seek` for field memories), and a confirmed field bug
becomes a battery case/test BEFORE the fix. The mailbox is local-only — m1nd never phones home.

**Agent-docs gate (CI, PR-only):** `scripts/agent_docs_gate.py` + the `agent-docs-gate` job FAIL any
PR that changes an agent-workflow surface (the MCP `M1ND_INSTRUCTIONS` string / tool schemas / verb
dispatch, `protocol/`, `help_guidance.rs`, `universal_docs.rs`, `skills/`, or the npm host installer)
without ALSO updating agent-facing docs in the same PR (`skills/`, `docs/` incl. the wiki, `README.md`,
`CONTRIBUTING.md`, or a root `CLAUDE.md`/`AGENTS.md`). It arms only on those surfaces (anti-cry-wolf);
an instructions-only edit self-satisfies; the `agent-docs-exempt` label skips it for genuine
no-behavioral-change refactors.

**CI cost discipline (`.github/workflows/ci.yml`).** m1nd is the account's heaviest Actions
consumer (a burst can fire ~19 PRs, each a 3-OS matrix, in two days), so the CI is tuned to spend
only where it buys bug-catching. Two levers, both coverage-neutral: (1) a `concurrency` group keyed
by ref with `cancel-in-progress` — a newer push to a PR cancels its own in-flight run (no more one
full matrix per rapid fix-push), while the protected branch keeps every run (`cancel-in-progress`
is false on `refs/heads/main|master`); other PRs, on different refs, are never touched. (2) The
release-mode build (`cargo build --release` × 3 OSes) is OFF the PR/push path — it was never a
required check, `cargo check`/`test`/`clippy` already compile the whole workspace on every PR, and
`release.yml` builds + ships the release binaries per target on tag. **Deliberately PRESERVED:** the
cross-OS `test` matrix (ubuntu/macos/windows, `fail-fast: false`) — it is what caught the macOS
port flake; blinding it trades a dollar bill for a bug you ship. **Deliberately NOT done:**
workflow-level `paths`/`paths-ignore` filters — the required checks are exactly `Test`, `Clippy`,
`Format`, and a path-skipped required check never reports, trapping a docs-only PR forever; the safe
levers above capture the waste without that risk.

## Access Map
- Battery harness: `scratchpad/m1nd_battery.py` — **TRACKED in-repo** (protected by the `.gitignore`
  negation `!scratchpad/m1nd_battery.py`, so it survives scratchpad clears). Fresh ingest +
  ground-truth PASS/FAIL + `rg` head-to-head; the m1nd suite runs green with zero grep losses.
  (Prior throwaway probes `impact_probe.py`/`edge_proof.py`/`focus_smoke.py` and the
  `M1ND_BATTERY_REPORT.md`/`battery_FINAL.txt` reports were scratchpad-cleared — pruned here by the
  R16 curator pass; the tracked battery is the durable one.)
- **The soul is verified now (R16 · `soul_check`):** run `soul_check` (or `soul_read`) against THIS
  file — it parses PATHOS into anchored claims and returns the freshness receipt (N fresh · M stale ·
  K priced @sha). The pathos skill is the AUTHORING guide; m1nd is the ENGINE.
- Build: `cargo build -p m1nd-mcp --bin m1nd-mcp` → `./target/debug/m1nd-mcp`.
- **The constitution + the build order:** `docs/ORGANISM-PRD.md` (§C10 is THE ladder; §C11 the
  amendment ledger). The five other PRDs: `MEDULLA-PRD.md`, `SOUL-PRD.md`, `HUMAN-LAYER-PRD.md`,
  `TWO-TIER-BRAIN-PRD.md`, `NEXTGEN-AGENT-PRD.md` (§O.12 delegation, §O.10 the OMEGA floor roadmap).
- Runtime PRDs: `docs/X360-RUNTIME-PRD.md`, `docs/FOCUS-RUNTIME-PRD.md`. Ambient layer per host:
  `docs/HOST-INTEGRATION-MATRIX.md`.
- git identity = Max Kle1nz <kleinz@cosmophonix.com>.

## Known Problems (honest, product-level)
- **M1ND-10 has named candidate blockers; use the canonical handoff, not an older green summary.**
  The former five G6 P1 and three P2/scorer findings are now corrective-source implemented and
  locally green; the final independent corrective re-review is `APPROVE`/none. The formal 220-task
  blind run is still absent and cannot run from the mutable tree.
  G7's organism-version and offline-cache defects are locally closed at checkpoint 22; its real
  isolated browser/owner/h4nd proof is still absent. The remaining blocking set also includes
  an immutable candidate, native cross-OS/power-loss/hosted-release proof, production autonomy
  custody, same-UID micro-race/peer-identity evidence, and same-candidate G10 receipts. Exact
  findings and continuation order:
  `docs/M1ND-10-HANDOFF-20260719.md` §§7–10.
- **The medulla ladder R2→R4 (M5a → M5b → M6) is BUILT — CODE-LANDED, live HELD.** M5a (per-brain
  storage + `Origin-Brain` labels + reversible migration + brainless-root refusal), M5b (`tier` recall
  + the no-leak invariant proven + `all-brains` through the eviction gate), and M6 (the `promote` verb
  + the C8.2 origin-qualified-evidence rider + the C8.3 verified-only gate + demotion) have all shipped
  to `main` with RED-first proofs. **The LIVE owner at `:1338` has NOT been migrated/restarted** — the
  code lands and is scratch-proven per slice, but serving the new storage + `promote` verb on the live
  owner needs a maintainer rebuild/kickstart (held deliberately). Next real build: R6 (delegation —
  `delegate`/`debrief`).
- **Per-brain session-counter partition is PENDING (ladder R14, §9.5.1).** In one served owner,
  session/query counters are not yet partitioned per brain, so aliveness counts can bleed across
  brains in the Hall. Backend work budgeted, not done.
- **The `seek` rerank centrality-vs-semantic balance was corrected (pre-R17).** Previously a
  high-PageRank node could out-rank a more semantically-relevant hit — the `graph_activation * 0.2`
  centrality prior was added ungated, so a near-zero-relevance hub could ride pure centrality to the
  top. Fixed by gating the activation term with the node's own relevance (`max(sem, keyword,
  trigram)`): centrality stays a full-strength co-ranker/tie-breaker for relevant nodes but can no
  longer swamp the semantic signal for irrelevant ones. This lands the balance FIRST, before R17
  adds `conformance_boost` to the same rerank — so X-RAY steers attention on top of a correct base.
- **`x.method()` receiver-type inference — the #1 remaining GRAPH gap.** A bare `x.method()` on a
  local/field receiver carries no qualifier, so same-name ties fall to proximity / `candidates[0]`.
  Qualified calls (`Type::method()`, `module::func()`) and cross-file proximity are solved;
  receiver-type inference (track `let x: T` / field types / fn return types) is the dedicated harder
  cycle. Method-call edges exist for Rust but not TS/Java/Go/Python.
- **`why`-closure UNRESOLVED node-granularity.** The `unresolved` closure tag still over-fires at
  node granularity (the ambiguous tag was fixed to edge granularity; unresolved was not): a clean
  path leaving a node that drops any outbound ref (e.g. a std/external call) still reads `blocked`.
  It needs a design decision — a dropped ref has no target node to key an edge-specific tag against.
- **`predict`'s strength model is COARSE.** Calibrated against m1nd's own history it tops out around
  ~28% act-band precision at ~15% coverage. The calibrator is honest — `act` is structurally withheld
  until the number clears a risk budget — but the underlying strength model (`0.1·N` in neighbor
  count) needs a real upgrade before `predict` can `act` at useful coverage.
- **The poisoned-oracle threat model is OPEN.** A poisoned eval set or co-change corpus makes the
  calibrator certify a wrong verdict with confidence — "who calibrates the calibrator?". Logged as
  unsolved; eval-set integrity is a prerequisite before any verb defaults on.
- **PATHOS auto-refresh is a review-only proposal, not an autonomous writer.** The workflow now has
  `contents: read`, disables checkout credential persistence, renders commit-derived text as inert
  quoted data, and uploads only `PATHOS.patch` plus a `REVIEW_ONLY_NO_COMMIT_NO_PUSH` receipt. It has
  no PAT seam and does not commit, push, bypass branch protection, or mutate repository state. A
  maintainer-reviewed PR remains the promotion boundary; hosted execution is still `NOT_RUN` here.
- **Multi-session hygiene.** A served owner holds the live brain and sibling worktrees may hold
  parallel work — `git fetch` before acting, confirm `git branch --show-current` before commit, do
  feature work in an isolated worktree under that worktree's OWN `CARGO_TARGET_DIR`
  (`export CARGO_TARGET_DIR="$(scripts/cargo_target_dir.sh)"` — sharing one lets a gate link a
  sibling's binary, checkpoint 36), and `git worktree remove` it when done.

## Proof Standard
Done = `cargo test --workspace` green + clippy `-D warnings` + `cargo fmt` clean + the BATTERY
(`scratchpad/m1nd_battery.py`, tracked) green on the m1nd suite (zero grep losses) showing the
targeted tool improved with a concrete example, zero regression. CI green on 3 OSes before merge.
**For UI/human-layer slices:** INV component tests against REAL captured envelopes
(`m1nd-ui/src/__fixtures__/`) + the violet-lint (violet reserved for abstain/unknown) + the icon-lint
+ a live dogfood against a `--serve` of m1nd's own graph. **For OMEGA/prediction verbs,
calibration-gated JOINS battery-gated:** battery tests prove the code does what it says (consistency);
the calibrator proves the verdict is right often enough to act on (correctness-at-coverage). A verb
earns `act` as an allowed output ONLY when measured precision-at-coverage clears the stated risk
budget — until then `act` is structurally withheld and the verb emits `reverify`/`abstain`/`unprovable`.
Recalibration, not retraining: the number is re-measured against ground truth, never asserted in a
README. Engine cadence: each rung lands in a worktree-isolated slice with a source-grounded spec +
battery gate → verify on the REAL diff → PR/merge → the UNIVERSAL DOC GATE (docs/wiki/README/PATHOS
current, agent surfaces updated in the SAME PR) → seed the next rung.

## Next Agent Prompt / next seeds

**CURRENT OVERRIDE — M1ND-10.** Read `docs/M1ND-10-HANDOFF-20260719.md` in full and follow its
continuation order. The five G6 P1, three P2, and scorer-audit findings are locally closed and the
final independent corrective re-review is `APPROVE`/none. Preserve that boundary and the closed G7
version/offline-dependency contract. The dirty tree has a green local aggregate but is not a
candidate: obtain the authority needed to freeze one reviewed immutable revision, repeat the full
matrix against that digest, and only then execute native G4, formal blind G6, isolated live G7,
hosted G8, production G9, and same-digest G10. Do not touch the served owner, inspect operator-only
labels, run the blind benchmark from mutable source, weaken a refusal, or convert source support
into a live/release/autonomy claim.

**Historical ladder below.** It remains useful organism history, but it is not the active ordering
for the owner-ratified M1ND-10 convergence program.

**→ THE ERA IS CONSTRUCTION.** The design era is closed; do not write another vision. Read the
**ORGANISM constitution's §C10 ladder** — it is the single cross-PRD build order, and an implementer
reads that chapter alone and knows what to build next. R0/R1/R5 shipped (#275); **R15 → R2 → R3 → R4
are now SHIPPED (code-landed, live held)** — the whole medulla memory spine (storage split, tier
recall, promotion) is real code with RED-first proofs. **Climb from R6 onward, in dependency order,
RED-first per rung:**

1. ~~**R2 — M5a: the medulla storage split**~~ **[SHIPPED #279]** Per-brain storage + `Origin-Brain`
   labels + reversible migration + brainless-root refusal. The long pole; every later memory rung
   stands on it.
2. ~~**R15 — the eviction gate**~~ **[SHIPPED #277]** LRU + persist-on-evict in the owner. The hard
   pre-condition for R3's `all-brains` half.
3. ~~**R3 — M5b: `tier` recall + no-leak proven + `all-brains`**~~ **[SHIPPED #280]** The leak
   permutation matrix, medulla-doctrine-surfaces-cross-brain, `all-brains` through the eviction gate.
4. ~~**R4 — M6: the `promote` verb**~~ **[SHIPPED]** The audited crossing + the C8.2 origin-qualified
   evidence rider + the C8.3 verified-only gate + demotion, agent-workflow surfaces in the SAME PR.
   **Next:** delegation (R6 `delegate`/`debrief`), then the packet memory slice (R7), the mailbox
   (R8/R9), the Pre-Flight Card (R10), and — LAST — the SOUL PRD + slices (R16), bound by the
   constitution's seven soul constraints.

**Doctrine pointers (carry verbatim into every spawned agent):** the **UNIVERSAL FIELD-TELEMETRY
DOCTRINE** (every agent/repo is a sensor → REPORT to `~/.m1nd/field-reports.jsonl`, never fix
mid-mission; `honesty` class is calibration ground truth; a triage session STARTS by sweeping the
mailbox, and a confirmed bug becomes a battery case BEFORE the fix); the **UNIVERSAL DOC GATE incl.
agent surfaces** (docs/wiki/README/PATHOS current before "done"; any change to HOW agents work
updates the agent-read surfaces in the SAME PR — the agent-docs CI gate enforces this); the **DISK
HYGIENE rule** (per-checkout `CARGO_TARGET_DIR` under one auto-deletable cache root + worktree
sweeps).

---

**↓ THE FLOOR (still-true reference): m1nd-OMEGA, `docs/NEXTGEN-AGENT-PRD.md` §O.10** — the verifiable
trust substrate, released and live. Moves 0 (conformal calibration harness) + 1 (the Trust-Gated
Envelope) are DONE and RELEASED (v1.2.0/1.2.1); the honest Move-2 reframe shipped. Read §O.1–O.11 for
the vision (answer + map + trust receipt), the calibration keystone (consistency ≠ correctness), the
baked-in critic corrections, and the open poisoned-oracle risk. **Move 2 (Solvency & Stop Gate)
remains a DESIGN task, not a build task — it is roadmap-only and un-grounded:** m1nd has no token
ledger, so a solvency arbiter would need a real budget signal wired or built net-new before it could
mean anything, and its `file:line` anchors must be re-verified against current `main` first. It is NOT
the active north — the ladder is. Return to deepening the substrate only if construction ever needs it.

## Do Not Do
- Don't edit/build m1nd source while a battery/subagent is building on the shared worktree (corrupts
  its measurement). Don't admin-merge / bypass branch protection. Don't claim a rung works without a
  battery re-run on the REAL diff. Don't delete unmerged branches without patch-id proof. Don't open a
  new front off-ladder — the constitution's §C10 order is the north; diverge only via a §C11 amendment.

## Open Questions
- Should auto-freshness default-on (watcher per ingest) or opt-in? (decide with a battery staleness
  scenario.)
- Does the `impact` symbol-first ranking want to differ by direction (reverse=callers vs
  forward=dependencies)?
- Where does the `last_used`/reinforce-on-use signal come from (a `learn`-style feedback on recall? an
  auto-stamp on `activate` touch?) — the blocker on the memory subsystem's reinforce/consolidate moves.

## Prior Eras (summary — full text in git history)
- **Checkpoint 9 (2026-07-03/04) — the construction era opens.** Three PRDs made official (HUMAN-LAYER,
  §O.12 delegation, TWO-TIER-BRAIN); the first human surfaces SHIPPED (Living Tree, the Hall, the tree
  precision system, per-brain Open); reception degraded-mode shipped; the field-report mailbox swept to
  empty in ~a day (each report a battery case before its fix). Releases: v1.3.0 (the shell reaches every
  host — 22-host recipes, MCP-Registry manifest), v1.3.1/1.3.2 (discoverability + the launch funnel).
- **Checkpoints 8 / 8.1 (2026-07-02/03) — the first OMEGA-era releases.** v1.2.0 (OMEGA Move 0
  calibration + Move 1 Envelope + the honest Move-2 reframe + `north` pre-orient + memory moves #1–#6)
  and v1.2.1 (the compounding fix — `north` composes L1GHT agent-memory recall — plus field-triage
  fixes) cut, published, and rebuilt into the served runtime. The universal field-telemetry doctrine
  established here.
- **Checkpoint 7 (2026-07-01) — memory roadmap #3–#6 + the pre-flight A/B.** Age-staleness,
  per-type decay, supersession-on-rewrite+flock, recency-capped auto-load all shipped. The first A/B
  proved `north` pre-orient HELPS orientation and does no harm, but found compounding architecturally
  blocked in process-per-hook — the insight that the ambient loop's real prerequisite is `--serve`/`--attach`.

---

<!-- ────────────────────────────────────────────────────────────────────────
  AUTO-GENERATED SECTIONS — do NOT hand-edit between the anchors below.
  Everything ABOVE this line is hand-curated and never touched by automation.
  The auto-changelog (git-cliff over Conventional Commits) and auto-overview are
  regenerated on every push to `main` by .github/workflows/pathos-autorefresh.yml
  and committed back as Max Kle1nz with [skip ci].
──────────────────────────────────────────────────────────────────────── -->

## Auto — changelog (since the last `vX.Y.Z` tag)

<!-- BEGIN:auto-changelog -->
### Unreleased

**Chores & infra**

- Stage the refreshed PATHOS per-path — a missing pathspec made git add atomically stage nothing (#237)
- Agent-docs gate — agent-workflow changes require agent-facing doc updates (or explicit exempt) (#229)

**Docs**

- Checkpoint 9 — the construction era opens (3 PRDs, Living Tree, mailbox swept) + auto-refresh installed (#236)
- The shell is the product — README re-spined around the operating loop (#228)
- TWO-TIER-BRAIN-PRD — per-project brains + shared medulla (official, proof-grown) (#227)
- The 5 launch plates (SOFT PROOF, maintainer-approved) (#223)
- §O.12 — the Delegation Layer (packet, debrief, delegation-abstain) (#224)
- HUMAN-LAYER-PRD — the Living Tree, post-it memory, Pre-Flight hero (vision → spec) (#222)

**Features**

- Living Tree slice 0 — the tree, post-its, trust dots (read-only) (#232)

**Fixes**

- Write-tool responses return real envelopes through the bridge (field-triage L21) (#235)
- Remove the opt-in savings/report unmeasured-claims surface (brand gate G1.5) (#234)
- Marker fragments excluded from recall/anchor slots (field-triage batch A) (#231)
- Re-init covers all unknown-session shapes + restart-survival proof (field-triage batch C) (#233)
- All persist targets resolve against runtime_root, never cwd (field-triage batch B) (#230)
- L1GHT recall robust on mixed graphs — memory beat scoped to light provenance (field-triage #6) (#226)
- Bridge transparently re-initializes on owner restart (-32001) (field-triage #5) (#225)
- Remove the unmeasured savings envelope — an uncalibrated claim is the confident guess (brand gate G1) (#221)
<!-- END:auto-changelog -->

## Auto — repo overview

<!-- BEGIN:auto-overview -->
- **Repo:** `m1nd`
- **Branch:** `main`
- **Last commit:** 2026-07-03
- **Commits since `v1.2.1`:** 17
<!-- END:auto-overview -->
