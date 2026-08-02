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

## Open fronts — the declared debt (2026-07-24, delta 2026-08-01)

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

**Delta 2026-08-01 — THE FIRST GRAPH CAN BE BORN: the product had no first-value path, on either side of the room:**
- **Measured on 1.6.2, in a virgin repo with an empty runtime, both actors dead-ended.** (a) An agent
  calling `ingest` with only `agent_id` on an EMPTY graph is refused
  `generic_action_authority_required: semantic_action=graph.ingest.replace
  authority_floor=POSITIVE_SOVEREIGN` — correct policy, and the README told every reader the opposite
  ("the agent may call `ingest` directly on an empty graph"). (b) The human's `m1nd init --birth .`
  exited **0** reporting 10 nodes, and the very next stdio session in that repo served **0**: the
  ceremony minted a project-brain sidecar under `<runtime>/project-brains/<key>/`, which only the
  served owner's HTTP caller-root routing reaches (`http_server::resolve_brain`), while a plain stdio
  owner serves the runtime's own graph and nothing else. A ceremony that succeeds loudly and delivers
  an empty brain is worse than one that refuses.
- **What the spec actually says, checked before changing anything.** `docs/GENESIS-INGEST-CONSUMERS-SPEC.md`
  admits exactly two ingest doors: SPEC-1 `refresh` (A2-local, exact declared root) and SPEC-2 `birth`
  (PositiveSovereign, owner-stamped `human-cli`, minted only by this ceremony). Same-root `replace` on an
  empty graph is **not** among them, and §1.1's purity rule (classify from `(tool, params)` alone, no
  trusted route facts) forbids implementing it as a classifier change, because "the graph is empty" and
  "the caller stands at the workspace root" are owner facts the pure pre-brain gate cannot see. So the
  spec is silent on (a) by construction, and the fix went where it is unambiguous: the human's door.
- **The fix, and why it does not weaken cross-root sovereignty.** `run_ceremony` now decides WHICH brain it
  is filling from a fact about the OWNER, never about a caller: if the runtime it boots from lives INSIDE
  the root the human named and the bound graph is empty, that root's brain IS this owner's own graph, so the
  ceremony fills it (`brain: "owner_bound_graph"`) instead of minting a sidecar nobody reads. Any other
  root takes the hosted path exactly as before, with every guard intact; agents gained no door; generic
  `ingest` is as refused as it was. The first ingest commits through the brain actor
  (`McpServer::ceremony_first_ingest`), because a graph written into the runtime behind `CURRENT` is
  reverted on the next boot — the `legacy_snapshot_adoption` incident, re-measured live here (a populated
  snapshot dropped into a once-booted runtime came back as 0 nodes).
