# m1nd Self-Update System v0

This document defines the v0 proof boundary for `m1nd update`.

## North Star

`m1nd` should be able to diagnose, plan, apply, verify, and roll back its local
installation without pretending that an already-open MCP host has refreshed its
runtime, tool list, workspace binding, or graph truth.

## Contract

Every command returns:

```json
{
  "schema": "m1nd-self-update-v0",
  "package_version": "0.9.0-beta.2",
  "runtime_version": "m1nd-mcp 0.9.0-beta.2",
  "latest_version": "0.9.0-beta.2",
  "channel": "beta",
  "install_state": "current",
  "planned_actions": [],
  "applied_actions": [],
  "blocked_actions": [],
  "requires_host_rebind": false,
  "non_claims": []
}
```

Allowed `install_state` values:

- `current`
- `stale`
- `missing`
- `mixed`
- `unknown`

## CLI Surface

```bash
m1nd update check --channel beta --json
m1nd update status --channel beta --json
m1nd update plan --channel beta --json
m1nd update apply --channel beta --yes --json
m1nd update verify --repo . --transport stdio --json
m1nd update rollback --json
```

`check`, `status`, and `plan` are read-only. `status` is the agent cockpit: it
wraps the update proof with doctor state, visible `m1nd-mcp` processes, a
readiness summary, and `host_rebind_proven=false`.
`apply` mutates only with `--yes`.

## Apply Order

1. Update the npm package when the selected channel is ahead and npm updates are
   allowed.
2. Install the native runtime, preferring a GitHub Release binary for the
   platform/arch.
3. Fall back to `cargo install m1nd-mcp --version <version> --force` when no
   release asset is available.
4. Refresh agent-pack files when allowed.
5. Stop visible `m1nd-mcp` processes unless `--no-kill` is set.
6. Report that host/client rebind is still required.

Runtime replacement writes a backup and rollback state before overwriting the
target binary.

## False-Positive Guards

- `apply` without `--yes` is dry-run only.
- `--no-runtime` prevents runtime install and prevents backup creation.
- Registry lag does not downgrade a newer local package.
- Missing runtime reports `install_state=missing`.
- Stale runtime reports `install_state=stale` when no other surface is stale.
- Runtime install actions set `requires_host_rebind=true`.
- Rollback only runs when a local backup state exists.
- `runtime.path_binary` and `runtime.path_version` expose a stale `m1nd-mcp`
  on `PATH` even when the managed runtime is current.

## Non-Claims

The v0 updater does not claim:

- an active MCP host refreshed its cached tool list;
- an already-open conversation rebound to the new runtime;
- graph contents were repaired;
- ingest roots or workspace selection were corrected;
- semantic retrieval was fixed;
- every agent host was updated;
- unattended production-grade auto-update.

## Verification

Minimum local gate:

```bash
node --test npm/test/cli.test.js
node npm/bin/m1nd.js update check --json
node npm/bin/m1nd.js update status --json
node npm/bin/m1nd.js update plan --json
node npm/bin/m1nd.js update verify --repo . --transport stdio --json
node npm/bin/m1nd.js hosts status --host all --project . --json
npm pack --dry-run --json
cargo check --workspace
git diff --check
```

Use fake runtime binaries and `M1ND_UPDATE_STATE_PATH` when proving apply and
rollback without touching the developer machine.

## Future Expansion

- signed/checksummed release downloads;
- read-only MCP tools for update check and plan;
- host-specific rebind recipes beyond the current read-only
  `m1nd hosts status` cockpit;
- background update monitor that only notifies;
- stable v1 class after npm, Cargo, GitHub binary, macOS, Linux, Windows, and
  at least three host bindings are proven.
