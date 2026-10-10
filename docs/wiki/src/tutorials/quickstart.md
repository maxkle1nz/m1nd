# Quick Start

Start with a read-only question about a real repository. You do not need to understand the full tool catalog, run missions, or configure hardware-backed authority first.

## Choose the version you are using

This wiki follows the `1.6.4` source line. At the documentation refresh, `1.6.3` is the latest published release. Check the [release list](https://github.com/maxkle1nz/m1nd/releases) before assuming a source feature is in your installed package.

- **Published release:** a person prepares the repository with the terminal birth command, then connects an MCP host.
- **Current source preview:** the matching npm wrapper and native binary can prepare an isolated graph under an explicit workspace grant. See [agent autonomy](https://github.com/maxkle1nz/m1nd/blob/main/docs/AGENT-AUTONOMY.md).

## Install the published release

Prerequisites: Node.js 18+, an MCP host, a repository you can read, and [cosign](https://docs.sigstore.dev/cosign/system_config/installation/) on `PATH` for signed binary verification. The 1.6.3 prebuilt assets cover Linux x86_64 and macOS x86_64/ARM64. This POSIX-shell walkthrough pins the wrapper; run it from your repository:

```bash
npx -y @maxkle1nz/m1nd@1.6.3 update apply --yes
npx -y @maxkle1nz/m1nd@1.6.3 doctor --json
```

The npm package is a wrapper; the native `m1nd-mcp` runtime is separate. `doctor` must find native **1.6.3** for this recipe. If it reports another version, check that release's setup contract before proceeding. The signed updater verifies its release before installation and refuses verification failures. For source builds, use the [contributor path](contributor-devkit.md) with a current stable Rust toolchain and a matching wrapper/binary.

## Prepare the repository, then connect your host

A person runs the following once, in a terminal, for an empty repository runtime:

```bash
export M1ND_RUNTIME_DIR="$(pwd -P)/.m1nd"
npx -y @maxkle1nz/m1nd@1.6.3 init --birth .
```

Read the receipt: require `brain: "owner_bound_graph"`, the intended `born_root`, `store_dir` equal to the absolute runtime above, and `node_count > 0`. A refusal is not a request to delete state or bypass an authority gate. Agents must not run the human-only ceremony or substitute generic MCP `ingest`.

Preview and apply the host setup:

```bash
npx -y @maxkle1nz/m1nd@1.6.3 hosts plan --host claude --project .
npx -y @maxkle1nz/m1nd@1.6.3 hosts apply --host claude --project . --yes
```

Replace `claude` with your host, such as `codex` or `cursor`. Some host-managed settings are printed rather than written; complete the printed steps and restart or reconnect your host. Hook support varies by host and OS. Use the [host matrix](https://github.com/maxkle1nz/m1nd/blob/main/docs/HOST-INTEGRATION-MATRIX.md) for the exact recipe. Before reconnecting, add `M1ND_RUNTIME_DIR` to the generated MCP server environment using the exact absolute value from `echo "$M1ND_RUNTIME_DIR"`; keep the generated `M1ND_WORKSPACE_ROOT`. `hosts apply` does not add this runtime selector. Without it, a host launched from a different working directory can open a different graph.

For example, the environment portion of a JSON host config should contain both absolute paths (replace them with your actual paths):

```json
{"M1ND_WORKSPACE_ROOT":"/absolute/path/to/project","M1ND_RUNTIME_DIR":"/absolute/path/to/project/.m1nd"}
```

This is a 1.6.3 direct-stdio recipe. The 1.6.4 source-preview launcher has a different external-private-runtime contract; do not mix its automatic grant with this nested runtime setup.

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
| Native binary missing | Re-run `doctor`; verify the updater succeeded and the selected binary is visible. |
| Host has no m1nd tools | Complete the printed configuration, reconnect, then inspect the host's MCP logs. |
| Only a small tool menu appears | This is intentional. `help` discovers the full registry; hidden tools are not necessarily removed. |
| `needs_ingest` / `needs_authority` | Check your installed version and binding. Use the appropriate published or source-preview path above. |
| `caller_root_mismatch` / `wrong_workspace_binding` | Stop writes and connect to the correct runtime or covering owner. Do not rebind another graph through `ingest`. |
| Writer lease already held | Attach to the existing owner rather than starting a competing writer. |
| `seek` is empty | Check graph health, scope and node-type filters, then use the returned recovery arguments or `doctor`. Empty is not proof the code is absent. |
| `reverify` / `abstain` | Read the evidence or gather more. Do not turn a trust limit into permission to act. |

Public `ingest` is not a bootstrap shortcut: default/`replace` and `merge` are policy-disabled for clients. The narrow `mode: "refresh"` path only re-scans an already declared exact root and can refuse unsafe root changes or graph shrinkage. It does not create a brain or add a document root.

## Continue

- [First query](first-query.md): orient, find code, verify, remember
- [Multi-agent workflows](multi-agent.md): share a local owner and coordinate
- [Deployment](https://github.com/maxkle1nz/m1nd/blob/main/docs/deployment.md): persistent owners and transport limits
- [Contributor development kit](contributor-devkit.md): source builds and private development state
