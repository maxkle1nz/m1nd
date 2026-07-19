# M1ND-10 G10 repository security truth

> Assessment status: Point-in-time, evidence-bounded repository assessment  
> Observed risk: `LOW_OBSERVED`  
> Assurance: `MEDIUM`  
> Observation point: `b59a1c2a1454a83164dfb4d5640c6b005154d1ee` plus the dirty working tree identified below, refreshed `2026-07-19T18:16:35Z`  
> Mode and scope: `standard`, M1ND-10 authority, local transport, ingestion, managed writes, runner ownership, updater, and release-integrity surfaces

## 1. Executive truth

No current `CRITICAL` or `HIGH` vulnerability was confirmed in the bounded surfaces reviewed. The strongest attacker paths examined are fail-closed: a browser cannot bootstrap a local session without a one-shot nonce and origin/host checks; generic MCP authority routes cannot exercise elevated authority; `apply_batch` does not execute repository-controlled verification code; managed writes select owner-controlled targets; GROBID redirects and unapproved endpoints are refused; transport admission and SSE resources are bounded; updater and release paths refuse unverifiable promotion.

This is not a release clearance. The assessment observed a live, concurrently changing dirty tree. The current-tree local aggregate now passes, including full workspace tests, strict Clippy, static checks, and a release build, but it is not an immutable candidate receipt. The Unix authority owner now pins its root descriptor and device/inode identity and deterministically refuses symlink/rename/recreate replacement, reducing one same-UID replacement surface; this does not prove resistance to micro-TOCTOU, hostile cross-process replacement, or loopback peer impersonation. Hosted GitHub execution, protected-environment settings, exact public publication, deployed runtime, cross-platform behavior, physical-presence ratification, residual same-UID races, and physical crash durability were not proven at this observation point. With no confirmed or likely reachable finding, observed risk is `LOW_OBSERVED`; those material unknowns cap assurance at `MEDIUM` and still block release.

### Decision table

| Decision | Verdict | Evidence/rationale |
|---|---|---|
| Urgent containment required | No | No confirmed reachable vulnerability or credential was found; E-003 through E-014 |
| Release/deploy should be blocked | Yes | Current dirty-tree aggregate is green, but no immutable candidate exists and hosted release/environment controls remain `NOT_RUN`; E-001, E-011, E-015 |
| Critical assets plausibly exposed | No observed exposure; residual unknown | Authority, filesystem, session, and release paths are locally guarded, but deployed and same-UID behavior were not observed; E-003, E-006 through E-011 |
| Evidence sufficient for current decision | Yes, for the decision to keep release gated | Local negative tests and source traces are sufficient to preserve the gate; not sufficient to authorize publication |

### Strongest existing controls

- Generic authority is limited to `Ordinary`; elevated classes and aliases are rejected without state mutation, and typed consumers remain mandatory.
- `apply_batch` records dynamic verification as `NOT_RUN` and forces `RISKY`; owner-side code never executes a repository `build.rs`, test runner, or verifier.
- Browser bootstrap uses a CSPRNG one-shot nonce, strict host/origin/fetch checks, `HttpOnly` and `SameSite=Strict` cookies, and query cleanup in the UI.
- Managed write APIs do not accept caller output paths; parent and target symlinks/non-regular files are rejected and temporary files use owner-selected stores.
- Transport caps session admission before state allocation and bounds session count, idle lifetime, SSE concurrency, and security-sensitive header sizes.
- Runner secrets require a regular non-symlink `0600` file containing a 32-byte random value encoded as 64 hex characters; registry and response sizes are bounded.
- On Unix, the authority owner opens its managed root and lock with no-follow/CLOEXEC semantics, pins the root directory descriptor plus device/inode identity for the owner lifetime, and fails closed on deterministic symlink/rename/recreate replacement.
- Release logic binds tag, commit, UI, npm tarball, and candidate bytes and refuses unverifiable update or publication paths.

## 2. Scope and observation point

