# m1nd Agent Skills

This directory is the installable agent doctrine for `m1nd`.

It is intentionally host-neutral:

- `m1nd-first` is the short rule set agents should load before repository work.
- `m1nd-operator` is the deeper operating manual for routing, recovery, `L1GHT`,
  document binding, multi-agent coordination, and runtime refresh.
- `m1nd-universal-agent-pack.md` is the portable prompt-pack form for hosts that
  do not have a native skill directory.

Use the npm installer from the repo root to install the right shape:

```bash
npm install -g .
m1nd install-skills codex
m1nd install-skills generic --project /path/to/project
```

For a published package, the same flow becomes:

```bash
npm install -g @m1nd/m1nd
m1nd init --host codex
```
