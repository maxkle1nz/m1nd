# M1ND-10 G6 supplemental generalization-v2 corpus proof — 2026-07-18

## Verdict

`PASS` for a sealed, deterministic **supplemental generalization guard** built from the fixed diagnostic snapshot.

This corpus contains 120 blind public queries: 100 localizable positives and 20 plausible unlocalizable negatives, balanced at 25 positive plus 5 negative tasks for each of four logical repositories. It is intentionally independent of the formal R2 gate and carries `formal_r2_claim: NOT_APPLICABLE` in every runner-facing or sealed control artifact.

It does **not** prove retrieval quality. M1ND runtime execution, the retrieval runner, and the scorer are all `NOT_RUN`.

## Proof boundary

| Claim | Status | Evidence |
| --- | --- | --- |
| Corpus has exactly 120 tasks | `PASS` | 100 positive plus 20 negative; checked by generator and tests |
| Per-repo balance is exact | `PASS` | 30 tasks per repo: 25 positive plus 5 negative |
| Positive anchors exist in the pinned source | `PASS` | all 100 declaration anchors, file digests, line numbers, and excerpt digests revalidated |
| Negative signatures are absent | `PASS` | 40 exact signatures rescanned across every source/dependency-manifest file in each logical repo |
| Public task projection is label-blind | `PASS` | public tasks contain exactly six identity/query fields; labels, anchors, evidence, and proofs remain sealed |
| Near-duplicate lexical guard | `PASS` | highest pairwise normalized `SequenceMatcher` ratio is 0.534, below the enforced 0.70 ceiling |
| Checked-in artifacts regenerate byte-for-byte | `PASS` | deterministic build comparison in the 11-test invariant suite |
| Author semantic adjudication | `PASS` | linked author second-pass review receipt covers all 120 tasks |
| Second independent semantic adjudicator | `NOT_RUN` | explicitly recorded in the linked review receipt |
| Cross-v1 semantic overlap check | `NOT_RUN` | would require reading v1 task content outside the requested seal |
| Zero semantic exposure to every v1 example | `NOT_PROVEN` | the allowed schema/corpus implementation file interleaves schema logic with embedded examples; no v1 artifact was opened, but this stronger claim cannot honestly be made |
| Formal R2 minimum-200 gate | `NOT_APPLICABLE` | this 120-task suite is supplemental and cannot satisfy or replace R2 |
| Retrieval runtime or scorer result | `NOT_RUN` | prohibited during corpus construction and validation |

## Seal and independence

Authoring used only:

- `/tmp/m1nd-g6-diag-snapshot-20260718` as the source truth;
- schema/corpus implementation for format compatibility;
- local Python artifact generation and invariant tests.

The v1 public task artifact, v1 operator-only labels, v1 results, v1 reports, retrieval runner, and scorer were not opened or executed. The builder has no dependency on those paths or modules, performs no subprocess or network work, and records `m1nd_usage_mode: direct_snapshot_only`.

The allowed schema/corpus implementation itself contains embedded example definitions. Therefore, this proof establishes independence within the authorized file boundary, not an impossible stronger claim of complete semantic non-exposure. The corpus does not claim or compute cross-v1 uniqueness.

## Corpus coverage

| Logical repo | Language | Positive | Negative | Positive source files | Files scanned for negatives | Source lines |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| `m1nd-mcp` | Rust | 25 | 5 | 14 | 104 | 136,663 |
| `m1nd-core` | Rust | 25 | 5 | 14 | 57 | 28,034 |
| `m1nd-python-tools` | Python | 25 | 5 | 10 | 11 | 6,890 |
| `m1nd-ui` | TypeScript | 25 | 5 | 12 | 185 | 34,585 |
| **Total** | — | **100** | **20** | **50 repo-qualified locations** | **357** | **206,172** |

The positive queries cover source ingestion, graph and state transitions, persistence, ranking, policy/truth rendering, UI interaction, and operational tooling. The negative set covers plausible but absent external integrations. Each negative binds at least two exact implementation signatures to the complete per-repo searched-file manifest.

Queries are behavioral paraphrases and expose neither source paths nor accepted symbol names. Their initial forms are deliberately varied: 25 `how`, 18 `what`, 37 `where`, and 40 `which` queries. Task order is a deterministic hash-based blind order, not grouping by label or repo.

The checked-in test suite also rejects pairwise normalized query similarity at or above 0.70. This is a mechanical near-copy guard, not a claim that lexical distance alone proves semantic independence.

## Artifact topology

- `docs/benchmarks/m1nd10-g6-generalization-v2/public/queries.json` — the only runner-readable corpus surface; identity and query fields only.
- `docs/benchmarks/m1nd10-g6-generalization-v2/operator-only/corpus.json` — sealed labels, accepted anchors, source evidence, and negative proofs.
- `docs/benchmarks/m1nd10-g6-generalization-v2/operator-only/review.json` — linked author review receipt and explicit `NOT_RUN` gates.
- `docs/benchmarks/m1nd10-g6-generalization-v2/manifest/source-manifest.json` — complete per-repo file inventory, hashes, revisions, and snapshot binding.
- `docs/benchmarks/m1nd10-g6-generalization-v2/manifest/digests.json` — exact checked-in artifact-byte hashes and the binding chain.
- `scripts/benchmark/m1nd10_g6_generalization_v2_corpus.py` — source-only deterministic builder and validator.
- `tests/test_m1nd10_g6_generalization_v2_corpus.py` — invariants, blinding, determinism, tamper detection, and forbidden-dependency checks.

