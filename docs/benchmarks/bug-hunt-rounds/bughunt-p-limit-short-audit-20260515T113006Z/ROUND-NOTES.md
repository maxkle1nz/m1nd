# Bug Hunt Round Notes: bughunt-p-limit-short-audit-20260515T113006Z

Status: internal product learning, not public benchmark copy.

## Result

- `direct`: 9/10 seeded bugs found (90.0%); per-lane counts `[4, 5]`.
- `m1nd-short-audit`: 9/10 seeded bugs found (90.0%); per-lane counts `[4, 5]`.

## Interpretation

Read this as an internal product-learning artifact, not a public scoreboard. The useful comparison is between instruction modes that received the same seeded repo and the same answer key.

The strongest recurring signal is not simply "m1nd on" versus "m1nd off". It is whether the agent has a compact, correct operating loop: trust check, scoped recovery, graph orientation, direct source/test proof, and honest fallback when retrieval is blocked.

If a Tempo/TEMPONIZER mode is present, interpret it as prompt-integration evidence too. Temporal recalibration should reduce inherited human-duration bias and improve decision quality, but an over-heavy checklist can add enough cognitive overhead to reduce bug recall.

This short-audit round partially confirmed the design. `m1nd-short-audit` tied
`direct` on seeded recall, and both arms found `9/10` seeded bugs. It did not
close the time gap: `direct` had `117.5s` median wall-clock and `65s` median
first-finding time, while `m1nd-short-audit` had `281.5s` median wall-clock and
`135.5s` median first-finding time.

The useful signal is that short-audit recovered recall compared with the prior
confirmation round's m1nd arms, but still carried cold-graph and helper-call
overhead. Both short-audit lanes used the intended pattern: one bounded m1nd
orientation pass, then direct source/runtime proof. One lane recorded that an
`audit` call was parameter-noisy and stopped m1nd after `seek`, which is exactly
the behavior the route was meant to encourage.

The next product target is therefore not more prompt text. It is a lower-friction
short-audit helper that bundles trust, ingest-if-needed, and one cheap
orientation query into a single stable call, then returns an explicit
`switch_to_direct_proof` envelope.

## Caveats

- This is one internal round on one fixture repo.
- Extra findings were preserved but not independently judged.
- This report measures seeded recall, not total bug discovery quality.

## Next Product Actions

- Build a dedicated short-audit helper/CLI flow so agents do not have to compose
  trust, ingest, and orientation by hand in tiny repos.
- Make the helper emit a clear `switch_to_direct_proof` recommendation when
  suspect files are visible or recovery overhead exceeds the short-audit budget.
- Track first-good-finding time and tool-call counts in the event stream.
- Keep using direct controls on small fixtures; short-audit should win by
  preventing missed edge cases without doubling wall-clock time.
