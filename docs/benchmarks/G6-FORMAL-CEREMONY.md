# M1ND-10 G6 — the formal blind run, as one owner command

This is the owner-facing description of the G6 Knowledge Quality ceremony: what
you type, what you will see, how long it takes, what PASS and FAIL mean, what is
committed afterwards, and what you must not do.

G6 today is `COMPONENT_PASS`. The runner, the scorer and the corpus contract are
`LOCAL_PROVEN`; the formal blind run itself is `NOT_RUN`. Nothing in this
document changes that. Running the ceremony is the only thing that can, and only
the owner can run it.

## 1. The command

```bash
scripts/benchmark/g6_formal_run.sh \
  --metric-spec               <ratified metric spec v2> \
  --sealed-corpus             <operator-only/corpus.json> \
  --sealed-corpus-self-digest sha256:<the sealed corpus self_digest> \
  --authority-assembly        <pinned production authority assembly> \
  --authority-assembly-digest <its 64-hex self digest> \
  --authority-provider        <authority provider executable> \
  --binary                    <pinned candidate m1nd-mcp binary> \
  --baseline-binary           <pinned baseline m1nd-mcp binary> \
  --baseline                  <previously sealed baseline result> \
  --baseline-receipt          <outcome-blind baseline-ratification receipt> \
  --run-ledger                <sealed-run ledger>
```

Every path is owner-held. The script holds none of them, and it never reads a
label: the sealed corpus is hashed and handed to the scorer, never parsed here.

To see how far the machinery gets without any of that:

```bash
scripts/benchmark/g6_formal_run.sh --dry-run
```

To check readiness without running anything at all:

```bash
scripts/benchmark/g6_formal_preflight.sh [same owner-held flags]
```

## 2. What the script does, in order

1. **Preflight** (`g6_formal_preflight.sh`). Verifies every public artifact
   against the pinned digests in `manifest/digests.json`, recomputes that
   manifest's own self digest, replays the frozen-contract byte pins, lints the
   frozen contracts for world state, re-validates the corpus through the
   runner's own `validate_public_queries`, confirms the corpus commit and all
   357 manifest files are present in the object store, and reports each
   owner-held input as present-and-valid or missing. Exit `0` READY, `3`
   READY_PUBLIC_ONLY, `1` FAIL. A formal run demands `0`.
2. **Run identity.** Refuses a dirty working tree (a dirty tree already produced
   one discarded run: `m1nd10-g6-failed-b59-dirty-af02c141.json`). Derives
   `run_id` and `system_revision` from `HEAD`.
3. **Isolated source snapshot.** Materialises the exact manifest file set from
   immutable Git objects at the corpus commit into a fresh `0700` directory
   outside every Git worktree, then verifies it through the runner's own
   `verify_public_source_snapshot`: byte, size and line count per file, no
   symlink, no extra file, no missing file.
4. **Blind run.** Invokes `m1nd10_g6_blind_runner.py` for the `current` lane:
   four owners (one per corpus repository), governed graph ingest with every
   authority receipt verified offline by the pinned candidate binary itself, a
   post-ingest re-proof that the source did not move, warm-up, then one `north`
   and one `seek` per task for all 220 tasks.
5. **Score.** Invokes `m1nd10_g6_retrieval.py` against the sealed corpus, the
   ratified baseline, the baseline-ratification receipt and the sealed-run
   ledger, then prints every metric next to its ratified threshold and writes
   the report and the receipt.

## 3. What you will see

Steps 1-3 print a check table, the run identity, and the snapshot verification
line (`checked=357 missing=0 mismatched=0 extra=0`). Step 4 prints runner
progress. Step 5 prints the verdict as measured-versus-ratified pairs:

```
measured vs ratified
  top-5 anchor recall      measured=0.905  >= ratified=0.9    PASS
  abstention recall        measured=0.95   >= ratified=0.95   PASS
  wrong-ground act rate    measured=0.0    <= ratified=0.01   PASS
  north p95 (ms)           measured=1085.8 <= ratified=2000   PASS
  seek p95 (ms)            measured=236.8  <= ratified=500    PASS
  paired regression        p=1.4e-24  alpha=0.05  improvements=102 regressions=5

VERDICT: PASS (claimable=true)
```

*(Those numbers are the 2026-07-18 held-out-v1 run, shown only so the shape of
the output is recognisable. They are not a prediction.)*

## 4. How long it takes

| Phase | Cost | Basis |
|---|---|---|
| Preflight | ~2 s | measured: hashes 6 public artifacts and walks one tree |
| Snapshot materialise + verify | ~11 s | measured: 357 files, 8,186,532 bytes, verified twice |
| Whole `--dry-run` | 13.7 s | measured end to end on this machine |
| Owner boot + 4 governed ingests | 10-25 min | **estimate**, not measured: 206,361 source lines across four owners |
| 220 measured tasks | 5-8 min | derived from the prior run's p95s (north 1086 ms + seek 237 ms per task) |
| Scoring | seconds | measured: the scorer is pure JSON evaluation |

