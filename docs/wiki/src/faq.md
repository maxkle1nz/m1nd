# Frequently Asked Questions

## What is m1nd for?

m1nd is a local code graph and evidence-linked memory for MCP coding agents. It helps find connected code, prepare changes, and carry findings between sessions. It is most useful when understanding relationships costs more than finding one exact string.

## Does it replace grep, an LSP, or tests?

No. Text search is good at exact matches; an LSP and compiler have language semantics the graph may not capture; tests and runtime probes check executed behavior. m1nd adds graph relationships, continuity, and explicit trust/gap signals. Use these tools together.

## How does it relate to RAG or other code-search tools?

m1nd represents relationships as graph edges and combines retrieval with local memory, temporal signals, and trust envelopes. Retrieval and memory systems vary; there is no claim here that competitors cannot represent relationships or retain context. Evaluate the workflow on your own repository.

## Does my code leave my machine?

m1nd processes its graph locally and does not require an LLM API key for queries. This is not a promise of zero network traffic: installation and updates fetch packages; the default embedding feature may download a model on first use; optional document providers can contact external services. Your MCP host may send tool results to its model provider under its own policy. Review those settings separately.

## Can I work offline?

Plan ahead: install the runtime and any required model assets, then avoid network-backed document providers. Optional features and local model configuration affect what is available. Do not infer offline readiness just from a successful package install.

## Which platforms and Rust versions are supported?

Use a current stable Rust toolchain for source builds; the workspace does not declare a minimum supported Rust version. CI covers Linux, macOS, and Windows. Prebuilt release availability is narrower: inspect the [release assets](https://github.com/maxkle1nz/m1nd/releases) for your exact OS and architecture. At the `1.6.3` release, native artifacts cover Linux x86_64 and macOS x86_64/ARM64. Binary size depends on target and enabled features; a fixed 8 MB claim is not current.

## Which version should I install?

The wiki follows the `1.6.4` source line; the latest published release at this refresh is `1.6.3`. Follow [Quick Start](tutorials/quickstart.md) for the published path. Use a matching source wrapper and binary when testing source-preview automatic preparation; an updated README does not publish a package.

## Why is the npm package not enough?

It is an installer, CLI wrapper, and agent pack. The native `m1nd-mcp` binary runs the graph. `doctor` checks runtime visibility; the signed updater needs `cosign` for verification. See [Quick Start](tutorials/quickstart.md).

## Why did hosts apply print configuration instead of installing it?

Some settings are managed by the host and are intentionally printed for you to install. Read the output, complete the host-specific steps, and reconnect. Hook support differs by host and OS. The [host integration matrix](https://github.com/maxkle1nz/m1nd/blob/main/docs/HOST-INTEGRATION-MATRIX.md) is the detailed reference.

## Why can I see only a few tools?

The default `tools/list` is a core menu. `help` discovers the full registry, and `health.tool_surface_contract` reports the surface contract. `M1ND_TOOL_TIER=full` expands the advertised menu. Tools still enforce their own input, proof, and authority requirements.

## Why does ingest refuse?

Generic `replace` and `merge` are policy-disabled for clients. A person prepares a shared/served brain through the terminal birth command. Current source also supports a narrower isolated startup under an explicit launcher grant. Public `mode: "refresh"` can only re-scan an already declared exact root; it cannot create a brain or move its binding. Do not force a different root into an existing runtime.

## Does a trust verdict prove the answer is correct?

No. It reports the evidence state. `reverify` calls for a direct check, `abstain` does not support acting on the signal, and `insufficient_evidence` means the evidence is inadequate. Current source treats all v1 calibration rows as [unqualified](../../CALIBRATION-QUALIFICATION.md): prediction abstains and seek envelopes stay at `reverify` or `unprovable`. Running calibration does not enable `act`. Scores and rankings remain useful for inspection; use source, compiler, test, and runtime evidence as appropriate.

## Does write verification run my test suite?

Graph/static verification is not test execution. Dynamic verification must use the appropriate isolated execution lane and a valid receipt; inspect `NOT_RUN` and other non-claims. Never treat a static check as a green build. Advanced authority-required paths can remain unavailable until their separate prerequisites are satisfied.

## What language and repository sizes work well?

See [language coverage](https://github.com/maxkle1nz/m1nd#language-coverage). Extractor coverage does not imply compiler-grade name resolution; qualified, dynamic, macro-generated, and external references can be incomplete. Runtime memory and ingestion cost depend on graph size, enabled features, and corpus shape. This guide does not extrapolate a synthetic node benchmark into a maximum repository-size guarantee.

## Can I add documents to the graph?

The ingest engine has document adapters, including ordinary text and scholarly formats. Availability and dependencies vary. The public MCP `merge` door is not a document-import workaround; use an authorized setup path and inspect provider health. Existing document nodes can be queried and inspected through the relevant reference tools.

## Can several agents share memory?

Yes, through one local served owner with attached clients. Its repository binding matters: an owner must already cover the requested repository. Overlap warnings help coordination but do not lock source files. See [multi-agent workflows](tutorials/multi-agent.md).

## Can I expose the HTTP server publicly?

The runtime refuses non-loopback binds. Loopback does not authenticate other local processes. See [deployment and security](https://github.com/maxkle1nz/m1nd/blob/main/docs/deployment.md) before using any tunnel or shared machine; do not bypass the bind gate.

## Is it open source?

Yes, [MIT licensed](https://github.com/maxkle1nz/m1nd/blob/main/LICENSE). Graph state and memory live on disk. The [benchmark guide](benchmarks.md) explains evidence limits, and [contributing](https://github.com/maxkle1nz/m1nd/blob/main/CONTRIBUTING.md) explains development checks.
