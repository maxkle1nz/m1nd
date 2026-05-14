# Correctness and Regression Audit

You are being dropped into a real maintenance audit for a Python text-formatting utility.

Repository path:
/Users/kle1nz/m1nd/.m1nd-field-workspaces/bughunt-humanize-20260514T021500Z/audit-02/humanize

Task:
Find concrete correctness bugs, regression risks, boundary mistakes, data-preservation issues, and missing tests in this codebase. Work as if this report will be used by maintainers to decide what to fix before the next release. Do not assume a fixed number of issues. Do not make broad rewrites. If you produce patches, keep them local and minimal, but a high-quality audit report is the main deliverable.

Focus on evidence. A good finding includes file, function/class, cause, impact, reproduction idea or focused test, and why it matters. Avoid style-only comments. Avoid vague risk claims without a concrete code path.

Write your result JSON to:
/Users/kle1nz/m1nd/docs/benchmarks/bug-hunt-rounds/bughunt-humanize-20260514T021500Z/lane-results/audit-02.json

Append raw investigation events as JSONL to:
/Users/kle1nz/m1nd/docs/benchmarks/bug-hunt-rounds/bughunt-humanize-20260514T021500Z/event-streams/audit-02.jsonl

Use this result shape:
{
  "schema": "m1nd-bug-hunt-audit-result-v0",
  "round_id": "bughunt-humanize-20260514T021500Z",
  "lane_id": "audit-02",
  "repo_path": "/Users/kle1nz/m1nd/.m1nd-field-workspaces/bughunt-humanize-20260514T021500Z/audit-02/humanize",
  "findings": [
    {
      "title": "short specific defect title",
      "severity": "critical|high|medium|low",
      "file": "relative/path.py",
      "symbol": "function or class",
      "cause": "what is wrong",
      "impact": "what breaks or weakens",
      "evidence": ["file/function/test/proof references"],
      "reproduction_or_test": "focused test or command",
      "confidence": "high|medium|low"
    }
  ],
  "commands_run": [],
  "files_inspected": [],
  "tests_run": [],
  "m1nd_usage": [],
  "final_notes": "limits and non-claims",
  "agent_testimony": "how you worked and what helped or hurt",
  "confidence": "high|medium|low"
}

Event JSONL examples:
{"event_type":"file_read","event_source":"agent","detail":"read src/humanize/number.py"}
{"event_type":"finding","event_source":"agent","detail":"identified exact boundary regression"}
{"event_type":"test_run","event_source":"agent","detail":"uv run --extra tests pytest -q returned ..."}

Operational note:
m1nd is available. Use this command card before falling back to plain search:

1. Establish trust and workspace:
   - If live MCP tools are exposed: trust_selftest or session_handshake with scope "/Users/kle1nz/m1nd/.m1nd-field-workspaces/bughunt-humanize-20260514T021500Z/audit-02/humanize".
   - If live MCP is missing, use the probe helper with isolated runtime:
     python3 /Users/kle1nz/.codex/skills/m1nd-operator/scripts/probe_m1nd.py tools

2. Ingest/orient this exact repo:
   - ingest path/scope "/Users/kle1nz/m1nd/.m1nd-field-workspaces/bughunt-humanize-20260514T021500Z/audit-02/humanize" if graph is cold.
   - audit for repo shape.
   - search for exact symbols before rg: intcomma, fractional, clamp, natural_list, naturaltime, naturaldelta, naturaldate, naturalsize, metric.
   - seek/activate for intent queries: number formatting boundaries, negative values, empty collections, future tense handling, filesize formatting, date/time formatting.

3. For risky findings:
   - impact on the suspected file/symbol.
   - validate_plan for proposed test/fix.
   - surgical_context_v2 if preparing a patch or focused proof.

4. If retrieval is empty/blocked:
   - call recovery_playbook with the provided payload if present.
   - classify wrong-workspace vs cold graph vs stale binding.
   - then verify with direct files/tests.

Record every m1nd call or recovery step in m1nd_usage and JSONL events. Final claims must be backed by code/test evidence, not by m1nd output alone.