| Field | Value |
|---|---|
| Repository root | repository root (`<repo-root>`) |
| Revision / working tree | HEAD `b59a1c2a1454a83164dfb4d5640c6b005154d1ee`; dirty |
| Snapshot identity | 245 porcelain entries: 112 modified, 30 deleted, 103 untracked entries; status SHA-256 `172ecf44b39e1931a89f3548ce075c2fb4024ade4d24c058250250312c3915d1`; tracked binary-diff SHA-256 `acba014dee901982e5ea344b96c53fd84e63a898192376cce3adf80a8da1a2fb` |
| Assessed at | Base observation `2026-07-18T17:21:07Z`; checkpoint-24 read-only/local-proof refresh through `2026-07-19T18:16:35Z` |
| Mode | `standard` |
| In scope | Rust authority/autonomy and MCP surfaces; HTTP session security; GROBID endpoint policy; surgical verification; managed persistence; transport and runner bounds; Node updater; release workflow/scripts/contracts; current-tree and Git-history secret scanning; cached dependency scan |
| Out of scope | Hosted GitHub jobs/settings, deployed processes, live external services, public registries, publication mutation, Windows/Linux runtime proof, physical Touch ID/hardware-key proof, destructive exploit execution, archives nested inside repository files |
| Execution/network policy | Source review and focused local tests; external Cargo target on `/Volumes/Cofre`; loopback/local fixtures only; no publication, tag, push, registry upload, or remote state mutation; scanners used local/cached data |
| External evidence | None. Collaborating local agents supplied focused test attestations; no hosting/control-plane evidence was available |
| Limitations | The tree remains mutable and untracked content is not frozen; Semgrep rules were not run; Trivy DB was cached and past `NextUpdate`; Python 3.14 lacks pytest; scanner or aggregate success is not proof of absence |
| Assessor/client | Initial Codex assessor plus checkpoint-24 Codex refresh using the3y3 evidence model; m1nd orientation refused cross-root authority, so direct source/test truth was used |

The status and diff digests identify a point in the changing worktree but do not content-hash untracked files. The final deliverable must be regenerated against a clean or otherwise immutable candidate before release.

## 3. Repository and system truth

### System shape

M1ND is a Rust workspace with control, ingestion, MCP/HTTP, and runner-daemon components, plus a Node-based CLI/updater, React/TypeScript UI surfaces, Python/Shell release tooling, and GitHub Actions publication automation. The refreshed repository probe saw 8,139 files and 184,982,060 bytes without hitting its 200,000-file cap; build/dependency/cache directories were intentionally not traversed by that probe. The primary security boundaries in this assessment are local browser-to-loopback transport, caller-to-MCP authority, repository-to-owner execution, caller-to-filesystem writes, ingest-to-network endpoints, MCP-to-runner ownership, and source-to-public-release promotion.

### Assets and trust boundaries

| Asset or boundary | Why it matters | Observed control | Evidence | Gap/unknown |
|---|---|---|---|---|
| Authority and autonomy state | Unauthorized promotion or mission mutation changes sovereign behavior | Generic routes admit only `Ordinary`; elevated scopes require typed consumers; denial is state-invariant | E-006 | Typed public promotion consumer is intentionally unavailable; full end-to-end autonomy remains gated |
| Local HTTP bearer session | Session theft permits MCP calls as the local owner | CSPRNG one-shot nonce, strict host/origin/fetch checks, strict cookie | E-003 | OS same-UID isolation is not provided by loopback HTTP |
| Repository execution boundary | A hostile repository can execute through build/test hooks | `apply_batch` never runs dynamic repo verification; reports `NOT_RUN`/`RISKY` | E-005 | Downstream humans/agents must preserve the refusal and not reinterpret `NOT_RUN` as success |
| Managed persistence | Caller-selected paths could overwrite arbitrary owner files | No caller path field; managed root, canonical-parent and symlink/non-regular checks; Unix root descriptor/device/inode pinning rejects deterministic replacement | E-007, C-024 | Same-UID micro-TOCTOU/cross-process races and directory-fsync crash durability not proven |
| GROBID/network egress | Hostile endpoints or redirects can create SSRF | Parsed allowlist; userinfo/query/fragment rejection; loopback policy; redirects disabled; 30-second timeout | E-004 | Live DNS rebinding and external TLS endpoint behavior not exercised |
| Runner ownership and secret | Runner impersonation or registry exhaustion can seize work | Random `0600` secret; non-symlink/format checks; TTL and registry/response caps | E-010 | OS-level peer identity beyond the shared secret not proven |
| Transport capacity | Session/SSE floods can exhaust local memory/file descriptors | 256 sessions, 30-minute idle TTL, 2 SSE/session, 64 global SSE, bounded headers | E-009 | Valid expensive calls and open SSE deletion semantics remain bounded only by adjacent controls |
| Release candidate bytes | Substitution can ship unreviewed code | Commit/tag checks, build-once artifacts, candidate digests and exact-byte consumers | E-011 | Hosted execution, environment protection, and real registry acceptance not observed |
| Update channel | Tool-path or release redirection can execute attacker code | Fixed canonical tool paths, permission checks, HTTPS restrictions, fail-closed npm update lock | E-011 | Production npm transaction/rollback is incomplete; Windows trusted npm path is refused |