- **Honesty, the half that was worth more than the mechanism.** A birth that scans to zero nodes now REFUSES
  (`birth_produced_empty_graph`, exit 1, naming what to check) on BOTH doors. And every refusal on this path
  names the way out: the field agent that found this defect hit four correct refusals
  (`generic_action_authority_required`, `refresh_caller_root_unknown`, `needs_authority_not_proven`, the
  birth verb's own), none of which mentioned `m1nd init --birth`, and concluded in writing that the product
  could not be used. Fixed at the floor gate (for the first-graph actions only — §5.9's two A2 siblings keep
  their pinned bytes), both `refresh_*` root refusals, `north`/`delegate`'s `next_move`, `recovery_playbook`
  (whose `use_served_owner_authenticated_ingress` step was fiction for a repo with no brain), the npm CLI's
  `needs_authority` envelope, and the served MCP instructions. The `_m1nd` envelope also stopped announcing
  "ingested: 0 nodes, 0 edges. graph ready." over a refusal.
- **Proof:** `m1nd-mcp/tests/first_graph_is_born.rs` drives the REAL binary from a virgin repo — first
  contact refused with the door named, the ceremony, then a SECOND boot that must serve what the ceremony
  reported. RED on today's code at both assertions, GREEN after.
- **Debt this front declares rather than hides.** (a) `refresh_caller_root_unknown` is still the answer for a
  plain stdio owner even AFTER the graph exists, because `caller_root` is a client-supplied header and a
  stdio owner has none — the refusal now says so and names the attach bridge, but an owner that knows its own
  cwd arguably should carry it; not touched here (SPEC-1's ingress rules are its own front). (b) A repo born
  before this change owns an orphan sidecar under `project-brains/`; the ceremony refuses to birth over it
  (`birth_destination_not_empty`) rather than deleting anything, which is the right refusal and leaves the
  cleanup unwritten. (c) The spec has no clause for the solo topology at all — the resolution is recorded in
  `brain_birth::home_birth_verdict` as a resolution, not a reading.
- **The handoff's ubuntu red was the ceremony ingesting ITSELF, and it was worse than a flake (2026-08-02).**
  With `runtime == repo root`, the source walk swallowed the runtime's own state: measured with an
  instrumented walk, 32 of the birth's 39 nodes were checkpoint blobs, lease files and boot sidecars — the
  first-value graph was born 82% runtime droppings — because `path_policy::RUNTIME_ARTIFACT_FILE_NAMES`
  aged silently (10 names covered, ~20 live state files not: `daemon_state`, `temporal_state_v1`,
  `checkpoint-store/**`, `registry/**`, `boot_*`…). The ubuntu failure was the same defect's second face:
  any lease-heartbeat/daemon write BETWEEN the extraction walk and `require_complete`'s revalidation walk
  shifted a captured mtime and killed the ceremony with `FullReindexRequired: VCS/file-metadata context
  changed since extraction` — a race ubuntu lost and macOS happened to win. Fix with zero schema change:
  `tools::code_ingest_config` injects the runtime's own top-level names into `skip_dirs`/`skip_files` when
  (and only when) the scanned root covers the runtime root — those fields already travel in the pipeline
  receipt and already drive revalidation, so extraction and `require_complete` see the same world forever.
  The new lists are gated by `the_runtime_owned_list_covers_what_a_real_session_writes`, which boots a real
  session and fails NAMING any state file the exclusion misses — the validator the old list never had, and
  it bit on its first run (`antibodies.json.bak`; the `.bak` class is now covered on both sides). Proof:
  walk 34→2 files, birth 39→7 nodes with zero runtime nodes, pollution test proven RED with the wiring
  removed, e2e 3/3 local; the ubuntu leg is the CI's to prove. Residual, filed not fixed: sibling seams
  still build a raw `IngestConfig` (`AutoIngest::replace_graph`, audit/layer handlers) — same disease if
  their scanned root ever covers a live runtime; and the runtime-at-root layout itself (state files strewn
  among user code) is the underlying illness — moving state under one hidden dir is a product decision for
  the owner's table, recorded in the project inbox.

**Delta 2026-08-01 — the menu fits on one screen (the shop window closes on the core):**
- **THE OWNER'S ACCEPTANCE RULE FOR THIS FRONT, verbatim and standing (ratified 2026-08-01):**

  > "o m1nd tem que voltar a ser rápido, fácil, e que o agente tenha VONTADE de usá-lo porque
  > realmente é incrível e resolve seus problemas."

  This is the definition of done for the whole width-vs-use front, not for one PR. In proof terms:
  a virgin repo's first `north` delights in seconds · **the menu fits on one screen** · telemetry
  (#514) shows adoption moving in the coming weeks. Judge every decision on this front against
  that sentence; it outranks any local cleverness, and it does not expire when this PR merges.
- **The advertised menu is now a core of 15, measured not guessed.** Two measurements decided it.
  (a) Six weeks of real agent traffic (458 calls / 157 sessions, reconstructed from host
  transcripts): **141 verbs advertised, 13 ever called**; 69 verbs live in prefix families and
  across ALL of them exactly **2** calls were ever made (`xray_paint`, `system_blocks_snapshot`) —
  `perspective_*` (12 verbs), `mission_*` (8), `trail_*`, `document_*`, `daemon_*`,
  `auto_ingest_*` (4 each), `antibody_*`, `candidate_*`, `authority_*`, `transplant*` (3 each),
  `calibrate_*`, `soul_*` (2 each) sat at absolute zero; 11 standalone verbs carried **370 of 458
  (81%)**. (b) An independent external evaluator, told to exercise everything on a foreign
  107k-LOC codebase, ranked `surgical_context` in its **top three** — a verb with **1 call** in six
  weeks. The verbs are not bad. They are invisible, and a 141-item menu is what made them so.
  `tools/list` now serves `CORE_TOOLS` (the owner-ratified 12: `north`, `memorize`, `ingest`,
  `seek`, `search`, `health`, `trust_selftest`, `view`, `impact`, `session_handshake`,
  `boot_memory`, `surgical_context`) ∪ `HOST_BINDING_REQUIRED_TOOLS` (`help`, `doctor`,
  `recovery_playbook`). This is the second move against the owner's harshest grade — width-vs-use
  4 — and the complement of 07-31's: that one composed unused verbs INTO the verb everybody calls,
  this one stops advertising a wall in front of both.
- **Nothing was removed, and that is mechanical.** The cut lands at exactly one seam,
  `tool_schemas_for_tier` (the menu). `all_tool_schemas` (the registry) and dispatch are
  untouched, so the two policy-parity guards in `action_routes.rs` still compare the registry to
  `MCP_TOOL_ROUTE_NAMES` and keep their full meaning, and the `POLICY-DISABLED` floor annotations
  still ride on every description because they are applied before the filter runs. A battery test
  names a hidden verb over the real MCP wire and asserts it still answers.
- **Discovery is the price of hiding, paid three ways.** `help` now catalogs the FULL registry at
  every tier (it read the tier-gated list before — a gap already on the record in
  `docs/uml/tool-surface.md` as low severity, promoted to load-bearing by this change and now
  CLOSED); the menu's `help` entry carries a computed `[CORE MENU]` line stating the live hidden
  count; `health.tool_surface_contract` gained `hidden_tool_count`, `hidden_tools_are_callable`
  and a `discovery_rule`; and the initialize instructions teach it before anything else.
  `M1ND_TOOL_TIER=full` remains the operator opt-out — an unrecognised value now falls back to the
  CORE, never to the wall.
- **Debt this front declares rather than hides.** (a) **The served owner will not see the small
  menu until its launchd env changes** — that plist sets `M1ND_TOOL_TIER=full`, which is exactly
  why the six-week measurement saw 141 advertised. The change is correct for every fresh install
  and inert for this machine until the owner drops that var; it is his config, outside the repo,
  and no agent should touch it. (b) The `lock_*` family is still invisible to `help` — not because
  help is narrow now, but because those five verbs are in no registry at all; fixing it means
  registering their schemas. (c) The core is a judgement about which verbs a new agent should MEET
  first, and #514's telemetry is what will falsify or confirm it — until those weeks pass the list
  is ratified, not proven. (d) `docs/uml/tool-surface.md` and `docs/UML-ORGANISM.md` still carry
  pre-existing stale line numbers and catalog counts (117/122/41) from before this front; only the
  lines this change invalidated were corrected.
- **The red the handoff named is paid (2026-08-02), and it was a real coupling.**
  `cockpit_budget_holds_with_the_eighth_slot` broke not because the menu shrank but because the
  DOOR widened: `help` now catalogs the full registry, so the cockpit — whose root lifts each
  entry's `why` from that one canonical catalog (amendment 10) — began embedding raw schema
  descriptions for the verbs the old tier-gated catalog could not see (`system_blocks_snapshot`
  alone: 762 chars; on main those four `why`s were silently EMPTY). Measured: root 914 tokens
  against the ≤800 ceiling. The fix is the budget owner's, in the cockpit: `menu_why` cuts the
  line at `MENU_WHY_CAP` (120 chars, sentence-boundary first, honest ellipsis), root back to
  ~633 tokens, and `menu_why_is_bounded_for_every_routed_verb` pins BOTH directions — no future
  description can resize the root, and no help regression can silently empty the menu's `why`
  again.

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

## Current State

> The live state is the LATEST CHECKPOINT at the top of this file (checkpoint 39, 2026-08-01) plus
> "Open fronts" and "HONEST HANDOFF (e) — state the successor inherits". There is no separate
> current-state narrative anymore.
>
> The former "Current State (2026-07-20, checkpoint 27)" section — 951 lines of July-era M1ND-10
> gate logs and a superseded 2026-07-15 "next agent starts HERE" handoff — was retired in the
> 2026-08-01 audit that closed the session rotation: a successor must never find two handoffs in
> one file. Full text: git history of this file (any commit before that audit).

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
