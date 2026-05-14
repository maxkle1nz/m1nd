# Bug Hunt Round Notes: bughunt-p-limit-compact-20260514T161105Z

Status: internal product learning, not public benchmark copy.

## Result

- `direct`: 7/10 seeded bugs found (70.0%); per-lane counts `[3, 4]`.
- `m1nd-temponizer-compact`: 10/10 seeded bugs found (100.0%); per-lane counts `[5, 5]`.
- `m1nd-trained`: 8/10 seeded bugs found (80.0%); per-lane counts `[4, 4]`.

## Interpretation

Read this as an internal product-learning artifact, not a public scoreboard. The useful comparison is between instruction modes that received the same seeded repo and the same answer key.

This round is the follow-up to the heavier `m1nd-temponizer-full` run. The
compact form performed better than both controls in this fixture: both compact
lanes found all five seeded defects.

The product lesson is sharper now: Temponizer works best here when it is
compact agent operating physics. The formula and time recalibration helped
without adding a reporting tax. The previous full-spec prompt likely hurt recall
because it made agents over-document and over-filter.

Operationally, the new `--no-worktree-artifacts` instruction reduced the
graph/plasticity sidecar problem, but the round exposed one remaining native
runtime leak: `ingest_roots.json` still persisted in three m1nd lane workspaces.
That was fixed in source after the round by making ingest roots persist next to
the graph snapshot, matching the existing load path.

## Product Observations

- `m1nd-temponizer-compact`: both lanes found all five seeded defects.
- `m1nd-trained`: both lanes found four defects, but missed different edge
  cases.
- `direct`: controls stayed strong, but missed more boundary/edge behavior.
- Timing fields are now present in the report, but event streams still need
  stricter finding-event conventions before timing can become headline evidence.
- No public claim should be made from this single fixture, but the compact
  condition is the best next experimental default.

## Caveats

- This is one internal round on one fixture repo.
- Extra findings were preserved but not independently judged.
- This report measures seeded recall, not total bug discovery quality.

## Next Product Actions

- Keep improving the compact trained-agent loop as a default universal agent pack behavior.
- Rebuild/restart the managed native runtime before the next agent benchmark so
  the ingest-root persistence fix is active outside `target/debug`.
- Track first-good-finding time and tool-call counts in the event stream.
- Add stricter event types for `finding_recorded` so first-good-finding timing
  can be compared cleanly.
- Add a judge pass for extra findings so future reports can separate true extras from noise.
