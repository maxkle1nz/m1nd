# Bug Hunt Round Notes: bughunt-humanize-20260514T021500Z

Status: internal product learning, not public benchmark copy.

## Result

- `direct`: 8/15 seeded bugs found (53.3%); per-lane counts `[3, 2, 3]`.
- `m1nd-basic`: 8/15 seeded bugs found (53.3%); per-lane counts `[2, 3, 3]`.
- `m1nd-trained`: 16/20 seeded bugs found (80.0%); per-lane counts `[3, 5, 4, 4]`.

## Interpretation

The strongest signal is not simply "m1nd on" versus "m1nd off". The strongest signal is that the `m1nd-trained` lanes, which received a compact operating card for using m1nd correctly, found substantially more seeded defects than both `m1nd-basic` and direct lanes in this round.

That is exactly the agent-first product lesson: m1nd needs excellent doctrine, recovery guidance, workspace clarity, and simple first-minute commands. The graph alone is not the whole product; the agent operating system around it is part of the result.

## Caveats

- This is one internal round on one fixture repo.
- Extra findings were preserved but not independently judged.
- The earlier `itsdangerous` attempt was invalidated because the signing-library domain triggered a subagent safety filter.
- This report measures seeded recall, not total bug discovery quality.

## Next Product Actions

- Turn the `m1nd-trained` command card into the default universal agent pack behavior.
- Add a repeatable bug-hunt init flow so seeded repo audits are not hand-assembled.
- Track first-good-finding time and tool-call counts in the event stream.
- Add a judge pass for extra findings so future reports can separate true extras from noise.
