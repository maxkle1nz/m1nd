# Correctness and Regression Audit

You are being dropped into a real maintenance audit for a Python text-formatting utility.

Repository path:
/Users/kle1nz/m1nd/.m1nd-field-workspaces/bughunt-humanize-20260514T021500Z/audit-10/humanize

Task:
Find concrete correctness bugs, regression risks, boundary mistakes, data-preservation issues, and missing tests in this codebase. Work as if this report will be used by maintainers to decide what to fix before the next release. Do not assume a fixed number of issues. Do not make broad rewrites. If you produce patches, keep them local and minimal, but a high-quality audit report is the main deliverable.

Focus on evidence. A good finding includes file, function/class, cause, impact, reproduction idea or focused test, and why it matters. Avoid style-only comments. Avoid vague risk claims without a concrete code path.

Write your result JSON to:
/Users/kle1nz/m1nd/docs/benchmarks/bug-hunt-rounds/bughunt-humanize-20260514T021500Z/lane-results/audit-10.json

Append raw investigation events as JSONL to:
/Users/kle1nz/m1nd/docs/benchmarks/bug-hunt-rounds/bughunt-humanize-20260514T021500Z/event-streams/audit-10.jsonl

Use this result shape:
{
  "schema": "m1nd-bug-hunt-audit-result-v0",
  "round_id": "bughunt-humanize-20260514T021500Z",
  "lane_id": "audit-10",
  "repo_path": "/Users/kle1nz/m1nd/.m1nd-field-workspaces/bughunt-humanize-20260514T021500Z/audit-10/humanize",
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

Operational constraint:
Use normal local engineering tools: file reads, rg, tests, git diff, and source reasoning. Do not use m1nd MCP tools, m1nd CLI helpers, probe_m1nd.py, graph retrieval, or m1nd docs for this audit.
