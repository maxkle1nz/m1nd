# Contributor development kit

The repository includes a portable local entrypoint at scripts/m1nd-dev. It keeps
development state separate for every checkout and does not install tools, download a
model, change shell startup files, or start a public listener.

## Requirements

Use native macOS or Linux. Native Windows is intentionally refused. WSL is supported
when the checkout and private state are on its Linux filesystem.

The default toolchain matches CI: Node 22, Python 3.12, and Rust 1.98.1. Set
M1ND_NODE, M1ND_PYTHON, M1ND_NODE_MAJOR, M1ND_PYTHON_MINOR, and
M1ND_RUST_TOOLCHAIN explicitly to use installed alternatives. No Homebrew path is
assumed. Source scripts/devkit/activate.sh to prepend the selected Node and Python
directories and repository scripts. m1nd-dev shell opens bash or zsh and reapplies
that scope after the normal user shell profile.

## State and model

By default state lives below XDG_STATE_HOME/m1nd-devkit or
HOME/.local/state/m1nd-devkit, keyed by physical checkout path. Runtime, its native
registry at runtime/registry, agent cache, binary identity metadata, and work are
owner-only 0700 directories.
The kit refuses symlinks, state inside the checkout, and the live HOME/.m1nd runtime.

Set M1ND_CHECKOUT only to a Git toplevel. The canonical
scripts/cargo_target_dir.sh determines CARGO_TARGET_DIR.

Set M1ND_DEV_MODEL_DIR (or M1ND_EMBED_MODEL) to the CI-pinned embedding model, then
run m1nd-dev model-verify. It checks the three expected SHA-256 values and never
downloads model data.

## Commands

    source scripts/devkit/activate.sh
    m1nd-dev doctor
    m1nd-dev build fast
    m1nd-dev stdio
    m1nd-dev serve
    m1nd-dev ui dev
    m1nd-dev gates lightning

The default contributor profile is fast. It maps to Cargo's dev-fast output at
target/dev-fast, inheriting dev with optimization level 1, debug level 1, incremental
builds, LTO disabled, and 256 codegen units. Its m1nd-mcp package override uses optimization
level 0 while core, ingest, model, and dependencies remain at level 1. Use `debug` or
`release` explicitly when needed. This is a measured local experiment, to be reverted if it
does not improve edit builds or degrades runtime behavior; it has no timing guarantee.

serve binds only 127.0.0.1 on M1ND_DEV_HTTP_PORT, default 14438. ui dev uses
M1ND_DEV_UI_PORT, default 5173; the demo default is 5175. Ports must be integers
from 1 through 65535 and busy ports fail.

A clean binary must report the exact checkout HEAD. A dirty binary must report
HEAD-dirty and match a private content fingerprint. A single package-only clean/rebuild
handles stale build.rs identity. Each build compares a pre-build HEAD/content snapshot
with the post-build tree before writing identity metadata. It detects ordinary concurrent
edits and writes no current receipt when they differ; it does not claim an adversarial
ABA-race proof. The launcher strips inherited attach, bearer, runtime, and graph bindings
while preserving the caller cwd and HOME for m1nd-dev cli.

`m1nd-dev cli` requires an exact supported command first, appends the exact checkout
binary through the public `--binary` flag, and sets the private agent cache explicitly.
A divergent caller `--binary`, an initial option, and `/restart` are refused. The npm CLI
can still operate on external settings and host state, so this does not claim that every
CLI command uses only checkout-private state. `restart` and `update` are refused by the
devkit wrapper; use npm directly for those external operations. Host staging and
installation retain their normal npm CLI authority and are outside this kit.

For stdio, a valid owner discovery attaches automatically. Direct private stdio is
allowed only for the native canonical no-owner response with exit status 1. Registry
errors, malformed responses, and ambiguity refuse to run.

## Local UI bridge

m1nd-dev ui dev loads Vite, React, and Tailwind from m1nd-ui. It replaces the ordinary
API development proxy with a loopback bridge. Every request needs the exact loopback
Host; duplicate or foreign Origin and cross-site Sec-Fetch-Site headers are refused
before an upstream connection. Same-origin GET requests without those optional headers
remain usable.

The bridge reads http-auth-token-v1 for every request from an owner-only runtime. It
requires a uid-owned 0700 runtime and a uid-owned nonsymlink regular 0600 token file
containing one lowercase 64-hex value, opened with O_NOFOLLOW. The bearer goes only to
the owner request. Browser responses and URLs never contain it. Missing tokens and
upstream failures return generic 503.

## MCP host recipe

Use an absolute checkout path, never a user-specific home path.

    [mcp_servers.m1nd-dev]
    command = "/absolute/path/to/checkout/scripts/m1nd-dev"
    args = ["stdio"]
    cwd = "/absolute/path/to/checkout"
    required = true
    startup_timeout_sec = 300
    tool_timeout_sec = 120

When a host supports per-tool approvals, grant local read approval only to health,
trust_selftest, north, seek, help, doctor, recovery_playbook, and session_handshake.
Do not grant wildcard or mutating approval.

## Focused proof

CI reuses existing gates: python-gates discovers the launcher fixtures on Ubuntu,
and ui-gates runs the real Vite proxy fixture after its Node 22 npm install. Local focused
fixtures cover macOS too; there is no separate devkit CI matrix.

    python3 -m unittest discover -s tests -p 'test_devkit.py' -v
    npm ci --prefix m1nd-ui
    node --test scripts/devkit/ui-dev.test.mjs

Those tests prove launcher isolation with temporary Git fixtures and a real Vite server
plus a temporary loopback owner. They do not prove a real m1nd native runtime.

## Compact plan validation

`validate_plan` returns at most 24 gaps and suggested additions by default. Set
`max_gaps` to request a different limit; zero returns counts without those lists,
and values above 128 are clamped. `gaps_total`, `gaps_truncated`,
`suggested_additions_total`, and `suggested_additions_truncated` describe what was
omitted. Narrow the plan when you need to inspect more than 128 entries.

Risk, proof state, heuristic summaries, and the next-step recommendation are
computed from the complete analysis before the response is shortened. A compact
response does not mean the omitted dependencies are safe or absent.

Plan validation warms visible missing files through their existing declared
directory root. Hidden paths and files outside those roots stay unresolved;
validation does not add a file path as a new runtime root.