### Entry points and sensitive sinks

| Entry/source | Attacker/control | Sensitive sink/effect | Review status |
|---|---|---|---|
| Browser bootstrap query, Host, Origin, Fetch-Site | Browser content, DNS, local process | Session cookie and MCP bearer authority | Source-traced; 6 focused negative tests attested PASS |
| Generic `m1nd.*`/`m1nd_*` tool invocation | MCP caller | Authority, mission, ratification, promotion, UI action | Source-traced; 35 focused Rust/UI cases attested PASS |
| `apply_batch` patch and verification request | Repository/caller | Owner-side process execution | Source-traced; hostile `build.rs` regression attested PASS |
| Universal ingest GROBID endpoint | Environment/configuration | Outbound HTTP(S) and document disclosure | Source-traced; 2 endpoint-policy cases attested PASS |
| Persistence/authoring request | MCP caller | Owner filesystem | Source-traced; negative path/symlink tests and current workspace aggregate PASS; native power-loss/race proof remains open |
| HTTP initialize, custom security headers, SSE | Local client | Session allocation, memory, open streams | Source-traced; 29 focused transport tests PASS |
| Runner announce/secret/config | Local process/filesystem actor | Runner identity, dispatch, response memory | Source-traced; 40 focused tests PASS |
| Tag, workflow artifact, npm/crate candidate | Maintainer/CI/supply-chain input | Public package/release bytes | Source-traced and local contracts passed before final concurrent edits; hosted run `NOT_RUN` |

The root runner also reported the full `m1nd-mcp` library at 1,080 PASS, 0 FAIL, 6 intentional ignores in 86.96 seconds; medulla integration at 1 PASS, 0 FAIL, 6 ignores; focused authority groups at 28 + 4 + 2 PASS; and `attach_reinit` at 3/3 PASS. Five library ignores are obsolete generic REST mutation-success flows pending a typed G2 consumer, and the old `attach_self_echo` mutation-success case remains ignored with 0 failures. These are honest availability gates: public generic elevated writes are intentionally unavailable and are not production-ready. Scoped Clippy was clean after isolating four authority-file warnings; full strict Clippy was not globally proven at this snapshot.

Checkpoint 24 supersedes only those aggregate counts and review boundaries: `cargo test --locked --workspace -- --test-threads=4` passed against the refreshed dirty tree. The `m1nd-mcp` library passed 1,399 with 0 failures and 15 intentional ignores; all executed external MCP integration cases passed and `attach_self_echo` retained one explicit future-G2 ignore; RETROBUILDER real passed 5/5 and stress passed 17/17. Workspace check, all-target Clippy with `-D warnings`, fmt, diff-check, and `cargo build --locked --release --workspace` passed, with the release build completing in 3m38s. The focused authority group is 29/29 including deterministic root symlink/rename/recreate and second-owner refusal. G6 runner/scorer is 85/85, the Rust verifier 8/8, and `m1nd-control` 149/149; the final independent corrective re-review returned `APPROVE`/none. This closes the current-tree local aggregate and corrective review only; it does not convert historical or ignored availability gaps into positive authority proof, run the formal blind score, or freeze a candidate.

