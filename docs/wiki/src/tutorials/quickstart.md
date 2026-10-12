# Quick Start

Start with a read-only question about a real repository. You do not need to understand the full tool catalog, run missions, or configure hardware-backed authority first.

## Choose the version you are using

Published 1.6.5 is available on [GitHub/native](https://github.com/maxkle1nz/m1nd/releases/tag/v1.6.5), crates.io, and npm at `363bc97f2336c73e90802916804dccd2109c5746`; npm `latest` is 1.6.5 as of 2026-10-12. This wiki also describes `main`; PR580's later learning-event separation is **Unreleased** and absent from sealed 1.6.5.

- **Published 1.6.5:** the matching wrapper/native runtime can prepare an isolated graph under an explicit workspace grant. See [agent autonomy](https://github.com/maxkle1nz/m1nd/blob/v1.6.5/docs/AGENT-AUTONOMY.md).
- **Historical 1.6.3:** retain [its signed installation guide](https://github.com/maxkle1nz/m1nd/blob/main/docs/INSTALL-1.6.3.md) and [historical README recipe](https://github.com/maxkle1nz/m1nd/blob/main/README.md#historical-pinned-163-install-prepare-connect). Its human birth and nested-runtime recipe are separate from the external-runtime 1.6.5 contract.

## Install the published release

Prerequisites: Node.js 18+ and a repository you can read. First follow the [signed native 1.6.5 installation guide](https://github.com/maxkle1nz/m1nd/blob/main/docs/INSTALL-1.6.5.md), which pins the exact artifact and verifies its signature with cosign. The 1.6.5 prebuilt assets cover Linux x86_64 and macOS x86_64/ARM64. Run this POSIX-shell walkthrough from your repository:

```bash
export M1ND_BINARY="$HOME/.local/share/m1nd/versions/1.6.5/m1nd-mcp"
"$M1ND_BINARY" --version
```

The npm package is a wrapper; the native `m1nd-mcp` runtime is separate. Require **1.6.5** at the release commit above. Pinning the wrapper alone does not pin `update apply`: its native target follows the registry channel. For source builds, use the [contributor path](contributor-devkit.md) with a current stable Rust toolchain and a matching wrapper/binary.

## Get a first result before connecting a host

The matching 1.6.5 CLI selects an external private cache. Clear legacy runtime overrides in a subshell for this route:

```bash
(
  unset M1ND_RUNTIME_DIR M1ND_MCP_ARGS
  npx -y @maxkle1nz/m1nd@1.6.5 agent first-minute --repo . \
    --binary "$M1ND_BINARY" --query "where is configuration loaded?" --json
)
```

Read `runtime.boot` and `runtime.owner_discovery`: a covering served owner is reused first; otherwise `first-minute` prepares only the exact granted repository in an isolated cache outside the source tree. Only when reusing the automatic isolated cache does the CLI refresh the declared root before retrieval. Attached-owner freshness follows the owner's own semantics; inspect that owner's version, repository binding, and freshness. `--binary` selects the client executable, not the discovered owner's version. Check returned source locations, gaps, and command status. A refusal is not permission to delete state, move a binding, run the human-only ceremony, or substitute generic MCP `ingest`.

## Connect your MCP host

Preview and apply the host setup:

```bash
npx -y @maxkle1nz/m1nd@1.6.5 hosts plan --host claude --project . --binary "$M1ND_BINARY"
npx -y @maxkle1nz/m1nd@1.6.5 hosts apply --host claude --project . --binary "$M1ND_BINARY" --yes
```

Replace `claude` with your host, such as `codex` or `cursor`. Some host-managed settings are printed rather than written; complete the printed steps and restart or reconnect your host. Hook support varies by host and OS. Use the [host matrix](https://github.com/maxkle1nz/m1nd/blob/main/docs/HOST-INTEGRATION-MATRIX.md) for host-specific keys.

For **direct stdio**, keep the generated `M1ND_WORKSPACE_ROOT` and add an absolute `M1ND_RUNTIME_DIR` outside the source tree. Create that runtime before connecting, for example `install -d -m 700 "$HOME/.cache/m1nd/host/repo-alpha"`, with a distinct location for each repository. The runtime must be a real directory owned by your user, with safe canonical ancestors and no group/other write permission. Use its actual absolute path in the host config; `hosts apply` does not add this selector. The first direct-stdio query can then use automatic preparation without `--birth`. Source files remain inputs; derived state stays in the private runtime.

For example, the environment portion of a JSON host config should contain both absolute paths (replace them with your actual paths):

```json
{"M1ND_WORKSPACE_ROOT":"/absolute/path/to/project","M1ND_RUNTIME_DIR":"/absolute/private/runtime/repo-alpha"}
```

For a **shared owner**, attach only to an owner that already covers your repository. Preparing a new shared brain retains its separate human/authority requirements; an isolated launcher grant cannot mint one. See [deployment](https://github.com/maxkle1nz/m1nd/blob/main/docs/deployment.md) and [agent autonomy](https://github.com/maxkle1nz/m1nd/blob/v1.6.5/docs/AGENT-AUTONOMY.md). The human-only birth ceremony remains available for the legacy/shared paths where required; agents must not perform it.

Do not copy a shared `/tmp/graph.json` configuration between repositories. Managed runtime state belongs to the selected runtime directory; persistence is not generally disabled just because `M1ND_GRAPH_SOURCE` is unset.

## Ask for your first result

Ask your agent:

> Use m1nd to find where configuration is loaded in this repository. Start with north, show the source locations, and list what still needs verification. Do not edit files.

The expected sequence is `north` → `seek` → source inspection. The [first-query tutorial](first-query.md) shows the wire calls. Successful setup means:

1. The response is bound to the repository you intended.
2. It contains useful locations that exist in that repository.
3. The agent reports gaps and trust verdicts rather than treating retrieval as proof.

No example node count, result score, or latency is a target. Your repository determines the result.

## If the first result is not useful

| Symptom | Next step |
|---|---|
| Native binary missing | Verify the pinned installation succeeded and `$M1ND_BINARY --version` identifies 1.6.5 at the sealed commit; pass that path to the setup commands. |
| Host has no m1nd tools | Complete the printed configuration, reconnect, then inspect the host's MCP logs. |
| Only a small tool menu appears | This is intentional. `help` discovers the full registry; hidden tools are not necessarily removed. |
| `needs_ingest` / `needs_authority` | Check the matching versions, launcher grant, private runtime, and binding. Shared-brain and legacy routes retain their authority requirements. |
| `caller_root_mismatch` / `wrong_workspace_binding` | Stop writes and connect to the correct runtime or covering owner. Do not rebind another graph through `ingest`. |
| Writer lease already held | Attach to the existing owner rather than starting a competing writer. |
| `seek` is empty | Check graph health, scope and node-type filters, then use the returned recovery arguments or `doctor`. Empty is not proof the code is absent. |
| `reverify` / `abstain` | In 1.6.5, all v1 calibration rows are unqualified: prediction abstains and seek stays at `reverify` or `unprovable`. Calibration cannot enable `act`; verify source and execution evidence. |

Public `ingest` is not a bootstrap shortcut: default/`replace` and `merge` are policy-disabled for clients. The narrow `mode: "refresh"` path only re-scans an already declared exact root and can refuse unsafe root changes or graph shrinkage. It does not create a brain or add a document root.

## Continue

- [First query](first-query.md): orient, find code, verify, remember
- [Multi-agent workflows](multi-agent.md): share a local owner and coordinate
- [Deployment](https://github.com/maxkle1nz/m1nd/blob/main/docs/deployment.md): persistent owners and transport limits
- [Contributor development kit](contributor-devkit.md): source builds and private development state