The public file embeds the source manifest so a future runner can resolve the exact snapshot without reading operator-only labels. The sealed corpus binds to the review digest and label-set digest. The digest manifest then hashes the exact JSON bytes for all four primary artifacts.

## Cryptographic receipts

- Snapshot digest: `sha256:c7ef8c5fb854257606d4252ea470bc3eea4301c81d31681623921ad7fe9d413a`
- Source-manifest digest: `sha256:c6ce4203ed22db76f06e884d181c56111982107e506da1e98d1150530ce4d420`
- Corpus ID: `m1nd10-g6-generalization-v2-53550cbe446b31e8`
- Public corpus digest: `sha256:53550cbe446b31e87f513d7d8af4c205dbd4305eb6a0e8f503b7d330ac1d64b7`
- Sealed label-set digest: `sha256:d4835918aeceef3309d3b2941fd612defb2bfabe51216da641993fccd6fe8fc8`
- Review digest: `sha256:4b26c603cd5c849fac41371dfa763f9fc627298cfed3efcc8eea9f90497fc084`
- Public exact-file digest: `sha256:164ac4178cee9ec1f6465c7ca9362907608eb0414b634bd97cea1acdac445642`
- Sealed corpus exact-file digest: `sha256:b5714a40753878dd4e9309ad23708fdb145237a884c683cd9240bc1a3cdd418a`
- Review exact-file digest: `sha256:3c0144d9b55f6b4990d84d5087e0ecd8ec89e863e098434ea8adb557b54390b6`
- Source-manifest exact-file digest: `sha256:f576e85ebe44e9becee051641b186b072f7d12d6824c709f07d16278fec44af3`

## Reproduction and local proof

Only source/artifact validation was run:

```text
python3 -m py_compile scripts/benchmark/m1nd10_g6_generalization_v2_corpus.py
python3 scripts/benchmark/m1nd10_g6_generalization_v2_corpus.py generate
python3 scripts/benchmark/m1nd10_g6_generalization_v2_corpus.py validate
python3 -m py_compile tests/test_m1nd10_g6_generalization_v2_corpus.py
python3 -m unittest tests.test_m1nd10_g6_generalization_v2_corpus
```

Observed result:

```text
generator/validator: PASS, errors=[]
task_count: 120
localizable_count: 100
unlocalizable_count: 20
by_repo: m1nd-core=30, m1nd-mcp=30, m1nd-python-tools=30, m1nd-ui=30
runtime: NOT_RUN
scorer: NOT_RUN

Ran 11 tests
OK
```

During corpus construction, no Rust code, UI code, runtime behavior, scorer, v1 corpus, v1 report,
or v1 result was changed. The supplemental scorer below was added afterward without reading or
executing the v1 label/result artifacts.

## Supplemental scorer readiness

`scripts/benchmark/m1nd10_g6_generalization_score.py` now provides a separate fail-closed
current-only scorer for this 120-task guard. It enforces exact 4-repo 25-positive/5-negative
strata, complete blind-run coverage, zero runner errors/actions/error-fallbacks, finite executed
latencies, the ratified G6 recall/abstention/wrong-action/latency thresholds, and per-repo metrics.
Its report always carries `supplemental_only: true` and
`formal_r2_effect: NOT_APPLICABLE`; it cannot be mistaken for the formal 200-task receipt or a
baseline non-inferiority result.

```text
python3 -m py_compile \
  scripts/benchmark/m1nd10_g6_generalization_score.py \
  tests/test_m1nd10_g6_generalization_score.py
python3 -m unittest tests.test_m1nd10_g6_generalization_score -v
```

Result: **6 passed, 0 failed**. Negative fixtures cover an unratified spec, incomplete coverage,
error-fallback measurements, wrong-ground action, and corpus-stratum drift. The runtime and actual scoring
remain `NOT_RUN` until the final candidate binary is sealed.

## Residual risks and non-claims

- Behavioral relevance of the 100 positive labels has author review but no second independent adjudicator; independent semantic correctness remains `NOT_PROVEN`.
- Exact-signature negative scans prove the named integrations are absent under those signatures; they do not prove absence of every semantically equivalent implementation.
- The four logical repositories are subtrees of one fixed diagnostic snapshot, not independent organizations or production checkouts.
- The snapshot lives under `/tmp`; the checked-in manifests and digests preserve its identity, but regeneration requires that exact pinned snapshot to remain available.
- This suite is a supplemental regression/generalization guard only. Any later runtime measurement must consume only the public artifact and must publish its own separate result receipt.