The final-local UI/canon receipt added `m1nd-ui` at 647/647 PASS in 2.17 seconds, PATHOS autorefresh at 2/2 PASS, `cargo fmt --all --check` PASS, and unchanged frozen PRD/UML hashes. A separate browser E2E lane passed 31/31 in 46.7 seconds against its own Vite server with mocked `/api`. This closes the local browser harness, not a live owner, hardware, deployed, hosted, or publication proof.

The UI lint receipt is also green: ESLint exited 0 with zero errors and five existing `react-refresh` warnings; `violet-lint` and `icon-lint` both passed. Warnings remain recorded rather than silently reclassified as errors or removed evidence.

## 4. Top attack paths

1. Malicious browser or rebinding page -> loopback bootstrap -> session cookie -> MCP authority. Host/origin/fetch validation plus a one-shot random nonce blocks the reviewed path; a malicious same-UID local process remains outside the browser-only isolation claim (E-003, H-001).
2. Hostile repository -> `apply_batch` -> `cargo test`/`build.rs` -> owner code execution. Owner-side dynamic execution is removed; the request records `NOT_RUN` and becomes `RISKY` (E-005).
3. MCP caller -> generic alias/casing variant -> elevated authority action -> autonomy/mission mutation. Normalization and the generic eligibility floor reject elevated scopes, require typed consumers, and preserve state digest on denial (E-006).
4. Untrusted GROBID endpoint -> redirect/userinfo/host confusion -> internal service or metadata endpoint. URL policy, exact allowed host rules, loopback restrictions, disabled redirects, and redacted diagnostics interrupt the path (E-004).
5. Local client flood -> initialize/SSE -> memory or descriptor exhaustion. Admission is reserved before session state and capped; SSE has per-session/global caps and releases on drop. Valid expensive calls and already-open SSE after DELETE need additional adversarial evidence (E-009, H-005).
6. Maintainer/CI compromise or configuration drift -> release workflow -> substitute UI/npm/crate bytes -> public consumers. Local contracts bind exact candidate bytes, but hosted jobs, protected environments, and registry acceptance are unobserved, so publication remains blocked (E-011, H-002).

## 5. Findings

No `CONFIRMED` or `LIKELY` vulnerability met the finding threshold in this bounded assessment. An empty finding set means “none confirmed in scope,” not “none exist.”

### Dismissed scanner leads

| Lead | Disposition | Evidence |
|---|---|---|
| 23 `generic-api-key` matches | `FALSE_POSITIVE`, high confidence. All reviewed locators are idempotency keys or `provider_key_version` values in code/tests/canonical fixtures; none is a service credential | E-012 |
| 2 `private-key` matches | `FALSE_POSITIVE`, high confidence. Both are Rust dependency `.rmeta` build outputs under `target/debug/deps` for cryptographic crates, not source or a usable repository key | E-012 |
| Git history scan | Zero findings across `--all`; exit 0 | E-013 |

## 6. Unresolved hypotheses

| ID | Severity if true | Confidence | Hypothesis | Evidence | Decision at risk | Why unresolved | Smallest next evidence |
|---|---|---|---|---|---|---|---|
| H-001 | HIGH | LOW | A malicious same-UID process may still win a micro-TOCTOU/cross-process filesystem race or impersonate a loopback peer despite deterministic root-identity checks | E-003, E-007, C-024 | Claim that local owner boundaries resist hostile co-tenants | Deterministic root symlink/rename/recreate replacement now fails closed, but no repeated cross-process race harness or OS peer-credential isolation was proven | Repeated cross-process replacement race harness plus OS peer-credential design verdict |
| H-002 | HIGH | LOW | Hosted workflow permissions, environment rules, or action behavior may diverge from local release contracts | E-011 | Public release authorization | No GitHub control-plane or hosted execution evidence | Run the exact immutable candidate through protected hosted dry-run and capture attestations/settings |
| H-003 | MEDIUM | MEDIUM | Power loss between rename and directory persistence, or a cross-store partial commit, may leave durable state inconsistent | E-007 | Crash consistency and autonomous recovery | Parent-directory fsync and cross-store physical atomicity were not proven | Fault-injection/power-loss harness with durable post-restart invariants |
| H-004 | HIGH | LOW | Linux/Windows path semantics or physical-presence providers may violate macOS-local assumptions | E-016 | Cross-platform and sovereign ratification claims | Only the local macOS path was observed | Linux/Windows matrix plus physical Touch ID/hardware-key negative and positive receipts |
| H-005 | MEDIUM | MEDIUM | Valid expensive calls or long-lived SSE bodies may consume resources within admission caps | E-009 | Local availability under an authenticated abusive client | Session/SSE counts were proven; cost-per-valid-call and forced close after DELETE were not | Saturation test with CPU/RSS/FD thresholds and DELETE/disconnect lifecycle assertions |

