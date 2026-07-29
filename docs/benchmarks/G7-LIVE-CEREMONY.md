# G7 LIVE — the owner's ceremony

> Staged 2026-07-29 against `main` `b96b191f` (m1nd `1.5.0`). Everything below is
> machine-checked except the two gestures that are the owner's by definition: the
> release build and the run itself. Nothing here starts, contacts, or mutates the
> installed owner on `:1338`.

G7 in the PRD (`docs/M1ND-10-PRD.md` §9, "Human Product Coherence") is five lines. This
ceremony closes exactly **one and a half** of them — the m1nd Human View half — and it is
written so the owner can see which. The rest is named in §7 as work, not ceremony.

---

## 1. What the gate proves

`scripts/m1nd10_g7_live_orchestrator.py` boots **one throwaway owner** from an exact binary
and drives a **real Chromium** against it. Its own boundary declaration (`proof_boundary` in
the receipt) is the honest summary:

**Proves on PASS**

- the exact supplied binary and a clean source identity (commit, tree, `--porcelain` empty);
- the UI harness installed **offline** from the closed SHA-512 lock;
- an explicit Playwright/Chromium revision and bundle digest;
- an isolated read-only owner on numeric loopback, **never** port 1338;
- a real browser shell reading the real owner API, with the served UI digest matched against
  the digest the binary was built to embed.

**Never touches**

- the installed M1ND service, ambient port discovery, or process discovery;
- network dependency installation or the checkout's own `node_modules`;
- mock routes, HARs, or a Playwright `webServer`.

The LIVE spec (`m1nd-ui/e2e-live/live-owner.spec.ts`) contains **zero** `page.route`
interceptors. It asserts the socket each response actually came from
(`response.serverAddr()` → the configured loopback address **and** port), refuses service
workers, and refuses any request method outside `GET`/`HEAD`/`OPTIONS` plus three
explicitly read-only POST paths. That is what "browser LIVE real, no API interception"
means here, and it is already implemented.

---

## 2. Preconditions — measured, not assumed

| Precondition | State on `b96b191f` | If it is missing |
|---|---|---|
| POSIX host (process groups) | ✓ macOS/Linux; the gate **refuses** on Windows | run it on the Mac |
| Clean worktree at the source root | you must check | `git status --porcelain` must print nothing |
| `m1nd-ui/dist` present and tracked | ✓ 24 files, tracked on purpose (rust-embed) | see §6 — do **not** rebuild casually |
| `graph_snapshot.json` at the source root | ✗ **absent in a fresh worktree** (gitignored) | see step 0 below |
| Playwright Chromium installed, with both `INSTALLATION_COMPLETE` and `DEPENDENCIES_VALIDATED` markers | ✓ `chromium-1234`, 339 files, markers present | `cd m1nd-ui && npx playwright install chromium` |
| A release binary built against the exact dist digest | you build it (step 1) | the gate refuses on a digest mismatch |

The gate reads `graph_snapshot.json` from the source root and copies it into ephemeral
state; the source copy is only ever read. Any real snapshot serves — the primary checkout's
own (~4 MB) is the obvious seed.

---

## 3. The ceremony — four commands

Run from the ceremony worktree; `$ROOT` is its root.

```bash
cd <ceremony-worktree> && ROOT="$PWD"
export CARGO_TARGET_DIR="$(bash "$ROOT/scripts/cargo_target_dir.sh")"
```

**Step 0 — give the owner a graph to serve.**

```bash
cp <primary-checkout>/graph_snapshot.json "$ROOT/graph_snapshot.json"
```

It is gitignored, so the worktree stays clean. Confirm with `git status --porcelain`
(silence is the pass).

**Step 1 — the production build, refused unless the UI is the promoted one.**

```bash
cd "$ROOT/m1nd-ui" && npm ci
UI=$(python3 "$ROOT/scripts/m1nd10_g7_live_expectations.py" --source-root "$ROOT" \
     | python3 -c 'import json,sys; print(json.load(sys.stdin)["ui_bundle_sha256"])')
cd "$ROOT" && M1ND_RELEASE_UI_REQUIRED=1 M1ND_EXPECTED_UI_BUNDLE_SHA256="$UI" \
  cargo build --release -p m1nd-mcp
```

