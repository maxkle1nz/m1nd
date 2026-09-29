# Contributor development kit

The repository-local scripts/m1nd-dev command provides private state, exact binary
identity checks, guarded Vite proxying, and an MCP stdio recipe for contributors. Its
default fast profile writes checkout-private target/dev-fast output. It keeps workspace
optimization level 1 but compiles m1nd-mcp at level 0 for a measured local edit-build
experiment; debug and release remain explicit alternatives.

See the development guide at ../../../DEVELOPMENT.md for setup, supported platforms,
the model contract, security boundaries, ports, focused proof commands, and the existing
Python/UI gates that run the fixtures without a separate devkit matrix.

## Isolated startup and interruption

The launcher requires an external private runtime. Windows checks native
directory ownership, access rules, reparse points, and canonical file identity
before creating state. Missing or unprovable directories refuse startup.

On POSIX, interrupting the agent CLI also interrupts a cache-lease wait or
forwards the first signal to its owned runtime. The cache stays leased until
the child actually closes; an unconfirmed shutdown retains the lease and
reports failure. SIGTERM returns status 143.

Cold startup can cancel temporal graph construction before ownership and
checkpoint recovery after ownership. Interrupted recovery of existing state
quarantines the session and preserves its authoritative checkpoint evidence.
The full contract and shutdown limits are in
[Agent autonomy](../../../AGENT-AUTONOMY.md#public-npm-cli-and-process-status).