## 7. Control and coverage truth

| Domain | Applicable | Critical | Depth | Verdict | Confidence | Evidence | Main gap/next evidence |
|---|---|---:|---:|---|---|---|---|
| Architecture and threat model | Multi-boundary agentic system | Yes | 2 | PARTIAL | HIGH | E-002, E-003–E-011 | Deployed topology and same-UID threat model not frozen |
| Authentication and sessions | Local HTTP bearer session and runner secret | Yes | 3 | PARTIAL | HIGH | E-003, E-010 | OS peer identity and live replay/race proof |
| Authorization and tenant isolation | Generic/typed authority split | Yes | 3 | PARTIAL | HIGH | E-006 | Typed positive-sovereign public consumer not installed; end-to-end autonomy remains gated |
| Input handling and injection | URLs, headers, paths, release inputs | Yes | 3 | PARTIAL | HIGH | E-003, E-004, E-007, E-011 | No broad SAST and no live external endpoint proof |
| Business logic, state, concurrency, and abuse | Mission/autonomy, transactions, caps | Yes | 3 | PARTIAL | MEDIUM | E-006, E-009 | Physical cross-store atomicity and valid-call abuse |
| Data protection and cryptography | Nonces, secrets, receipts, package hashes | Yes | 2 | PARTIAL | MEDIUM | Hardware-backed/physical provider and key lifecycle not exercised |
| Secrets and credentials | Source/history/build output and runner secret | Yes | 3 | PARTIAL | HIGH | Archives and deployed secret stores not scanned |
| Dependencies and components | Cargo/npm lockfiles | Yes | 2 | PARTIAL | MEDIUM | Cached stale DB, three lockfile targets only, no SBOM/VEX reachability proof |
| CI/CD and release integrity | GitHub/npm/crates/UI promotion | Yes | 3 | PARTIAL | HIGH | Hosted jobs/settings and real publication `NOT_RUN` |
| Infrastructure and deployment | Local daemon plus hosted release | Yes | 2 | UNKNOWN | LOW | E-015 | No deployed/runtime or control-plane evidence |
| Detection, recovery, vulnerability management | Rollback, WAL, scanner evidence | Yes | 2 | PARTIAL | MEDIUM | No live rollback/power-loss drill or operational alert evidence |
| Repository governance | Branch/tag/environment controls | Yes | 2 | UNKNOWN | LOW | E-015 | Remote branch protection and CODEOWNERS/environments not observed |
| Testing and security assurance | Focused negative suites, local contracts, and current-tree aggregate | Yes | 3 | PARTIAL | HIGH | E-003–E-014 plus C-020–C-022 | Immutable-candidate replay, Semgrep rules, cross-platform and hosted proof pending |
| Agentic AI/MCP | Tools, authority, delegation, owner execution | Yes | 3 | PARTIAL | HIGH | E-005, E-006, E-009 | Live multi-agent adversarial scenario and OS isolation pending |

## 8. Supply-chain and release truth

The release workflow locally enforces tag/HEAD/SHA alignment, exact `origin/main`, a UI artifact built once and verified by consumers, an npm tarball packed once, candidate digests, and re-verification before release. The final local crate receipt proves one Cargo 1.95.0 multi-package overlay invocation produced four `.crate` candidates: core `8f7501…bf70` (245,970 bytes), control `1540e8…9908` (143,892), ingest `fb7424…a5ce` (198,918), and MCP `2fb012…6d8a` (1,806,301). The MCP package binds UI hash `8e4405…b5c` across 24 files for version 0.1.0. Post-hardening, the unique M1ND-10 discover suite passed 90/90 in 42.066 seconds; actionlint, shellcheck, py_compile, targeted diff-check, frozen-canon SHA-256, and exact extracted-crate preflight all passed. The structural harness honestly labels itself `STRUCTURALLY_VALID_NOT_CRYPTOGRAPHICALLY_VERIFIED`. Local crate packaging is `PASS`; hosted GitHub/crates.io upload remains `NOT_RUN/NOT_PROVEN` (E-018, E-022).

