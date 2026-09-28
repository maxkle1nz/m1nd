# Contributor development kit

The repository-local scripts/m1nd-dev command provides private state, exact binary
identity checks, guarded Vite proxying, and an MCP stdio recipe for contributors. Its
default fast profile writes checkout-private target/dev-fast output. It keeps workspace
optimization level 1 but compiles m1nd-mcp at level 0 for a measured local edit-build
experiment; debug and release remain explicit alternatives.

See the development guide at ../../../DEVELOPMENT.md for setup, supported platforms,
the model contract, security boundaries, ports, focused proof commands, and the existing
Python/UI gates that run the fixtures without a separate devkit matrix.