`m1nd-mcp/build.rs` panics rather than produce a binary if the dist is absent, a
placeholder, or a different digest than the one you named. That refusal is the point: the
binary cannot be built ignorant of what it embeds.

**Step 2 — read the three digests the gate demands up front.**

```bash
python3 "$ROOT/scripts/m1nd10_g7_live_expectations.py" \
  --source-root "$ROOT" \
  --binary "$CARGO_TARGET_DIR/release/m1nd-mcp"
```

It prints the UI bundle digest, the Chromium bundle digest, the binary digest, whether the
source is clean, the receipt path, and the exact orchestrator command with all of them
filled in. It starts nothing and writes nothing. On `b96b191f` the first two are already
known:

- UI bundle (the tracked `m1nd-ui/dist`, 24 files) —
  `7b7904c84fabe630a986a104af4a17aa7743b1798bf2ebcc4adef91efd9b6dc8`
- Chromium bundle (`chromium-1234`, 339 files) —
  `5f6cb737f03ea05c71070c888ea34a89c6aa04706f9775db913ffdcdd105a07d`

**Step 3 — run the gate.** Paste the command step 2 printed. It looks like:

```bash
python3 scripts/m1nd10_g7_live_orchestrator.py \
  --binary "$CARGO_TARGET_DIR/release/m1nd-mcp" \
  --expected-binary-sha256 <from step 2> \
  --source-root "$ROOT" \
  --expected-ui-bundle-sha256 <from step 2> \
  --expected-browser-bundle-sha256 <from step 2> \
  --output <absolute path OUTSIDE $ROOT>
```

