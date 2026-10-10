# Your First Query: Find, Check, Remember

Complete [Quick Start](quickstart.md) first. Your session must already have a graph for the intended repository. These are MCP `tools/call` payloads; your host supplies JSON-RPC framing and request IDs.

## 1. Orient before reading or editing

```json
{"method":"tools/call","params":{"name":"north","arguments":{"agent_id":"dev","task":"find where configuration is loaded"}}}
```

Read the binding, task context, prior memory, `next_move`, and `honest_gaps`. If the packet names another root or reports `needs_ingest`, stop and follow [setup recovery](quickstart.md#if-the-first-result-is-not-useful). Do not call generic `ingest` to replace the graph.

## 2. Find candidates

```json
{"method":"tools/call","params":{"name":"seek","arguments":{"agent_id":"dev","query":"configuration loading"}}}
```

Use a question that actually fits your repository. `seek` returns candidates, not a verified explanation. Its scope and node-type filters are applied to lexical and semantic candidates in the current source. Overly narrow filters can legitimately produce an explained empty result.

Read the returned source locations. Follow the suggested tool or inspect the file directly. Use node IDs from the actual response when calling `impact`, `why`, or `surgical_context`; do not invent an ID from an example.

## 3. Verify the finding

Check the source and the relevant tests. For a proposed change, inspect callers and run the repository's real compiler/test checks. A graph edge may be approximate or unresolved; a trust verdict does not replace execution proof.

- `reverify`: validate the result against direct evidence.
- `abstain`: the signal does not support acting on the prediction.
- `insufficient_evidence`: there is no adequate evidence for that conclusion.
- `caller_root_mismatch`: this session is not bound to the repository you meant.

If retrieval is blocked or empty, use its returned recovery arguments when available, or ask `doctor` for the diagnostic path. Repeating a refused write is not recovery.

## 4. Preserve only what you checked

After verifying a durable finding, the agent can store a claim with a repo-relative evidence path. This is an illustrative payload: replace the path and claim with facts from your repository before calling it.

```json
{"method":"tools/call","params":{"name":"memorize","arguments":{"agent_id":"dev","node_label":"ConfigurationLoading","claims":[{"label":"ConfigurationEntry","text":"Configuration is loaded by the entry point identified in this source file.","confidence":"high","evidence":["src/config.rs"]}]}}}
```

`memorize` is a state-changing call, not part of the read-only setup check. Stored claims retain provenance; `cross_verify` can check cited files for staleness. At the next session, review recalled evidence rather than assuming memory is still true.

## 5. Go deeper only when the task needs it

Use `help` to discover the full registry. The default `tools/list` is a small menu, not the complete API. Tools such as `trail_resume`, `impact`, and `why` may help with a longer investigation; advanced writes have separate proof and authority requirements.

Next: [multi-agent workflows](multi-agent.md), [API overview](../api-reference/overview.md), or [evidence and benchmarks](../benchmarks.md).
