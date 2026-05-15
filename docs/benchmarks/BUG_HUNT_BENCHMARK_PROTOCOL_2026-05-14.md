# Bug-Hunt Benchmark Protocol

Date: 2026-05-14
Status: internal protocol, not a result claim

## Purpose

Bug-hunt rounds answer a narrower and sharper question than broad real-world
agent rounds:

> When agents audit the same seeded repo, does m1nd plus the trained operating
> loop help them find more hidden behavioral defects?

The accepted `humanize` round showed a clear internal signal:

- `m1nd-trained`: `16/20`, `80.0%`
- `m1nd-basic`: `8/15`, `53.3%`
- `direct`: `8/15`, `53.3%`

That result is not public benchmark copy. It is product evidence that the agent
pack is part of m1nd.

The p-limit confirmation round showed the opposite pressure on a tiny,
localized fixture:

- `direct`: `10/10`, `100.0%`, median wall-clock `119.208s`
- `m1nd-trained`: `9/10`, `90.0%`, median wall-clock `176.5s`
- `m1nd-temponizer-compact`: `8/10`, `80.0%`, median wall-clock `192.5s`

The first short-audit round tested the resulting route:

- `direct`: `9/10`, `90.0%`, median wall-clock `117.5s`
- `m1nd-short-audit`: `9/10`, `90.0%`, median wall-clock `281.5s`

Treat this as a design signal, not a contradiction to hide. m1nd's advantage
should grow when structural context, continuity, docs/code binding, impact
analysis, or multi-file reasoning matter. On small local bugs, the agent pack
needs a short-audit route: make one scoped trust/ingest/orientation pass, then
move quickly to direct source reads and runtime probes.

The short-audit route preserved recall parity on the p-limit fixture but did
not yet recover direct-mode speed. The next benchmark/product target is a
dedicated short-audit helper that performs trust, ingest-if-needed, and one
cheap orientation query in one stable call, then tells the agent when to switch
to direct proof.

## Instruction Modes

`m1nd-full-spec` means the agent receives the full m1nd operating layer:

```text
skills/m1nd-operator/references/full-spec-agent-os.md
```

This condition tests whether an agent performs better when it can route through
the entire m1nd/L1GHT system: architecture maps, docs drift, multi-repo
federation, perspectives/trails, locks, monitoring, deep risk tools, and
surgical change prep. It should be interpreted separately from
`m1nd-trained`; full spec is a route table for broad/hard situations, not a
checklist for every narrow recall task.

`m1nd-temponizer-full` means the agent receives the shipped trained-agent loop
plus the explicit Temponizer formula:

```text
Tc = alpha(phi) * Tp
phi in {GEN, IO, DBG, PAR}
```

The agent should use corrected agent time (`Tc`) instead of inherited human
duration guesses, then record measured `Te` around major decisions. This mode is
currently experimental: use it to test prompt integration, not to claim
Temponizer value by itself.

`m1nd-temponizer-compact` means the agent receives the trained-agent loop plus
the full Temponizer formula in compact operating form: calculate `Tc` around
major branch decisions, record `Te` where it matters, and keep the audit moving.
This is the preferred next experimental condition.

`m1nd-temponizer` is the earlier lighter Tempo mode: phase/time awareness and
`Te` notes without the full formula. Current evidence suggests compact or light
forms may preserve recall better than the full-spec checklist while still
improving telemetry.

`m1nd-short-audit` means the agent uses m1nd as a bounded orientation pass, not
as the main investigation engine. The agent establishes trust, performs at most
one recovery/ingest sequence and one or two cheap orientation calls, then moves
to direct source reads, git diff, focused runtime probes, tests, or compiler
output. This mode exists because the p-limit confirmation round showed that
tiny localized tasks can be hurt by m1nd recovery/orientation overhead.

`m1nd-trained` means the agent receives the shipped trained-agent loop:

1. trust check scoped to the repo
2. recovery before interpreting absence
3. precise `wrong_workspace_binding` handling
4. `audit`, `search`, `seek`, and `activate` for orientation
5. runtime envelope reading
6. direct source/test/compiler proof
7. `impact`, `validate_plan`, and `surgical_context_v2` when needed
8. evidence logging

`m1nd-basic` means m1nd is available, but the full operating card is not given.

`direct` means no m1nd tools or helper scripts.

Future reports must keep these modes separate. "m1nd installed" is not the same
experimental condition as "m1nd trained", and "Temponizer formula present" is
not the same condition as "Temponizer integrated ergonomically".

For small bug-hunt fixtures, agents should record whether m1nd was used as:

- `short_audit_orientation`: bounded trust/ingest/orientation, then direct proof.
- `deep_structural_investigation`: graph navigation remains central after the first map.
- `recovery_overhead`: m1nd state repair consumed meaningful time before findings.

This distinction prevents a tiny-repo result from being overread as a global
m1nd quality score.

## Create A Round

```bash
python3 scripts/benchmark/bug_hunt_round.py init \
  --out-dir docs/benchmarks/bug-hunt-rounds/round-001 \
  --round-id round-001 \
  --repo your-fixture-repo \
  --source-repo .m1nd-benchmark-fixtures/bug-hunt/your-fixture-source \
  --seeded-repo .m1nd-benchmark-fixtures/bug-hunt/round-001/your-fixture-seeded \
  --seeded-bug-count 5 \
  --json
```

The command writes:

- `round.json`
- `operator-only/answer-key.json`
- `event-streams/*.jsonl`
- `lane-prompts/*.md`
- `lane-results/*.json`
- `lane-result-template.json`

It does not plant bugs, clone repos, or prepare workspaces. The operator must
prepare the seeded repo and answer key before dispatching agents.

## Score A Round

```bash
python3 scripts/benchmark/bug_hunt_round.py score \
  --round-file docs/benchmarks/bug-hunt-rounds/round-001/round.json \
  --answer-key docs/benchmarks/bug-hunt-rounds/round-001/operator-only/answer-key.json \
  --lane-results-dir docs/benchmarks/bug-hunt-rounds/round-001/lane-results \
  --output docs/benchmarks/bug-hunt-rounds/round-001/report.json \
  --notes docs/benchmarks/bug-hunt-rounds/round-001/ROUND-NOTES.md \
  --json
```

The scorer reports seeded recall by mode. Extra findings remain
`extra_unadjudicated_findings_count`; they are not precision penalties until a
separate judge validates them.

## Next Fixture Queue

Recommended next rounds:

- `click-python-cli`: CLI parser, type conversion, option defaults, docs/tests
  drift.
- `p-limit-node`: concurrency accounting, queue clearing, abort/reject edge
  cases, TypeScript public API boundaries.
- `human-panic-rust-cli`: metadata formatting, panic path configuration,
  filesystem/report output, feature flag boundaries.

Avoid security-token/signing-library repos for this benchmark unless the
operator has checked that the task will not trigger safety filters.

## Claim Boundary

Allowed internal claims:

- seeded recall by round and instruction mode
- whether all lanes produced event evidence
- whether m1nd recovery or workspace binding helped or hurt
- qualitative agent testimony tied to artifacts

Forbidden public claims from one round:

- universal bug-finding superiority
- production benchmark certainty
- precision score without judging extra findings
- "m1nd installed is enough"
- "m1nd replaces direct files, tests, compiler output, or human review"
