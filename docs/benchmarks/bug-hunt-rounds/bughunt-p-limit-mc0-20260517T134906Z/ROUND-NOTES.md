# Bug Hunt Round Notes: bughunt-p-limit-mc0-20260517T134906Z

Status: internal product learning, not public benchmark copy.

## Result

- `direct`: 10/10 seeded bugs found (100.0%); per-lane counts `[5, 5]`.
- `m1nd-mission-control`: 10/10 seeded bugs found (100.0%); per-lane counts `[5, 5]`.
  Mission Control: loop-complete lanes `0/2`, unavailable lanes `2`, median `mission_next` count `0.0`, median direct-proof switches `1.5`, median adherence `0.0`.
- `m1nd-short-audit`: 10/10 seeded bugs found (100.0%); per-lane counts `[5, 5]`.
- `m1nd-trained`: 10/10 seeded bugs found (100.0%); per-lane counts `[5, 5]`.

## Mission Control Validity

- Evaluable lanes: `0/2`.
- Partial or unavailable lanes: `['audit-03', 'audit-04']`.
- Missing result lanes: `[]`.
- MC0 recall is fallback evidence only until completed Mission Control lanes are evaluable.

## Interpretation

Read this as an internal product-learning artifact, not a public scoreboard. The useful comparison is between instruction modes that received the same seeded repo and the same answer key.

The strongest recurring signal is not simply "m1nd on" versus "m1nd off". It is whether the agent has a compact, correct operating loop: trust check, scoped recovery, graph orientation, direct source/test proof, and honest fallback when retrieval is blocked.

If a Tempo/TEMPONIZER mode is present, interpret it as prompt-integration evidence too. Temporal recalibration should reduce inherited human-duration bias and improve decision quality, but an over-heavy checklist can add enough cognitive overhead to reduce bug recall.

## Caveats

- This is one internal round on one fixture repo.
- Extra findings were preserved but not independently judged.
- This report measures seeded recall, not total bug discovery quality.

## Next Product Actions

- Fix worker-host Mission Control exposure before rerunning MC0 comparisons.
- Keep improving the compact trained-agent loop as a default universal agent pack behavior.
- Add cleaner state placement so m1nd benchmark/probe flows do not write sidecar metadata into target repos.
- Track first-good-finding time and tool-call counts in the event stream.
- Add a judge pass for extra findings so future reports can separate true extras from noise.