Budget **20-40 minutes** end to end and do not interrupt it. Only the first two
rows have been measured; the rest is arithmetic on the previous era's numbers
and will be replaced by a real figure once the ceremony has run once.

## 5. What PASS and FAIL mean

**PASS** (`status: PASS`, `claimable: true`) means every ratified threshold was
met on complete evidence: top-5 anchor recall on localizable tasks, abstention
recall on unlocalizable tasks, the wrong-ground `act` rate, the `north` and
`seek` p95 SLOs, and no statistically significant paired regression against the
ratified baseline. That is the evidence G6 has been missing. It does not by
itself close G6 cumulatively — the receipt still has to be accepted by the
release authority alongside the other gates.

**FAIL** means a measured threshold was missed. The numbers are real; preserve
them.

**NOT_PROVEN** means the evidence was incomplete — a missing task, a duplicate
measurement, an unresolved SLO, an unratified spec, a non-finite latency, an
absent baseline. It is neither a pass nor a fail, and it is the honest outcome
whenever the machinery cannot see enough to judge.

## 6. What gets committed afterwards

The script writes to `docs/benchmarks/m1nd10-<run_id>/`:

| Artifact | Visibility | Commit? |
|---|---|---|
| `report.json` | public | yes — the scorer's verdict |
| `receipt.json` | public | yes — digests binding spec, runner, scorer, both binaries, the raw result and the report to the verdict |
| `runner-results/` | operator-only | never — already gitignored |

`.gitignore` carries explicit allow-lines for `report.json` and `receipt.json`
under `m1nd10-g6-formal-*/`, so the two public artifacts survive the repository's
blanket `*.json` rule while the raw measurements stay out. That is the
boundary-era law: public formal artifacts are never gitignored, operator-only
always is.

`receipt.json` is **not** a `GateReceiptV1`. That structure
(`m1nd-control::release`) binds a ratified custody floor and is minted by the
release authority. This receipt is the evidence the release authority consumes.

Also update `docs/PATHOS.md` in the same session: G6 moves off `NOT_RUN`, and the
checkpoint records the verdict with its numbers.

## 7. What NOT to do

- **Do not re-run until it goes green.** The metric spec is explicit:
  `same_revision_rerun_policy: "one_sealed_run_only_no_rerun_until_pass"`, and
  `new_run_requires: "new system_revision or binary_digest; preserve prior
  FAIL/NOT_PROVEN evidence"`. One sealed run per revision. A new run needs a new
  revision or a new binary, and the old FAIL stays in the record — the repository
  already keeps two of them on purpose (`m1nd10-g6-failed-*.json`).
- **Do not run from a dirty tree.** The script refuses; do not work around it.
- **Do not open the labels.** No agent, implementer or reviewer reads
  `operator-only/`. The runner and the authority provider are sandboxed away
  from those paths precisely so the blindness is structural, not promised.
- **Do not weaken a fail-closed check to get a verdict.** `NOT_PROVEN` is a
  legitimate outcome and is worth more than a manufactured `PASS`.
- **Do not run against the served owner or port 1338.** The ceremony spawns its
  own owners on kernel-assigned ports with private registries.

## 8. What is still missing before this can run

The preflight names these every time it runs. As of this writing none of them
exist in the repository, and none of them can be produced by an agent:

1. **A ratified metric spec v2.** The checked-in
   `m1nd10-g6-metric-spec-v1.json` is v1. Both the runner
   (`validate_metric_spec_for_runner`) and the scorer (`_validate_spec`) require
   schema `m1nd10-g6-metric-spec-v2` with a calibration gate, an outcome-blind
   ratification, and an authority receipt digest. The v1 thresholds carry over;
   the artifact has to be re-minted under authority.
2. **A pinned production authority assembly and its provider executable.** The
   runner refuses formal mode without one: *"formal run requires a pinned
   production authority assembly"*. This is the G9 custody floor — Path B,
   amendment G9-A1, ratified 2026-07-21 in
   `docs/M1ND-10-G9-CUSTODY-DECISION-20260721.md`, implementation not started.
3. **One frozen immutable candidate binary**, for the current lane and for the
   baseline lane.
4. **The sealed held-out corpus**, matching the pinned digest
   `sha256:5abe6f7d…` in `m1nd10-g6-held-out-v2/manifest/digests.json`.
5. **A ratified baseline run, its outcome-blind ratification receipt, and the
   sealed-run ledger.**

Item 2 is the frontier; it gates G6-formal, G7-complete and G8-signing together.
Everything else in this ceremony is staged, verified and waiting.

## 9. Related documents

- `docs/benchmarks/M1ND10_G6_RETRIEVAL_PROTOCOL.md` — the gate's measurement contract
- `docs/proofs/m1nd10-g6-knowledge-quality.md` — the G6 proof receipt
- `docs/M1ND-10-HANDOFF-20260719.md` §7 — why the run is blocked
- `docs/M1ND-10-G9-CUSTODY-DECISION-20260721.md` — the custody decision it waits on
