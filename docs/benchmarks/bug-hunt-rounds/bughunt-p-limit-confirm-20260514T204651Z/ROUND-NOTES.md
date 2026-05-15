# Bug Hunt Round Notes: bughunt-p-limit-confirm-20260514T204651Z

Status: internal product learning, not public benchmark copy.

## Result

- `direct`: 10/10 seeded bugs found (100.0%); per-lane counts `[5, 5]`.
- `m1nd-temponizer-compact`: 8/10 seeded bugs found (80.0%); per-lane counts `[5, 3]`.
- `m1nd-trained`: 9/10 seeded bugs found (90.0%); per-lane counts `[5, 4]`.

## Interpretation

Read this as an internal product-learning artifact, not a public scoreboard. The useful comparison is between instruction modes that received the same seeded repo and the same answer key.

The strongest recurring signal is not simply "m1nd on" versus "m1nd off". It is whether the agent has a compact, correct operating loop: trust check, scoped recovery, graph orientation, direct source/test proof, and honest fallback when retrieval is blocked.

If a Tempo/TEMPONIZER mode is present, interpret it as prompt-integration evidence too. Temporal recalibration should reduce inherited human-duration bias and improve decision quality, but an over-heavy checklist can add enough cognitive overhead to reduce bug recall.

This confirmation round is a useful counterweight to the previous p-limit round. On this small, highly localized seeded fixture, `direct` was the strongest arm: `100%` recall with a `119.208s` median wall-clock and `50.5s` median first-finding time. The m1nd arms still found most seeded bugs, but their recovery and orientation overhead mattered more than graph context on this task.

The product lesson is not "m1nd is worse". It is sharper: m1nd must make the short-task path cheaper. Agents repeatedly hit `needs_ingest` because benchmark probes use fresh temporary runtimes, then had to learn to bundle ingest and retrieval in one same-process call. That friction is now a concrete improvement target for the agent pack, helper scripts, or a reusable benchmark/runtime session mode.

## Caveats

- This is one internal round on one fixture repo.
- Extra findings were preserved but not independently judged.
- This report measures seeded recall, not total bug discovery quality.

## Next Product Actions

- Add a same-session benchmark probe mode so `trust_selftest -> ingest -> search/seek/activate` can run without surprising fresh-runtime resets.
- Teach the agent pack a short-audit lane: when the repo is tiny and the bug surface is localized, spend a small fixed budget on m1nd orientation, then move quickly to direct probes.
- Track first-good-finding time and tool-call counts in the event stream.
- Add a judge pass for extra findings so future reports can separate true extras from noise.
