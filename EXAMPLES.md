# m1nd Examples

These examples are meant to show when `m1nd` is practically useful, not just what tools exist.

All payloads below use the canonical MCP tool names from the live registry, without the optional client transport prefix.

## 1. First ingest

```jsonc
{"method":"tools/call","params":{"name":"ingest","arguments":{
  "agent_id":"dev",
  "path":"/project/backend"
}}}
```

Illustrative response shape:

```jsonc
{
  "files_processed": 335,
  "nodes_created": 9767,
  "edges_created": 26557,
  "languages": {"python": 335},
  "elapsed_ms": 910
}
```

Use this once before asking structural questions about the repo.

## 2. Search vs glob vs seek

Use the right entry point for the question.

### Exact text or regex: `search`

```jsonc
{"method":"tools/call","params":{"name":"search","arguments":{
  "agent_id":"dev",
  "query":"SessionPool",
  "mode":"literal",
  "top_k":10
}}}
```

Use this when you already know the string you want.

### Filename or path pattern: `glob`

```jsonc
{"method":"tools/call","params":{"name":"glob","arguments":{
  "agent_id":"dev",
  "pattern":"**/*session*.py",
  "top_k":20
}}}
```

Use this when you are looking for file names, not meaning.

### Intent or topic: `seek`

```jsonc
{"method":"tools/call","params":{"name":"seek","arguments":{
  "agent_id":"dev",
  "query":"where retry backoff is decided before outbound requests",
  "top_k":8
}}}
```

Use this when you know the job, but not the symbol or file name.

Another good `seek` prompt is:

```jsonc
{"method":"tools/call","params":{"name":"seek","arguments":{
  "agent_id":"dev",
  "query":"which helper canonicalizes alias tool names into dispatch status values",
  "top_k":5
}}}
```

This is the kind of question where plain grep often needs several reformulations before it converges on the right helper.

### Unsure which tool fits: `help`

```jsonc
{"method":"tools/call","params":{"name":"help","arguments":{
  "agent_id":"dev",
  "tool_name":"validate_plan"
}}}
```

Use this when the agent is stuck between tools or needs a compact recovery path after a bad call.

The useful public behavior is:

- `WHEN TO USE` for the tool's best-fit question
- `AVOID WHEN` so the agent does not force the wrong surface
- `WORKFLOWS` for the likely next move
- recovery guidance so retry loops stay short

## 3. Connected neighborhood around a topic

```jsonc
{"method":"tools/call","params":{"name":"activate","arguments":{
  "agent_id":"dev",
  "query":"session pool timeout cleanup",
  "top_k":8
}}}
```

Typical use:

- you know the topic
- you do not yet know the right file
- you want nearby code, not just exact matches

This is usually where m1nd saves the first big chunk of file reads.

## Error recovery pattern

When a tool fails, do not treat that as the end of the workflow.

Use the returned guidance like this:

- `hint`: what was wrong with the current call
- `example`: the smallest valid retry shape
- `next_step_hint`: whether to retry, inspect, or switch tools

Operational rule for agents:

- wrong tool -> reroute
- weak proof -> follow the suggested next seam
- stale continuity -> resume from hints, not from zero

Concrete example:

```jsonc
{"method":"tools/call","params":{"name":"search","arguments":{
  "agent_id":"dev",
  "query":"(",
  "mode":"regex"
}}}
```

If that fails, the useful behavior is not just "invalid regex". The useful behavior is:

- tell the agent to switch to `mode="literal"` if exact text was intended
- keep the retry payload small
- avoid falling back to shell grep unless the repair hint also fails

Another concrete recovery loop is `edit_commit` when the preview is valid but the guard rail was not lifted:

```jsonc
{"method":"tools/call","params":{"name":"edit_commit","arguments":{
  "agent_id":"dev",
  "preview_id":"preview-123",
  "confirm":false
}}}
```

The useful behavior is:

- do not force the agent to rediscover the write path
- explain that the same `preview_id` can be reused with `confirm=true`
- only rerun `edit_preview` when the preview is stale or expired

## 4. Stacktrace triage

If you already have failure output, use `trace` instead of manually walking top frames.

```jsonc
{"method":"tools/call","params":{"name":"trace","arguments":{
  "agent_id":"dev",
  "error_text":"Traceback (most recent call last): ... RuntimeError: worker pool closed during submit"
}}}
```

Use this to get from:

- crash site

to:

- likely root cause files
- structurally connected suspects
- a `proof_state` that tells you whether the run is still triaging or already strong enough for edit prep

faster than repeated grep and caller chasing.

## 5. What breaks if I touch this file?

Use `impact` before editing a central file.

```jsonc
{"method":"tools/call","params":{"name":"impact","arguments":{
  "agent_id":"dev",
  "node_id":"file::chat_handler.py"
}}}
```

