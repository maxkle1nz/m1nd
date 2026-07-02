# m1nd Agent Skills

This directory is the installable agent doctrine for `m1nd`.

It is intentionally host-neutral:

- `m1nd-first` is the short rule set agents should load before repository work.
- `m1nd-operator` is the deeper operating manual for routing, recovery, `L1GHT`,
  document binding, multi-agent coordination, and runtime refresh.
- `m1nd-operator/references/full-spec-agent-os.md` is the full operating layer:
  a route table for the whole m1nd/L1GHT tool system and high-value tool
  combinations.
- `m1nd-universal-agent-pack.md` is the portable prompt-pack form for hosts that
  do not have a native skill directory.

All three carry the trained-agent loop measured in internal bug-hunt rounds:
`north(task)` as the in-session front door (one round-trip for trust, task
context, prior memory, sufficiency, and one `next_move`; `needs_ingest` → ingest
→ re-north), then act on the calibrated verdicts (`act`/`reverify`/`abstain`,
`closure`, `trust_envelope`, `insufficient_evidence`), prove final truth
directly, and `memorize` the durable finding at close. Recovery before absence,
workspace-binding clarity, and evidence logging stay part of the loop.

Use the npm installer from the repo root to install the right shape:

```bash
npm install -g .
m1nd install-skills codex
m1nd install-skills generic --project /path/to/project
```

For a published package, the same flow becomes:

```bash
npm install -g @maxkle1nz/m1nd@beta
m1nd init --host codex
```