The updater uses fixed canonical executables, rejects symlink/non-regular/world-writable tools, applies HTTPS/effective-URL constraints, rejects ambient production test overrides, and refuses legacy Cargo fallback journals. Automatic npm update remains deliberately locked behind `npm-signed-artifact-required`; this is secure fail-closed behavior but an availability/completeness gap. Windows trusted npm execution is intentionally refused. A real signed update/rollback and real registry publication were not run.

Trivy 0.71.2 reported zero vulnerabilities, secrets, or misconfigurations for `Cargo.lock`, `m1nd-demo/package-lock.json`, and `m1nd-ui/package-lock.json`. Its cached vulnerability DB was updated `2026-07-16T01:02:16Z` and past its declared next-update time, so the check is `PARTIAL`, not a clean bill of health. No SBOM/VEX reachability proof, lifecycle-hook audit across every fixture, or hosted provenance verification was completed.

## 9. Checks and evidence

| Check | Tool/version | Command/scope | Status | Exit/duration | Evidence/limitations |
|---|---|---|---|---|---|
| C-001 | the3y3 repo probe 1.0 | Read-only, network-free census of the repository root | PASS | 0 / duration not retained | E-002; ignored dependency/build/cache trees |
| C-002 | gitleaks 8.30.1 | Candidate-only worktree projection, `detect --no-git --redact=100` | PASS | 0 / <2s | 25.76 MB scanned, zero findings. Thirteen synthetic fixture leads were individually annotated; no broad rule was disabled |
| C-003 | gitleaks 8.30.1 | `git --all --redact=100` history | PASS | 0 / duration not retained | E-013; zero findings; nested archives not unpacked |
| C-004 | Trivy 0.71.2 | Filesystem vuln/secret/misconfiguration JSON artifact | PARTIAL | 0 / duration not retained | E-014; zero results in 3 lockfiles, cached DB stale |
| C-005 | Cargo | `mcp_http::tests::`, `--features serve` | PASS | 0 / duration not retained | E-009; 29 passed |
| C-006 | Cargo | runner owner, naming runner, curation runner, runner config focused suites | PASS | 0 / duration not retained | E-010; 8 + 12 + 9 + 11 passed |
| C-007 | Cargo/Vitest | HTTP, GROBID, `apply_batch`, authority/mission/promotion/UI negative suites | PASS | 0 / duration not retained | E-003–E-006; 6 + 2 + 1 + 35 passed |
| C-008 | npm / Node 22.22.3 | updater `npm test` | PASS | 0 / 14.86s | E-011; 1/1 harness pass, live signed update not run |
| C-009 | Python/Node/actionlint/shellcheck | local release contracts before final concurrent edits | PARTIAL | 0 / duration not retained | E-011; Python 30 and Node 2 passed; final crate lane and hosted jobs pending |
| C-010 | Semgrep 1.167.0 | Repository SAST rules | NOT_RUN | — | No approved rule set was configured or executed; scanner availability is not a pass |
| C-011 | GitHub hosted release | Protected candidate publication and settings | NOT_RUN | — | No hosting/control-plane evidence; E-015 |
| C-012 | Cross-platform/physical | Linux, Windows, Touch ID, hardware key | NOT_RUN | — | Local macOS-only evidence; E-016 |
| C-013 | Full aggregate suite | Current dirty-tree all-workspace regression | PASS | 0 / aggregate durations retained by component | 1,399 `m1nd-mcp` library PASS, all executed external integrations PASS, RETROBUILDER 5 + 17 PASS; not a frozen-candidate receipt |
| C-014 | Cargo 1.95.0 | Earlier full `m1nd-mcp` lib, medulla integration, focused authority/attach | PASS | 0 / 86.96s full-lib lane | Historical E-017 baseline; superseded for aggregate counts and strict Clippy by C-013/C-020, while typed G2 success flows remain intentionally unavailable |
| C-015 | Cargo 1.95.0 | Package-once four-crate overlay and sealed UI | PASS | 0 / duration not retained | E-018; four artifact hashes/sizes and sealed UI binding recorded; public upload is a separate NOT_RUN boundary |
| C-016 | npm/Node/Cargo | UI, PATHOS autorefresh, formatting, frozen-canon hashes | PASS | 0 / 2.17s UI lane | E-019; UI 647/647, autorefresh 2/2, fmt PASS, PRD/UML hashes unchanged; browser covered separately by C-017, hosted not proven |
| C-017 | npm browser harness | `npm run test:e2e` | PASS | 0 / 46.7s | E-020; 31/31 on local Vite with mocked `/api`; no live owner/hardware/deployed API |
| C-018 | ESLint/project linters | `lint`, `violet-lint`, `icon-lint` | PASS | 0 / duration not retained | E-021; 0 errors, 5 existing react-refresh warnings; violet/icon PASS |
| C-019 | Python/actionlint/shellcheck | Final M1ND-10 hardening and extracted-crate preflight | PASS | 0 / 42.066s discover lane | E-022; 90/90 unique tests and all local hardening checks PASS; structural, not cryptographic; hosted upload NOT_RUN |
| C-020 | Cargo | Workspace check, all-target Clippy `-D warnings`, fmt, release build | PASS | 0 / release build 3m38s | Current dirty tree only; no signing, packaging, publication, install, or activation |
| C-021 | Python 3.14 / unittest / Ruff | Repository 174, benchmark 60, Windows contract 4; exact G6 Ruff scope | PASS | 0 / repository 40.5s | Pytest unavailable/`NOT_RUN`; broad non-canonical Ruff reports 99 legacy violations outside the scoped clean files |
| C-022 | Node/npm/actionlint | UI unit 646, live-contract 8, build, lint, workflow syntax | PASS | 0 / durations partly retained | ESLint 0 errors/5 warnings; real isolated owner/browser/h4nd LIVE and hosted workflows `NOT_RUN` |
| C-023 | Python/Rust/Fugu | G6 corrective scorer, verifier, control, and independent re-review | PASS | 0 / test durations partly retained | 60 runner + 25 scorer, 8 verifier, 149 control PASS; Fugu `APPROVE`/high/none. Corrective readiness only; formal 220-task blind run `NOT_RUN` |
| C-024 | Cargo / Unix authority runtime | Root descriptor/device/inode binding and deterministic replacement refusal | PASS | 0 / focused duration not retained | Authority focus 29/29; root symlink, live rename/recreate, and in-process second-owner replacement fail closed. Micro-race, hostile cross-process same-UID replacement, TCP peer identity, Windows, and hardware anti-rollback remain unproven |
| C-025 | Python candidate source guard + Fugu review | Exact Git-tree and non-mutating worktree-projection source boundary | CHANGE | 0 / review duration not retained | Original 1,410-path projection passed, but independent review reproduced case, credential/key, archive and public-content bypasses. Candidate freeze is blocked pending remediation and re-review |
| C-026 | Python/actionlint/Gitleaks workflow contract | Mandatory pinned source and secret gates in CI/release | PARTIAL | 0 / local durations retained | 18 original tests and actionlint pass; Action v2.3.9/full SHA and scanner 8.30.1 are pinned. Semantic content-gate coverage and hosted execution remain open |
| C-027 | Python/Cargo candidate-public regression | Public projection tests plus touched Rust libraries | PASS | 0 / Rust 425.55s plus compile | Python 142 repository + 60 benchmark PASS; `m1nd-control` 134/134 and `m1nd-mcp` 1,399 PASS/15 ignored. Dirty projection only, not an immutable-candidate receipt |

