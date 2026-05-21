# Bug Hunt Round Notes: bughunt-click-mc1-20260518T000000Z

Status: internal product learning, not public benchmark copy.

## Result

- `direct`: 2/2 seeded bugs found (100.0%); per-lane counts `[1, 1]`.
  Timing/actions: median first-good finding `71.434`s, median observed actions `39.5`, median shell commands `27.0`, median file reads `8.5`, median tests/probes `3.5`.
- `m1nd-mission-control`: 2/2 seeded bugs found (100.0%); per-lane counts `[1, 1]`.
  Timing/actions: median first-good finding `216.5`s, median observed actions `46.5`, median shell commands `11.5`, median file reads `10.5`, median tests/probes `2.5`.
  Mission Control: loop-complete lanes `2/2`, unavailable lanes `0`, median `mission_next` count `5.5`, median direct-proof switches `1.5`, median coverage sweeps `1.0`, median adherence `1.0`.

## Mission Control Validity

- Evaluable lanes: `2/2`.
- Partial or unavailable lanes: `[]`.
- Missing result lanes: `[]`.

## Interpretation

Read this as an internal product-learning artifact, not a public scoreboard. The useful comparison is between instruction modes that received the same seeded repo and the same answer key.

The strongest recurring signal is not simply "m1nd on" versus "m1nd off". It is whether the agent has a compact, correct operating loop: trust check, scoped recovery, graph orientation, direct source/test proof, and honest fallback when retrieval is blocked.

If a Tempo/TEMPONIZER mode is present, interpret it as prompt-integration evidence too. Temporal recalibration should reduce inherited human-duration bias and improve decision quality, but an over-heavy checklist can add enough cognitive overhead to reduce bug recall.

## Caveats

- This is one internal round on one fixture repo.
- Extra findings were preserved but not independently judged.
- This report measures seeded recall, not total bug discovery quality.

## Next Product Actions

- Repeat the Mission Control direct-sweep calibration on another fixture before treating it as a generalized improvement.
- Keep improving the compact trained-agent loop as a default universal agent pack behavior.
- Add cleaner state placement so m1nd benchmark/probe flows do not write sidecar metadata into target repos.
- Use first-good-finding time, observed action counts, and source-backed finding counts as standard internal benchmark dimensions.
- Add a judge pass for extra findings so future reports can separate true extras from noise.