Illustrative response shape:

```jsonc
{
  "blast_radius": ["file::session_store.py", "file::router.py"],
  "causal_chains": ["chat_handler.py -> session_store.py"],
  "proof_state": "proving",
  "next_suggested_tool": "view",
  "next_suggested_target": "file::session_store.py",
  "next_step_hint": "Open the strongest downstream seam before editing the root file."
}
```

This is the right tool when the question is “should I be careful?” rather than “where is the string?”

Recent behavior:

- `impact` can now suggest the strongest downstream file to open next and expose `proof_state` so the agent can tell blast triage from stronger proof
- this makes blast-radius work better as a guided handoff instead of a raw blast set

## 6. Test a structural claim

Use `hypothesize` when you want to test whether a dependency or path likely exists.

```jsonc
{"method":"tools/call","params":{"name":"hypothesize","arguments":{
  "agent_id":"dev",
  "claim":"worker_pool depends on whatsapp_manager at runtime"
}}}
```

This is useful for questions like:

- does auth bypass rate limiting here?
- does this worker path touch billing?
- is there a runtime edge between these subsystems?

Recent behavior:

- `hypothesize` can now return both a `next_suggested_target` and a `proof_state`
- strong runs land in `ready_to_edit`, while partial or inconclusive runs stay in `proving`

## 7. Ask what is missing

Use `missing` when the problem smells like an absence:

- missing validation
- missing pool abstraction
- missing cleanup
- missing lock

```jsonc
{"method":"tools/call","params":{"name":"missing","arguments":{
  "agent_id":"dev",
  "query":"database connection pooling"
}}}
```

This is one of the places where m1nd often beats plain grep, because grep can find present code, but not always the shape of what should have been there.

## 8. Prepare a multi-file edit

Use `validate_plan` before a risky change.

```jsonc
{"method":"tools/call","params":{"name":"validate_plan","arguments":{
  "agent_id":"dev",
  "include_risk_score":true,
  "include_test_impact":true,
  "actions":[
    {"action_type":"modify","file_path":"src/auth.py","description":"tighten token validation"},
    {"action_type":"modify","file_path":"src/session.py","description":"align refresh path"}
  ]
}}}
```

Then pull the primary file plus connected sources in one pass:

```jsonc
{"method":"tools/call","params":{"name":"surgical_context_v2","arguments":{
  "agent_id":"dev",
  "file_path":"src/auth.py",
  "include_tests":true,
  "radius":2,
  "max_connected_files":8
}}}
```

What this saves:

- fewer manual caller/callee reads
- fewer missed neighboring files
- better pre-edit risk awareness

Recent behavior:

- `validate_plan` now exposes `proof_state`
- `surgical_context_v2` now also exposes `proof_state`, which makes proof-focused edit prep explicit instead of implicit
- that makes it easier for an agent to tell whether it still needs more proof or can move on to edit prep

If either tool returns guidance that the plan is still unresolved, treat that as a compact repair loop:

- read `proof_hint`
- follow `next_suggested_tool`
- inspect `next_suggested_target`
- retry only after the missing seam is grounded

## 9. Explain why something looks risky

Use `heuristics_surface` when ranking or risk feels opaque and you want the reason, not just the result.

```jsonc
{"method":"tools/call","params":{"name":"heuristics_surface","arguments":{
  "agent_id":"dev",
  "file_path":"src/auth.py"
}}}
```

This is especially useful after:

- `validate_plan`
- `predict`
- `surgical_context`
- `surgical_context_v2`

## 10. Write multiple files and verify

```jsonc
{"method":"tools/call","params":{"name":"apply_batch","arguments":{
  "agent_id":"dev",
  "verify":true,
  "edits":[
    {"file_path":"/project/src/auth.py","new_content":"..."},
    {"file_path":"/project/src/session.py","new_content":"..."}
  ]
}}}
```

This is the “I already know the edit, now keep me honest” path.

It is useful when you want:

- one atomic multi-file write
- one re-ingest pass
- one post-write verdict

Recent behavior:

- `apply_batch` now returns `batch_id` so clients can correlate the final output with progress events
- `apply_batch` now returns `proof_state` plus `next_suggested_tool`, `next_suggested_target`, and `next_step_hint`
- `apply_batch` now returns a human-readable `status_message`
- it also returns coarse progress fields like `active_phase`, `completed_phase_count`, `phase_count`, `remaining_phase_count`, `progress_pct`, and `next_phase`
- it also returns structured `phases` for `validate`, `write`, `reingest`, `verify`, and `done`, with per-phase `progress_pct` and `next_phase`
- it also returns `progress_events`, which mirrors the same lifecycle in a streaming-friendly shape
- each phase now includes `phase_index` and, when useful, `current_file`
- on the HTTP/UI transport, those progress events are also emitted live onto the SSE bus as `apply_batch_progress`
- this makes long-running batch writes easier to surface in shells and UI clients