## 10. Unknowns, not tested, and assumptions

| Priority | Area | Risk decision affected | Reason | Smallest next evidence |
|---|---|---|---|---|
| High | Hosted release and governance | Public release | GitHub permissions, protected environments, branch/tag rules and hosted behavior were not observed | Captured protected hosted dry-run plus control-plane settings |
| High | Immutable candidate | Final closure | The future path/content projection is clean, but it remains uncommitted and therefore has no immutable tree identity | Authorized candidate commit/tree digest, exact-commit guard + Gitleaks, regenerated ledger, repeated full aggregate suite |
| High | OS/platform/physical authority | Full autonomy and sovereign ratification | macOS local tests cannot prove Linux/Windows or real physical-presence providers | Platform matrix and hardware-backed receipts |
| Medium | Same-UID race/peer identity | Local isolation | Root identity is pinned and deterministic replacement refuses, but per-operation micro-races and loopback checks still do not isolate hostile processes running as the same user | Repeated cross-process race harness plus OS peer credentials or an explicit same-UID non-goal |
| Medium | Durable transactions | Recovery and atomicity | Directory fsync, power loss, and cross-store physical commit were not proven | Fault injection and restart invariant proof |
| Medium | Resource abuse | Availability | Count caps pass, but valid expensive calls and SSE close timing lack measured saturation evidence | CPU/RSS/FD bounded adversarial load test |
| Medium | Dependency/SAST reachability | Supply-chain risk | Cached Trivy DB was stale; Semgrep/SBOM/VEX not run | Fresh offline snapshot, approved rules, SBOM and reachable-vulnerability triage |
| Low | Broad Python lint debt | Maintainability and future scanner signal | Exact G6 files are Ruff-clean, but a broad non-canonical scan reports 99 legacy violations in older tests/scripts | Triage on an immutable candidate without mixing unrelated cleanup into security fixes |
| Low | Archives/deployed secret stores | Credential exposure | Gitleaks covered current files and Git history but did not recursively unpack archives or inspect deployment stores | Archive-aware scan and read-only deployed-store inventory |