Two constraints worth knowing before you retype it by hand: `--output` must be **absolute**
(`input_validation/relative_output`) and must sit **outside the source root**
(`input_validation/receipt_inside_source_refused` — "the receipt must be outside the source
root so proof does not dirty its subject"). Step 2 defaults it to a sibling of the worktree
for exactly that reason. Optional: `--port <n>` to pin a port (1338 is refused by name);
omit it and the kernel picks one.

---

## 4. What the owner will see

The gate is quiet — no browser window is handed to you; Chromium runs headless under the
orchestrator's own process group, and the whole run is a couple of minutes. What comes back
is one line and one file:

```
G7 LIVE PROVEN; receipt=docs/proofs/m1nd10-g7-live-receipt.json
```

or, on refusal:

```
G7 LIVE orchestrator refused [<stage>/<code>]: <detail>
```

The refusal codes are the map. The ones you are most likely to meet first:

| Stage / code | What it means |
|---|---|
| `source_validation/source_dirty` | the worktree is not an immutable candidate |
| `source_validation/source_ui_digest_mismatch` | `m1nd-ui/dist` is not the bundle you named |
| `binary_validation/binary_digest_mismatch` | the binary is not the one you named |
| `browser_identity/browser_bundle_digest_mismatch` | Chromium moved since step 2 |
| `dependency_preparation/npm_ci_failed` | the offline lock replay failed |
| `browser_gate/gate_failed` | the browser reached the owner and the **product** was incoherent |

Only the last one is a G7 finding. The others are the gate protecting the claim.

The receipt is token-free by construction: a digest monitor watches the gate's stdout and
refuses with `browser_gate/token_output_leak` if the bearer ever appears in it.

---

## 5. Acceptance criteria, per PRD line

| PRD G7 line | Closed by this ceremony? | The exact assertion |
|---|---|---|
| Human View consumes manifest/authority state | **Yes** | the shell must render `[data-role="manifest-status"]` at `data-manifest-state="ready"`, and the same surface must show `COHERENT` and `SRC/BIN/BND x/y/z · ALIGNED` |
| h4nd consumes manifest/authority state | **No** — different product, different repo | see §7 |
| production build + runtime attestation of the promoted bundle digest; mismatch → refuse/DRIFT | **Yes, for the m1nd shell** | `ui.bundle_sha256` must equal the digest you named; `authorities.ui_bundle.digest` must equal it too; anything else is `DRIFT` by `m1nd-mcp/src/ui_attestation.rs` |
| UI unit · accessibility · browser fixture · browser LIVE as **separate** proofs | **3 of 4** | unit + fixture are green in CI (§6); LIVE is this gate; **accessibility does not exist** on the m1nd side |
| poold policy explicit; each ratified lane passes a non-synthetic E2E | **Not touched** | measured separately — see §7 |
| landing, stale, degraded, drift, recovery visible | **Partially** | the LIVE spec asserts the coherent/aligned path only; the degraded paths are proven in the fixture lane, not against a real owner |

A green receipt therefore reads: *the m1nd Human View, served by a production binary from an
immutable source, is coherent under a real browser against a real owner.* It does not read
"G7 passes".

---

## 6. The one thing to know before rebuilding the UI

`m1nd-ui/dist` is **tracked on purpose** (`.gitignore:21` — rust-embed compiles it into the
binary). Two consequences, both measured on `b96b191f`:

1. `npm run build` is **not** byte-reproducible against the committed dist. A local rebuild
   emits different content-hashed chunk names for `dagre`, `highlighter` and `index`, plus a
   changed `index.html` — four tracked paths modified, three new untracked. That instantly
   fails `source_validation/source_dirty`.
2. The reason is that the committed dist is stale: it was last built at `70598733`
   (2026-07-20), while `m1nd-ui/src` has moved twice since (latest `4cb99b3e`, 2026-07-25)
   and `m1nd-ui/package-lock.json` three times (latest `f6385036`, a nine-package UI
   dependency bump on 2026-07-25).

So the shell this ceremony attests is internally coherent — binary, manifest and served
bytes all agree — while being **five commits behind the UI source**. The manifest cannot see
this, because every authority in it points at the same stale bundle. Refreshing the dist is
a normal PR (rebuild, commit the dist, let CI go green), not part of the ceremony; run the
ceremony against whatever `main` says at the moment you want the claim.

---

## 7. What is still NOT_RUN after a green receipt

- **The h4nd half.** h4nd is a separate product outside this repo. It has already shipped a
  shell runtime attestation of its own (`h4nd-shell-runtime-attestation-v1`, with
  `COHERENT`/`DRIFT`/`DEGRADED`/`UNATTESTED` and a refuse window) and a production serve
  mode — the "Express/Vite serves dirty source" line in `docs/M1ND-10-UML.md` is stale. What
  it does **not** have is a LIVE lane: every one of its browser specs installs a fixture
  interceptor, and its "real server" spec drives a disposable fixture server, not a real
  owner. Its `e2e/accessibility.spec.ts`, by contrast, is real and reported separately — the
  m1nd side has no equivalent.
- **Accessibility, m1nd side.** Zero role/name assertions in the fixture browser suite and
  no axe dependency. The PRD asks for a separate proof; there is none to run.
- **poold execution.** The policy *is* explicit and fail-closed:
  `h4nd-pool/pool-policy-v1.json`, `policy_id: m1nd10-g7-observe-only-v1`, all five lanes
  (`warm_fast`, `warm_smart`, `cold_prepare`, `cold_spawn`, `advisory_judge`)
  `enabled: false, proof: null`. Read literally, the PRD line is satisfied by an
  observe-only policy — every unproven lane is disabled. Read as intent, zero lanes have a
  non-synthetic claim→handoff→spawn→ACK→result→transition E2E, so zero lanes are usable.
  Which reading governs is an owner call, not an agent's.
- **The degraded surfaces against a real owner.** Landing, stale, degraded, drift and
  recovery are proven in the fixture lane; the LIVE lane only walks the coherent path.

---

## 8. Where the receipt goes

The gate writes it outside the tree (see step 3). Copying it into `docs/proofs/` beside the
other M1ND-10 proofs is a **separate, deliberate commit** after the run — never during it,
or the proof dirties its own subject and the next run refuses.

Its `proof_boundary` block travels with it, so no later reader can inflate what it proved.
If the run refuses, keep the refusal — a named refusal is a better artifact than a retry
that happened to work.