## 11. Persist small operating state

Use `boot_memory` for lightweight, canonical state that should stay hot in runtime memory.

```jsonc
{"method":"tools/call","params":{"name":"boot_memory","arguments":{
  "agent_id":"dev",
  "action":"set",
  "key":"current_auth_focus",
  "value":"refresh-token-rotation rollout in progress"
}}}
```

Read it back:

```jsonc
{"method":"tools/call","params":{"name":"boot_memory","arguments":{
  "agent_id":"dev",
  "action":"get",
  "key":"current_auth_focus"
}}}
```

Use this for compact operating doctrine or live task state, not for whole investigations.

## 12. Save and resume an investigation

Use trails for larger, graph-grounded investigations.

```jsonc
{"method":"tools/call","params":{"name":"trail_save","arguments":{
  "agent_id":"dev",
  "label":"auth-leak-investigation",
  "hypotheses":[
    {"statement":"auth tokens leak through session pool","confidence":0.7,"status":"investigating"},
    {"statement":"rate limiter missing from auth chain","confidence":0.9,"status":"confirmed"}
  ]
}}}
```

Later:

```jsonc
{"method":"tools/call","params":{"name":"trail_resume","arguments":{
  "agent_id":"dev",
  "trail_id":"trail-abc123",
  "max_reactivated_nodes":3,
  "max_resume_hints":2
}}}
```

Illustrative response shape:

```jsonc
{
  "trail_id":"trail-abc123",
  "reactivated_node_ids":[
    "file::src/auth/session.rs",
    "fn::src/auth/session.rs::rotate_refresh_token"
  ],
  "resume_hints":[
    "Re-open the refresh token rotation path before editing auth session storage.",
    "The previous investigation left an open question about recent churn in the rotation helper."
  ],
  "next_focus_node_id":"fn::src/auth/session.rs::rotate_refresh_token",
  "next_open_question":"What changed recently in refresh token rotation and which callers moved with it?",
  "next_suggested_tool":"timeline"
}
```

Use trails when you want continuity across sessions or across agents. The compact limits help when you want just the next move instead of a big resume payload.

If the trail is stale but still worth using, the recovery path should stay compact too:

```jsonc
{"method":"tools/call","params":{"name":"trail_resume","arguments":{
  "agent_id":"dev",
  "trail_id":"trail-abc123",
  "force":true,
  "max_reactivated_nodes":2,
  "max_resume_hints":2
}}}
```

Use this only when degraded continuity is still better than restarting from zero.

## 13. Follow a resumed trail with `timeline`

If the carried-forward question is temporal, `trail_resume` may point you to `timeline` next.

```jsonc
{"method":"tools/call","params":{"name":"timeline","arguments":{
  "agent_id":"dev",
  "node":"file::src/auth/session.rs",
  "depth":"30d",
  "include_co_changes":true,
  "include_churn":true
}}}
```

Illustrative response shape:

```jsonc
{
  "node":"file::src/auth/session.rs",
  "changes":[
    {
      "commit":"0b2e172",
      "subject":"route temporal resume questions to timeline",
      "timestamp":"2026-03-24T10:41:00Z",
      "lines_added":18,
      "lines_deleted":2
    }
  ],
  "total_churn":{"added":42,"deleted":11},
  "co_changes":[
    {"node":"file::m1nd-mcp/src/layer_handlers.rs","score":0.84}
  ]
}
```

This is the quickest way to turn “what were we doing here?” into recent commit proof plus likely co-changed files.

## 14. Ingest code and docs together

### Memory adapter

```jsonc
{"method":"tools/call","params":{"name":"ingest","arguments":{
  "agent_id":"dev",
  "path":"/project/docs",
  "adapter":"memory",
  "namespace":"docs",
  "mode":"merge"
}}}
```

### JSON adapter

```jsonc
{"method":"tools/call","params":{"name":"ingest","arguments":{
  "agent_id":"dev",
  "path":"/project/domain.json",
  "adapter":"json"
}}}
```

### L1GHT adapter

```jsonc
{"method":"tools/call","params":{"name":"ingest","arguments":{
  "agent_id":"dev",
  "path":"/project/specs",
  "adapter":"light",
  "mode":"merge"
}}}
```

This is useful when the right answer lives across implementation and docs, not only in code.

## 15. Honest boundary: when not to use m1nd

Use plain tools when:

- you already know the one file to edit
- you need an exact-text replacement
- the compiler or test runner is the source of truth
- the task is runtime-log driven

Examples:

- rename a string across markdown files with `rg` and `sed`
- fix a single typo in a known file
- inspect a failing test with `cargo test`, `pytest`, or logs

m1nd is most useful when structure, connected context, or blast radius is what you are missing.