## 11. Prioritized roadmap

| Horizon | ID | Action | Risk/uncertainty reduced | Existing capability to reuse | Owner | Effort | Verification |
|---|---|---|---|---|---|---|---|
| Now | R-001 | Freeze one immutable candidate, repeat the now-green aggregate suite, and regenerate this ledger | H-002 and evidence drift | Checkpoint-23 current-tree aggregate, focused suites and ledger schema | Release/security owner | M | Clean candidate identity plus all required suites PASS with retained commands/times |
| Now | R-002 | Exercise the exact candidate in a protected hosted dry-run without public promotion | H-002, CI/release unknowns | Existing candidate sealing and local contract tests | CI/release owner | M | Hosted attestations bind tag, SHA, UI, npm and crate bytes; environment rules captured |
| Next | R-003 | Extend the deterministic root-replacement proof into repeated cross-process same-UID micro-races and a loopback peer-identity verdict | H-001 | Root descriptor/device/inode pinning, managed path validators, and HTTP gate | Platform/security owner | M | Race harness cannot replace the live root or impersonate a session under the declared threat model; OS peer-credential requirement is explicit |
| Next | R-004 | Prove crash durability and cross-store recovery with fault injection | H-003 | Existing temp-file, WAL and recovery machinery | Persistence owner | L | Power-loss matrix restores one consistent state with durable receipts |
| Next | R-005 | Complete the signed npm update transaction/rollback and rehearse exact bytes | Update availability/completeness | Existing fixed-tool, URL, candidate and rollback guards | Updater/release owner | M | Signed local candidate installs exact bytes, rollback restores prior version, tampering refuses |
| Next | R-006 | Run Linux/Windows and physical-presence authority lanes | H-004 | Existing canonical vectors and focused authority suites | Platform/authority owner | L | Cross-platform negative/positive receipts and real Touch ID/hardware-key proof |
| Next | R-007 | Measure valid-call and SSE saturation, including DELETE/disconnect lifecycle | H-005 | Existing session/SSE caps and timeout controls | Transport owner | M | CPU/RSS/FD thresholds remain bounded and all slots release deterministically |
| Later | R-008 | Add an approved targeted SAST ruleset and SBOM/VEX reachability lane | Dependency/SAST unknowns | Existing CI contract harness and Trivy artifacts | Security/CI owner | M | Rules, versions, exclusions, fresh data and triage are retained in the ledger |

## 12. Boundary statement

> This is the repository's evidenced security truth for the recorded revision, scope, environment, and time—not a certification or a guarantee of absence of vulnerabilities.
