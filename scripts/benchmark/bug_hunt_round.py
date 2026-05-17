#!/usr/bin/env python3
"""Score blinded bug-hunt benchmark rounds.

Bug-hunt rounds measure seeded defect recall in realistic repo audits. They are
not precision claims: agents may report extra real issues, but only seeded bugs
from the operator-only answer key are adjudicated by this scorer.
"""

import argparse
import json
import statistics
from collections import defaultdict
from datetime import datetime, timezone
from pathlib import Path


ROUND_SCHEMA = "m1nd-bug-hunt-round-v0"
LANE_SCHEMA = "m1nd-bug-hunt-audit-result-v0"
ANSWER_KEY_SCHEMA = "m1nd-bug-hunt-answer-key-v0"
REPORT_SCHEMA = "m1nd-bug-hunt-report-v0"
FINDING_EVENT_TYPES = {
    "finding",
    "finding_recorded",
    "findings_finalized",
    "findings_identified",
    "runtime_probe",
    "runtime_probes_completed",
    "focused_probes",
    "probe_result",
}
MISSION_CONTROL_TOKENS = (
    "mission_start",
    "mission_next",
    "mission_verify",
    "mission_close",
)
MISSION_CONTROL_REQUIRED_STEP_COUNT = len(MISSION_CONTROL_TOKENS)

NON_CLAIMS = [
    "one bug-hunt round is not a public performance claim",
    "seeded recall does not measure all real defects in the fixture repo",
    "extra findings are reported as unadjudicated, not as false positives",
    "agent testimony is not evidence without scored finding artifacts",
    "m1nd does not replace tests, compiler output, git history, rg, or direct file truth",
]


VALID_INSTRUCTION_MODES = (
    "m1nd-full-spec",
    "m1nd-temponizer-full",
    "m1nd-temponizer-compact",
    "m1nd-temponizer",
    "m1nd-short-audit",
    "m1nd-mission-control",
    "m1nd-trained",
    "m1nd-basic",
    "direct",
)


def now_iso():
    return datetime.now(timezone.utc).isoformat()


def load_json(path: Path):
    with path.open("r", encoding="utf-8") as handle:
        return json.load(handle)


def dump_json(path: Path, payload):
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("w", encoding="utf-8") as handle:
        json.dump(payload, handle, indent=2, ensure_ascii=False)
        handle.write("\n")


def safe_rate(numerator, denominator):
    if not denominator:
        return None
    return round(numerator / denominator, 4)


def median(values):
    values = [value for value in values if isinstance(value, (int, float))]
    if not values:
        return None
    return round(statistics.median(values), 3)


def lane_plan(
    full_spec_count=0,
    temponizer_full_count=0,
    temponizer_compact_count=0,
    temponizer_count=0,
    short_audit_count=0,
    mission_control_count=0,
    trained_count=3,
    basic_count=3,
    direct_count=3,
):
    lanes = []
    for index in range(1, full_spec_count + 1):
        lanes.append({"lane_id": f"audit-{len(lanes) + 1:02d}", "instruction_mode": "m1nd-full-spec"})
    for index in range(1, temponizer_full_count + 1):
        lanes.append({"lane_id": f"audit-{len(lanes) + 1:02d}", "instruction_mode": "m1nd-temponizer-full"})
    for index in range(1, temponizer_compact_count + 1):
        lanes.append({"lane_id": f"audit-{len(lanes) + 1:02d}", "instruction_mode": "m1nd-temponizer-compact"})
    for index in range(1, temponizer_count + 1):
        lanes.append({"lane_id": f"audit-{len(lanes) + 1:02d}", "instruction_mode": "m1nd-temponizer"})
    for index in range(1, short_audit_count + 1):
        lanes.append({"lane_id": f"audit-{len(lanes) + 1:02d}", "instruction_mode": "m1nd-short-audit"})
    for index in range(1, mission_control_count + 1):
        lanes.append({"lane_id": f"audit-{len(lanes) + 1:02d}", "instruction_mode": "m1nd-mission-control"})
    for index in range(1, trained_count + 1):
        lanes.append({"lane_id": f"audit-{len(lanes) + 1:02d}", "instruction_mode": "m1nd-trained"})
    for index in range(1, basic_count + 1):
        lanes.append({"lane_id": f"audit-{len(lanes) + 1:02d}", "instruction_mode": "m1nd-basic"})
    for index in range(1, direct_count + 1):
        lanes.append({"lane_id": f"audit-{len(lanes) + 1:02d}", "instruction_mode": "direct"})
    return lanes


def lane_prompt(round_payload, lane):
    common = [
        f"# Bug-Hunt Audit Lane: {lane['lane_id']}",
        "",
        f"Round: `{round_payload['round_id']}`",
        f"Repo: `{round_payload['repo']}`",
        f"Instruction mode: `{lane['instruction_mode']}`",
        f"Workspace: `{lane['repo_path']}`",
        "",
        "Work as if this is a real production-minded dependency audit.",
        "Do not guess the benchmark hypothesis, bug count, or comparison arm.",
        "Find real behavioral defects, edge-case regressions, missing tests, or contract mismatches.",
        "Do not patch files. Do not read `operator-only/` artifacts.",
        "",
    ]

    if lane["instruction_mode"] == "m1nd-full-spec":
        mode = [
            "## m1nd Full-Spec Operating Layer",
            "",
            "Use m1nd as the full agent operating layer, not only as search.",
            "Before the audit, read or reference the full-spec manual:",
            "",
            "`/Users/kle1nz/m1nd/skills/m1nd-operator/references/full-spec-agent-os.md`",
            "",
            "Required operating posture:",
            "",
            "1. Establish trust with `trust_selftest`, or `session_handshake` scoped to this repo.",
            "2. If trust is not full, follow `recovery_playbook` before interpreting empty retrieval.",
            "3. Choose tools by situation: `search`/`glob`/`view` for exact truth, `audit`/`panoramic`/`layers` for repo map, `seek`/`activate`/`why` for connected purpose, `trace`/`heuristics_surface`/`impact` for defects, `validate_plan`/`surgical_context_v2` for connected proof.",
            "4. Use deeper families when warranted: `document_*`/L1GHT for docs, `perspective_*`/`trail_*` for long investigation, `federate*` for multi-repo, `lock_*` for coordination, `taint_trace`/`ghost_edges`/`tremor`/`epidemic` for deep risk.",
            "5. Verify final truth with source reads, focused probes, tests, or compiler/runtime output.",
            "6. Treat the manual as a route table, not a checklist; use the narrowest combination that proves the finding.",
            "7. If using local `probe_m1nd.py` in this benchmark workspace, pass `--no-worktree-artifacts --workspace-root <repo>` unless intentionally debugging runtime sidecar state.",
            "8. Record m1nd calls, tool combinations, recovery path, files inspected, commands run, fallback reasons, and where the full-spec layer helped or hurt.",
            "",
        ]
    elif lane["instruction_mode"] == "m1nd-temponizer-full":
        mode = [
            "## m1nd + Temponizer Full-Spec Mode",
            "",
            "Use the trained-agent m1nd loop and the full Temponizer recalibration model.",
            "",
            "m1nd trained loop:",
            "",
            "1. Establish trust with `trust_selftest`, or `session_handshake` scoped to this repo.",
            "2. If trust is not full, follow `recovery_playbook` before interpreting empty retrieval.",
            "3. Treat `wrong_workspace_binding` as binding/scope state, not stale graph truth.",
            "4. Orient with `audit`, then use `search`, `seek`, or `activate` for focused discovery.",
            "5. Read runtime envelopes before trusting empty results.",
            "6. Verify final truth with source reads, focused probes, tests, or compiler/runtime output.",
            "7. Use `impact`, `validate_plan`, or `surgical_context_v2` when a finding needs connected proof.",
            "8. If using local `probe_m1nd.py` in this benchmark workspace, pass `--no-worktree-artifacts` unless intentionally debugging runtime sidecar state.",
            "",
            "Temponizer full spec:",
            "",
            "Before every major investigation move, classify phase `phi`: `GEN`, `IO`, `DBG`, or `PAR`.",
            "For any action that feels long, name the inherited human-duration estimate `Tp`, then compute corrected agent time: `Tc = alpha(phi) * Tp`.",
            "Initial alpha priors: `GEN=0.03`, `IO=0.10`, `DBG=0.08`, `PAR=0.05`.",
            "Act on `Tc`, not `Tp`.",
            "Your real constraints are tokens, context window, tool latency, compute, permissions, and I/O.",
            "Your real constraints are not fatigue, boredom, attention decay, human schedule, or manual typing cost.",
            "After each phase, record measured `Te`. If `Te` diverges from `Tc`, update the local alpha used for the next similar phase.",
            "Use this loop to decide whether to keep searching, run a focused probe, run broad tests, iterate, abandon a line, parallelize independent reads/probes, or stop when proof is enough.",
            "",
            "Record `temponizer_usage` with at least: `phase`, `tp_estimate`, `alpha`, `tc_estimate`, `te_measured`, `decision`, and `recalibration_note` where measurable.",
            "",
        ]
    elif lane["instruction_mode"] == "m1nd-temponizer-compact":
        mode = [
            "## m1nd + Temponizer Compact Mode",
            "",
            "Use the trained-agent m1nd loop and the compact Temponizer model.",
            "",
            "m1nd trained loop:",
            "",
            "1. Establish trust with `trust_selftest`, or `session_handshake` scoped to this repo.",
            "2. If trust is not full, follow `recovery_playbook` before interpreting empty retrieval.",
            "3. Treat `wrong_workspace_binding` as binding/scope state, not stale graph truth.",
            "4. Orient with `audit`, then use `search`, `seek`, or `activate` for focused discovery.",
            "5. Read runtime envelopes before trusting empty results.",
            "6. Verify final truth with source reads, focused probes, tests, or compiler/runtime output.",
            "7. Use `impact`, `validate_plan`, or `surgical_context_v2` when a finding needs connected proof.",
            "8. If using local `probe_m1nd.py` in this benchmark workspace, pass `--no-worktree-artifacts` unless intentionally debugging runtime sidecar state.",
            "",
            "Temponizer compact model:",
            "",
            "- For major decisions only, classify phase `phi`: `GEN`, `IO`, `DBG`, or `PAR`.",
            "- When an action feels long, name inherited human-time `Tp` and compute corrected agent time `Tc = alpha(phi) * Tp`.",
            "- Initial alpha priors: `GEN=0.03`, `IO=0.10`, `DBG=0.08`, `PAR=0.05`.",
            "- Act on `Tc` and real agent constraints: tokens, context, tool latency, compute, permissions, and I/O.",
            "- Do not optimize for human fatigue, boredom, attention decay, typing cost, or calendar intuition.",
            "- Record `Te` only for meaningful branch decisions, broad probes, focused probes, and stopping decisions.",
            "- Keep the audit moving; temporal calibration should reduce hesitation, not become paperwork.",
            "",
            "Record compact `temponizer_usage` entries with: `phase`, `tc_estimate`, `te_measured` if known, `decision`, and `recalibration_note`.",
            "",
        ]
    elif lane["instruction_mode"] == "m1nd-temponizer":
        mode = [
            "## m1nd + Temponizer Mode",
            "",
            "Use the trained-agent m1nd loop and apply temporal calibration to the audit:",
            "",
            "1. Establish trust with `trust_selftest`, or `session_handshake` scoped to this repo.",
            "2. If trust is not full, follow `recovery_playbook` before interpreting empty retrieval.",
            "3. Treat `wrong_workspace_binding` as binding/scope state, not stale graph truth.",
            "4. Orient with `audit`, then use `search`, `seek`, or `activate` for focused discovery.",
            "5. Read runtime envelopes before trusting empty results.",
            "6. Verify final truth with source reads, focused probes, tests, or compiler/runtime output.",
            "7. Use `impact`, `validate_plan`, or `surgical_context_v2` when a finding needs connected proof.",
            "8. If using local `probe_m1nd.py` in this benchmark workspace, pass `--no-worktree-artifacts` unless intentionally debugging runtime sidecar state.",
            "9. Apply TEMPONIZER: classify work phases as GEN/IO/DBG/PAR, avoid inherited human duration guesses, prefer short measured loops, and record measured `Te` for major phases.",
            "10. Record m1nd calls, recovery path, files inspected, commands run, fallback reasons, and `Te` notes.",
            "",
        ]
    elif lane["instruction_mode"] == "m1nd-short-audit":
        mode = [
            "## m1nd Short-Audit Mode",
            "",
            "Use m1nd as a bounded orientation pass, then move quickly to direct source and runtime proof.",
            "This mode is for small or localized audit tasks where full graph navigation may cost more than it returns.",
            "",
            "Short-audit loop:",
            "",
            "1. Establish trust with `trust_selftest`, or `session_handshake` scoped to this repo.",
            "2. Prefer `probe_m1nd.py --no-worktree-artifacts --workspace-root <repo> "
            "short-audit --agent-id <lane> --repo <repo> --query <focused query> "
            "--tool search` so trust, ingest when needed, one cheap orientation call, "
            "and the direct-proof handoff happen in one MCP process.",
            "3. Spend a fixed small budget on m1nd: at most one helper call plus, only if it produced concrete leads, one additional focused orientation call.",
            "4. Record `m1nd_usage_mode=\"short_audit_orientation\"` in notes or `m1nd_usage`.",
            "5. After that budget, switch to direct source reads, git diff, focused runtime probes, tests, or compiler output.",
            "6. If m1nd is blocked, stale, or noisy after the bounded pass, record `recovery_overhead` and continue directly.",
            "7. Do not keep exploring the graph after concrete suspect files and behaviors are visible.",
            "8. Record m1nd calls, recovery path, files inspected, commands run, direct probes, fallback reason, and where short-audit helped or hurt.",
            "",
        ]
    elif lane["instruction_mode"] == "m1nd-mission-control":
        mode = [
            "## m1nd Mission Control Mode",
            "",
            "Use Mission Control v0 as the operating loop for this audit.",
            "Mission Control is not a replacement for source reads, tests, compiler output, or runtime proof.",
            "",
            "Required operating loop:",
            "",
            "1. Establish trust with `trust_selftest`, or `session_handshake` scoped to this repo.",
            "2. If mission tools are not visible in this host, record `mission_control_unavailable=true`, fall back to the `m1nd-trained` loop, and do not fake mission calls.",
            "3. Start a repo-scoped mission with `mission_start`: `agent_id=<lane_id>`, `repo=<workspace>`, `task=\"bug-hunt audit for behavioral defects\"`, `mode=\"bug_hunt\"`, `budget=\"normal\"`, and `risk=\"medium\"`.",
            "4. Take the starter move, then call `mission_next` after each meaningful action with a concise `last_event` summary.",
            "5. Treat `do_not` entries from `mission_next` as guardrails. If you disagree, record a dissent event explaining the chosen tool and required evidence.",
            "6. When `mission_next` switches to direct proof, stop graph exploration and use direct source reads, rg, tests, compiler output, or focused runtime probes.",
            "7. Call `mission_verify` before finalizing material findings. If a claim is rejected or needs evidence, gather that evidence or lower the confidence.",
            "8. Call `mission_close` before writing the final lane JSON; preserve gaps, non-claims, and proof-packet summary.",
            "9. If using local `probe_m1nd.py` in this benchmark workspace, pass `--no-worktree-artifacts --workspace-root <repo>` unless intentionally debugging runtime sidecar state.",
            "10. Fill `mission_control_usage` in the lane result with `mission_id`, route, call counts, unavailable state, `do_not` guardrails, verified/rejected claims, direct-proof switches, and proof-packet summary.",
            "11. Also preserve raw m1nd calls in `m1nd_usage` when useful for auditability.",
            "",
        ]
    elif lane["instruction_mode"] == "m1nd-trained":
        mode = [
            "## m1nd-Trained Operating Loop",
            "",
            "Use the trained-agent m1nd loop:",
            "",
            "1. Establish trust with `trust_selftest`, or `session_handshake` scoped to this repo.",
            "2. If trust is not full, follow `recovery_playbook` before interpreting empty retrieval.",
            "3. Treat `wrong_workspace_binding` as binding/scope state, not stale graph truth.",
            "4. Orient with `audit`, then use `search`, `seek`, or `activate` for focused discovery.",
            "5. Read runtime envelopes before trusting empty results.",
            "6. Verify final truth with source reads, focused probes, tests, or compiler/runtime output.",
            "7. Use `impact`, `validate_plan`, or `surgical_context_v2` when a finding needs connected proof.",
            "8. If using local `probe_m1nd.py` in this benchmark workspace, pass `--no-worktree-artifacts` unless intentionally debugging runtime sidecar state.",
            "9. Record m1nd calls, recovery path, files inspected, commands run, and fallback reasons.",
            "",
        ]
    elif lane["instruction_mode"] == "m1nd-basic":
        mode = [
            "## m1nd-Basic Mode",
            "",
            "m1nd is available if useful, but no special operating card is provided.",
            "Preserve truth: if m1nd is blocked, stale, wrong-workspace, or unavailable, say so and fall back to local files.",
            "If using local `probe_m1nd.py` in this benchmark workspace, pass `--no-worktree-artifacts` unless intentionally debugging runtime sidecar state.",
            "",
        ]
    else:
        mode = [
            "## Direct Mode",
            "",
            "Do not use m1nd tools or m1nd helper scripts for this audit.",
            "Use normal local repo tools such as file reads, rg, git, tests, and compiler/runtime output.",
            "",
        ]

    output = [
        "## Required Output",
        "",
        f"Write your final JSON result to `{lane['result']}`.",
        f"Append investigation events to `{lane['events']}` using `event_source=\"agent\"`.",
        "Every event must include `schema`, `round_id`, `lane_id`, `event_source`, `event_type`, and `created_at`.",
        "Record at least `audit_started`, one first-discovery event such as `findings_identified`, `focused_probes`, or `runtime_probe`, and `result_written`.",
        "Use ISO timestamps; do not use `ts` or `event` as substitutes in new rounds.",
        "Use the schema in `lane-result-template.json`.",
        "",
        "Findings should include title, severity, file, symbol, cause, impact, evidence, reproduction_or_test, and confidence.",
        "Extra findings are welcome, but they must be concrete and source-backed.",
        "",
    ]
    return "\n".join(common + mode + output)


def result_template(round_payload, lane):
    return {
        "schema": LANE_SCHEMA,
        "round_id": round_payload["round_id"],
        "lane_id": lane["lane_id"],
        "instruction_mode": lane["instruction_mode"],
        "repo": round_payload["repo"],
        "model": "",
        "started_at": "",
        "finished_at": "",
        "findings": [],
        "commands_run": [],
        "files_inspected": [],
        "m1nd_usage": [],
        "mission_control_usage": {
            "mission_id": "",
            "mission_route": "",
            "mission_control_unavailable": False,
            "mission_start_called": False,
            "mission_next_count": 0,
            "mission_verify_count": 0,
            "mission_close_called": False,
            "do_not_guardrails_observed": [],
            "verified_claims": [],
            "rejected_or_insufficient_claims": [],
            "direct_proof_switches": [],
            "proof_packet_summary": "",
        },
        "temponizer_usage": [],
        "agent_testimony": "",
        "notes": "",
        "non_claims": [
            "auditor did not see the operator-only answer key",
            "extra findings are unadjudicated until a judge validates them",
        ],
    }


def answer_key_template(round_payload):
    return {
        "schema": ANSWER_KEY_SCHEMA,
        "round_id": round_payload["round_id"],
        "repo": round_payload["repo"],
        "source_commit": round_payload.get("source_commit"),
        "seeded_bug_count": round_payload["seeded_bug_count"],
        "bugs": [],
        "non_claims": [
            "Primary auditors are not told bug count or comparison arm.",
            "Finding extra real issues is allowed but seeded recall is measured against operator-defined defects.",
        ],
    }


def write_text(path: Path, content: str):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(content, encoding="utf-8")


def init_round(args):
    out_dir = args.out_dir.resolve()
    workspace_root = args.workspace_root.resolve() if args.workspace_root else None
    if workspace_root is None:
        workspace_root = (Path(".m1nd-field-workspaces") / args.round_id).resolve()

    lanes = []
    for lane in lane_plan(
        args.lanes_full_spec,
        args.lanes_temponizer_full,
        args.lanes_temponizer_compact,
        args.lanes_temponizer,
        args.lanes_short_audit,
        args.lanes_mission_control,
        args.lanes_trained,
        args.lanes_basic,
        args.lanes_direct,
    ):
        repo_path = workspace_root / lane["lane_id"] / args.repo
        lane.update(
            {
                "repo_path": str(repo_path),
                "prompt": str(out_dir / "lane-prompts" / f"{lane['lane_id']}.md"),
                "result": str(out_dir / "lane-results" / f"{lane['lane_id']}.json"),
                "events": str(out_dir / "event-streams" / f"{lane['lane_id']}.jsonl"),
            }
        )
        lanes.append(lane)

    round_payload = {
        "schema": ROUND_SCHEMA,
        "round_id": args.round_id,
        "created_at": now_iso(),
        "repo": args.repo,
        "source_repo": str(args.source_repo.resolve()) if args.source_repo else None,
        "seeded_repo": str(args.seeded_repo.resolve()) if args.seeded_repo else None,
        "workspace_root": str(workspace_root),
        "source_commit": args.source_commit,
        "seeded_bug_count": args.seeded_bug_count,
        "lanes": lanes,
        "invalidated_attempts": [],
        "non_claims": [
            "Primary auditors are not told bug count or comparison arm.",
            "This is an internal field simulation, not public benchmark evidence.",
            "The init command creates scaffolding only; the operator must prepare seeded workspaces and answer key.",
        ],
    }

    dump_json(out_dir / "round.json", round_payload)
    dump_json(out_dir / "lane-result-template.json", result_template(round_payload, lanes[0]))
    dump_json(out_dir / "operator-only" / "answer-key.json", answer_key_template(round_payload))

    for lane in lanes:
        write_text(Path(lane["prompt"]), lane_prompt(round_payload, lane))
        dump_json(Path(lane["result"]), result_template(round_payload, lane))
        event = {
            "schema": "m1nd-bug-hunt-event-v0",
            "round_id": args.round_id,
            "lane_id": lane["lane_id"],
            "event_type": "audit_assigned",
            "event_source": "harness",
            "created_at": now_iso(),
        }
        write_text(Path(lane["events"]), json.dumps(event, ensure_ascii=False) + "\n")

    return round_payload


def average(values):
    values = [value for value in values if isinstance(value, (int, float))]
    if not values:
        return None
    return round(sum(values) / len(values), 3)


def text_of(value):
    if value is None:
        return ""
    if isinstance(value, str):
        return value
    if isinstance(value, (int, float, bool)):
        return str(value)
    if isinstance(value, list):
        return " ".join(text_of(item) for item in value)
    if isinstance(value, dict):
        return " ".join(text_of(item) for item in value.values())
    return str(value)


def finding_text(finding):
    return text_of(finding).lower().replace("-", "_")


def match_terms(text, match_terms):
    for group in match_terms:
        if isinstance(group, str):
            options = [group]
        else:
            options = list(group)
        if not any(str(option).lower().replace("-", "_") in text for option in options):
            return False
    return True


def match_seeded_bug(finding, answer_bugs=None):
    text = finding_text(finding)
    symbol = str(finding.get("symbol", "")).lower()
    title = str(finding.get("title", "")).lower()

    for bug in answer_bugs or []:
        terms = bug.get("match_terms")
        if terms and match_terms(text, terms):
            return bug["id"]

    if (
        ("intcomma" in text or symbol == "intcomma")
        and "negative" in text
        and ("group" in text or "separator" in text or "comma" in text)
    ):
        return "intcomma-negative-numbers-not-grouped"

    if (
        ("fractional" in text or symbol == "fractional")
        and "negative" in text
        and ("sign" in text or "proper" in text or "positive" in text or "data_preserving" in text)
    ):
        return "fractional-negative-proper-fraction-loses-sign"

    if (
        ("clamp" in text or symbol == "clamp")
        and (
            "boundary" in text
            or "floor" in text
            or "ceil" in text
            or "ceiling" in text
            or "exact" in text
            or "equality" in text
        )
    ):
        return "clamp-equal-boundary-marked-out-of-range"

    if (
        ("natural_list" in text or "natural list" in text or "naturallist" in text)
        and ("empty" in text or "none" in text or "[none]" in text)
    ):
        return "natural-list-empty-list-renders-none"

    if (
        ("naturaltime" in text or symbol == "naturaltime")
        and "future" in text
        and ("numeric" in text or "seconds" in text or "from now" in text or "positive" in title)
    ):
        return "naturaltime-numeric-future-flag-ignored"

    return None


def load_events(path: Path):
    if not path.exists():
        return []
    events = []
    with path.open("r", encoding="utf-8") as handle:
        for line in handle:
            line = line.strip()
            if not line:
                continue
            try:
                events.append(json.loads(line))
            except json.JSONDecodeError:
                events.append({"parse_error": True, "raw": line})
    return events


def parse_iso_datetime(value):
    if not isinstance(value, str) or not value:
        return None
    normalized = value.strip()
    if normalized.endswith("Z"):
        normalized = f"{normalized[:-1]}+00:00"
    try:
        parsed = datetime.fromisoformat(normalized)
    except ValueError:
        return None
    if parsed.tzinfo is None:
        parsed = parsed.replace(tzinfo=timezone.utc)
    return parsed.astimezone(timezone.utc)


def event_time(event):
    return parse_iso_datetime(
        event.get("created_at") or event.get("timestamp") or event.get("ts")
    )


def event_kind(event):
    return (event.get("event_type") or event.get("type") or event.get("event") or "").lower()


def seconds_between(start, end):
    if start is None or end is None:
        return None
    return round((end - start).total_seconds(), 3)


def event_payload_text(event):
    parts = [
        event_kind(event),
        str(event.get("title") or ""),
        str(
            event.get("detail")
            or event.get("summary")
            or event.get("message")
            or event.get("note")
            or ""
        ),
    ]
    for field in ("data", "details"):
        value = event.get(field)
        if value:
            parts.append(json.dumps(value, sort_keys=True, ensure_ascii=False))
    return " ".join(parts)


def token_count(payload, token):
    try:
        text = json.dumps(payload, sort_keys=True, ensure_ascii=False)
    except TypeError:
        text = text_of(payload)
    return text.lower().count(token)


def truthy(value):
    if isinstance(value, bool):
        return value
    if isinstance(value, str):
        return value.strip().lower() in {"1", "true", "yes", "y"}
    return bool(value)


def count_value(value):
    if isinstance(value, bool):
        return int(value)
    if isinstance(value, (int, float)):
        return int(value)
    if isinstance(value, (list, tuple, set)):
        return len(value)
    if isinstance(value, dict):
        return len(value)
    if isinstance(value, str):
        return 1 if value.strip() else 0
    return 0


def summarize_mission_control(result, agent_events):
    structured = result.get("mission_control_usage")
    structured = structured if isinstance(structured, dict) else {}
    raw_mission_payload = {
        "m1nd_usage": result.get("m1nd_usage"),
        "notes": result.get("notes"),
        "agent_testimony": result.get("agent_testimony") or result.get("testimony"),
        "events": agent_events,
    }
    summary = {
        "unavailable": truthy(structured.get("mission_control_unavailable"))
        or token_count(raw_mission_payload, "mission_control_unavailable") > 0,
        "loop_complete": False,
        "direct_proof_switch_count": max(
            count_value(structured.get("direct_proof_switches")),
            token_count(raw_mission_payload, "switch_to_direct_proof")
            + token_count(raw_mission_payload, "direct-proof switch")
            + token_count(raw_mission_payload, "direct proof switch"),
        ),
        "do_not_guardrail_count": max(
            count_value(structured.get("do_not_guardrails_observed")),
            token_count(raw_mission_payload, "do_not")
            + token_count(raw_mission_payload, "do not guardrail"),
        ),
        "verified_claim_signal_count": max(
            count_value(structured.get("verified_claims")),
            token_count(raw_mission_payload, "verified_claim")
            + token_count(raw_mission_payload, "claim_verified")
            + token_count(raw_mission_payload, "verdict=verified")
            + token_count(raw_mission_payload, '"verdict": "verified"'),
        ),
        "rejected_claim_signal_count": max(
            count_value(structured.get("rejected_or_insufficient_claims")),
            token_count(raw_mission_payload, "rejected_claim")
            + token_count(raw_mission_payload, "claim_rejected")
            + token_count(raw_mission_payload, "insufficient_evidence")
            + token_count(raw_mission_payload, '"verdict": "rejected"'),
        ),
    }
    for token in MISSION_CONTROL_TOKENS:
        summary[f"{token}_count"] = token_count(raw_mission_payload, token)
    summary["mission_start_count"] = max(
        summary["mission_start_count"], count_value(structured.get("mission_start_called"))
    )
    summary["mission_next_count"] = max(
        summary["mission_next_count"], count_value(structured.get("mission_next_count"))
    )
    summary["mission_verify_count"] = max(
        summary["mission_verify_count"], count_value(structured.get("mission_verify_count"))
    )
    summary["mission_close_count"] = max(
        summary["mission_close_count"], count_value(structured.get("mission_close_called"))
    )
    summary["required_step_count"] = MISSION_CONTROL_REQUIRED_STEP_COUNT
    summary["completed_step_count"] = sum(
        1 for token in MISSION_CONTROL_TOKENS if summary[f"{token}_count"] > 0
    )
    summary["adherence_rate"] = safe_rate(
        summary["completed_step_count"],
        summary["required_step_count"],
    )
    summary["loop_complete"] = (
        not summary["unavailable"]
        and all(summary[f"{token}_count"] > 0 for token in MISSION_CONTROL_TOKENS)
    )
    return summary


def event_matches_seeded_bug(event, answer_bug_payloads):
    return match_seeded_bug({"title": event_payload_text(event)}, answer_bug_payloads)


def summarize_event_timing(events, agent_events, answer_bug_payloads):
    timed_events = [(event_time(event), event) for event in events]
    timed_events = [(timestamp, event) for timestamp, event in timed_events if timestamp is not None]
    timed_agent_events = [
        (event_time(event), event)
        for event in agent_events
        if event_time(event) is not None
    ]

    first_event_time = min((timestamp for timestamp, _event in timed_events), default=None)
    first_agent_time = min((timestamp for timestamp, _event in timed_agent_events), default=None)
    last_agent_time = max((timestamp for timestamp, _event in timed_agent_events), default=None)

    finding_events = [
        (timestamp, event)
        for timestamp, event in timed_agent_events
        if event_kind(event) in FINDING_EVENT_TYPES
    ]
    first_finding_time = min((timestamp for timestamp, _event in finding_events), default=None)

    seeded_finding_events = [
        (timestamp, event)
        for timestamp, event in finding_events
        if event_matches_seeded_bug(event, answer_bug_payloads)
    ]
    first_seeded_finding_time = min(
        (timestamp for timestamp, _event in seeded_finding_events), default=None
    )

    return {
        "first_event_at": first_event_time.isoformat() if first_event_time else None,
        "first_agent_event_at": first_agent_time.isoformat() if first_agent_time else None,
        "last_agent_event_at": last_agent_time.isoformat() if last_agent_time else None,
        "agent_wall_clock_seconds": seconds_between(first_agent_time, last_agent_time),
        "assignment_to_first_agent_event_seconds": seconds_between(
            first_event_time, first_agent_time
        ),
        "first_finding_event_elapsed_seconds": seconds_between(
            first_agent_time, first_finding_time
        ),
        "first_seeded_finding_event_elapsed_seconds": seconds_between(
            first_agent_time, first_seeded_finding_time
        ),
        "timestamped_event_count": len(timed_events),
        "timestamped_agent_event_count": len(timed_agent_events),
    }


def summarize_lane(round_dir: Path, lane, answer_bug_ids, answer_bug_payloads):
    result_path = Path(lane["result"])
    if not result_path.is_absolute():
        result_path = round_dir / result_path
    events_path = Path(lane.get("events", ""))
    if events_path and not events_path.is_absolute():
        events_path = round_dir / events_path

    if not result_path.exists():
        return {
            "lane_id": lane["lane_id"],
            "instruction_mode": lane["instruction_mode"],
            "completed": False,
            "missing_result": True,
            "seeded_recall_count": 0,
            "seeded_recall_rate": 0.0,
            "matched_seeded_bug_ids": [],
            "findings_count": 0,
            "extra_unadjudicated_findings_count": 0,
        }

    result = load_json(result_path)
    findings = result.get("findings") or []
    matched_by_finding = []
    matched_ids = []
    for index, finding in enumerate(findings):
        bug_id = match_seeded_bug(finding, answer_bug_payloads)
        if bug_id in answer_bug_ids:
            matched_ids.append(bug_id)
            matched_by_finding.append(
                {
                    "finding_index": index,
                    "finding_title": finding.get("title", ""),
                    "seeded_bug_id": bug_id,
                }
            )

    unique_matches = sorted(set(matched_ids))
    events = load_events(events_path) if events_path else []
    agent_events = [
        event
        for event in events
        if event.get("event_source") == "agent" or event.get("source") == "agent"
    ]
    m1nd_usage = result.get("m1nd_usage")
    event_timing = summarize_event_timing(events, agent_events, answer_bug_payloads)
    mission_control = summarize_mission_control(result, agent_events)

    return {
        "lane_id": lane["lane_id"],
        "instruction_mode": lane["instruction_mode"],
        "completed": result.get("schema") == LANE_SCHEMA,
        "result_schema": result.get("schema"),
        "result_path": str(result_path),
        "events_path": str(events_path) if events_path else None,
        "findings_count": len(findings),
        "matched_seeded_bug_ids": unique_matches,
        "matched_findings": matched_by_finding,
        "seeded_recall_count": len(unique_matches),
        "seeded_recall_rate": safe_rate(len(unique_matches), len(answer_bug_ids)),
        "missed_seeded_bug_ids": sorted(set(answer_bug_ids) - set(unique_matches)),
        "extra_unadjudicated_findings_count": max(0, len(findings) - len(matched_by_finding)),
        "event_count": len(events),
        "agent_event_count": len(agent_events),
        **event_timing,
        "m1nd_usage_count": len(m1nd_usage) if isinstance(m1nd_usage, list) else None,
        "mission_control": mission_control,
        "agent_testimony": result.get("agent_testimony") or result.get("testimony") or "",
    }


def group_arms(lanes, seeded_bug_count):
    grouped = defaultdict(list)
    for lane in lanes:
        grouped[lane["instruction_mode"]].append(lane)

    arms = {}
    for arm, arm_lanes in sorted(grouped.items()):
        completed = [lane for lane in arm_lanes if lane.get("completed")]
        recall_counts = [lane["seeded_recall_count"] for lane in completed]
        possible = seeded_bug_count * len(completed)
        matched_total = sum(recall_counts)
        arms[arm] = {
            "lane_count": len(arm_lanes),
            "completed_lane_count": len(completed),
            "seeded_bug_count_per_lane": seeded_bug_count,
            "seeded_recall_total": matched_total,
            "seeded_possible_total": possible,
            "seeded_recall_rate": safe_rate(matched_total, possible),
            "per_lane_seeded_recall_counts": recall_counts,
            "median_seeded_recall_count": median(recall_counts),
            "average_seeded_recall_count": average(recall_counts),
            "median_agent_wall_clock_seconds": median(
                lane.get("agent_wall_clock_seconds") for lane in completed
            ),
            "median_first_finding_event_elapsed_seconds": median(
                lane.get("first_finding_event_elapsed_seconds") for lane in completed
            ),
            "median_first_seeded_finding_event_elapsed_seconds": median(
                lane.get("first_seeded_finding_event_elapsed_seconds")
                for lane in completed
            ),
            "total_findings": sum(lane["findings_count"] for lane in completed),
            "extra_unadjudicated_findings_total": sum(
                lane["extra_unadjudicated_findings_count"] for lane in completed
            ),
            "mission_control_loop_complete_lanes": sum(
                1 for lane in completed if lane.get("mission_control", {}).get("loop_complete")
            ),
            "mission_control_unavailable_lanes": sum(
                1 for lane in completed if lane.get("mission_control", {}).get("unavailable")
            ),
            "median_mission_next_count": median(
                lane.get("mission_control", {}).get("mission_next_count")
                for lane in completed
            ),
            "median_direct_proof_switch_count": median(
                lane.get("mission_control", {}).get("direct_proof_switch_count")
                for lane in completed
            ),
            "median_mission_control_adherence_rate": median(
                lane.get("mission_control", {}).get("adherence_rate")
                for lane in completed
            ),
            "lanes": [lane["lane_id"] for lane in arm_lanes],
        }
    return arms


def build_report(round_file: Path, answer_key_file: Path, lane_results_dir: Path):
    round_file = round_file.resolve()
    answer_key_file = answer_key_file.resolve()
    lane_results_dir = lane_results_dir.resolve()
    round_payload = load_json(round_file)
    answer_key = load_json(answer_key_file)
    round_dir = round_file.parent

    if round_payload.get("schema") != ROUND_SCHEMA:
        raise SystemExit(f"unexpected round schema: {round_payload.get('schema')}")
    if answer_key.get("schema") != ANSWER_KEY_SCHEMA:
        raise SystemExit(f"unexpected answer-key schema: {answer_key.get('schema')}")

    answer_bug_payloads = answer_key["bugs"]
    answer_bug_ids = [bug["id"] for bug in answer_bug_payloads]
    lanes = []
    for lane in round_payload["lanes"]:
        lane = dict(lane)
        result_name = Path(lane["result"]).name
        lane["result"] = str(lane_results_dir / result_name)
        lanes.append(summarize_lane(round_dir, lane, answer_bug_ids, answer_bug_payloads))

    arms = group_arms(lanes, len(answer_bug_ids))
    completed_lanes = [lane for lane in lanes if lane.get("completed")]
    arm_lane_counts = {arm: payload["completed_lane_count"] for arm, payload in arms.items()}
    public_claim_blockers = [
        "single internal round",
        "one fixture repo",
        "seeded recall only; extra findings are not independently judged",
    ]
    if len(set(arm_lane_counts.values())) != 1:
        public_claim_blockers.append(
            "instruction-mode lane counts are not balanced; compare rates rather than raw totals"
        )

    return {
        "schema": REPORT_SCHEMA,
        "round_id": round_payload["round_id"],
        "generated_at": now_iso(),
        "repo": round_payload["repo"],
        "source_commit": round_payload.get("source_commit"),
        "seeded_bug_count": len(answer_bug_ids),
        "seeded_bug_ids": answer_bug_ids,
        "lanes_completed": len(completed_lanes),
        "lanes_expected": len(round_payload["lanes"]),
        "lanes": lanes,
        "arms": arms,
        "comparability": {
            "all_lane_results_present": len(completed_lanes) == len(round_payload["lanes"]),
            "primary_arm_lane_counts": arm_lane_counts,
            "rate_comparison_available": all(
                payload["completed_lane_count"] > 0 for payload in arms.values()
            ),
            "balanced_lane_counts": len(set(arm_lane_counts.values())) == 1,
            "comparability_notes": [
                "Compare rates rather than raw totals when arm lane counts differ.",
                "Extra findings are unadjudicated and are not used as precision penalties.",
            ],
        },
        "top_line": {
            arm: {
                "seeded_recall_rate": payload["seeded_recall_rate"],
                "seeded_recall_total": payload["seeded_recall_total"],
                "seeded_possible_total": payload["seeded_possible_total"],
                "median_seeded_recall_count": payload["median_seeded_recall_count"],
            }
            for arm, payload in arms.items()
        },
        "invalidated_attempts": round_payload.get("invalidated_attempts", []),
        "public_claim_worthy": False,
        "public_claim_blockers": public_claim_blockers,
        "non_claims": sorted(set(NON_CLAIMS + answer_key.get("non_claims", []))),
    }


def write_notes(path: Path, report):
    lines = [
        f"# Bug Hunt Round Notes: {report['round_id']}",
        "",
        "Status: internal product learning, not public benchmark copy.",
        "",
        "## Result",
        "",
    ]

    for arm, payload in report["arms"].items():
        rate = payload["seeded_recall_rate"]
        rate_text = f"{rate * 100:.1f}%" if rate is not None else "n/a"
        lines.append(
            f"- `{arm}`: {payload['seeded_recall_total']}/{payload['seeded_possible_total']} seeded bugs found ({rate_text}); "
            f"per-lane counts `{payload['per_lane_seeded_recall_counts']}`."
        )
        if arm == "m1nd-mission-control" or payload["mission_control_loop_complete_lanes"]:
            lines.append(
                f"  Mission Control: loop-complete lanes `{payload['mission_control_loop_complete_lanes']}/{payload['completed_lane_count']}`, "
                f"unavailable lanes `{payload['mission_control_unavailable_lanes']}`, "
                f"median `mission_next` count `{payload['median_mission_next_count']}`, "
                f"median direct-proof switches `{payload['median_direct_proof_switch_count']}`, "
                f"median adherence `{payload['median_mission_control_adherence_rate']}`."
            )

    lines.extend(
        [
            "",
            "## Interpretation",
            "",
            "Read this as an internal product-learning artifact, not a public scoreboard. The useful comparison is between instruction modes that received the same seeded repo and the same answer key.",
            "",
            "The strongest recurring signal is not simply \"m1nd on\" versus \"m1nd off\". It is whether the agent has a compact, correct operating loop: trust check, scoped recovery, graph orientation, direct source/test proof, and honest fallback when retrieval is blocked.",
            "",
            "If a Tempo/TEMPONIZER mode is present, interpret it as prompt-integration evidence too. Temporal recalibration should reduce inherited human-duration bias and improve decision quality, but an over-heavy checklist can add enough cognitive overhead to reduce bug recall.",
            "",
            "## Caveats",
            "",
            "- This is one internal round on one fixture repo.",
            "- Extra findings were preserved but not independently judged.",
            "- This report measures seeded recall, not total bug discovery quality.",
            "",
            "## Next Product Actions",
            "",
            "- Keep improving the compact trained-agent loop as a default universal agent pack behavior.",
            "- Add cleaner state placement so m1nd benchmark/probe flows do not write sidecar metadata into target repos.",
            "- Track first-good-finding time and tool-call counts in the event stream.",
            "- Add a judge pass for extra findings so future reports can separate true extras from noise.",
            "",
        ]
    )
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text("\n".join(lines), encoding="utf-8")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    subparsers = parser.add_subparsers(dest="command", required=True)

    init_parser = subparsers.add_parser("init")
    init_parser.add_argument("--out-dir", required=True, type=Path)
    init_parser.add_argument("--round-id", required=True)
    init_parser.add_argument("--repo", required=True)
    init_parser.add_argument("--source-repo", type=Path)
    init_parser.add_argument("--seeded-repo", type=Path)
    init_parser.add_argument("--workspace-root", type=Path)
    init_parser.add_argument("--source-commit")
    init_parser.add_argument("--seeded-bug-count", type=int, default=5)
    init_parser.add_argument("--lanes-full-spec", type=int, default=0)
    init_parser.add_argument("--lanes-temponizer-full", type=int, default=0)
    init_parser.add_argument("--lanes-temponizer-compact", type=int, default=0)
    init_parser.add_argument("--lanes-temponizer", type=int, default=0)
    init_parser.add_argument("--lanes-short-audit", type=int, default=0)
    init_parser.add_argument("--lanes-mission-control", type=int, default=0)
    init_parser.add_argument("--lanes-trained", type=int, default=3)
    init_parser.add_argument("--lanes-basic", type=int, default=3)
    init_parser.add_argument("--lanes-direct", type=int, default=3)
    init_parser.add_argument("--json", action="store_true")

    score_parser = subparsers.add_parser("score")
    score_parser.add_argument("--round-file", required=True, type=Path)
    score_parser.add_argument("--answer-key", required=True, type=Path)
    score_parser.add_argument("--lane-results-dir", required=True, type=Path)
    score_parser.add_argument("--output", required=True, type=Path)
    score_parser.add_argument("--notes", type=Path)
    score_parser.add_argument("--json", action="store_true")

    args = parser.parse_args()

    if args.command == "init":
        round_payload = init_round(args)
        if args.json:
            print(
                json.dumps(
                    {
                        "ok": True,
                        "round": str((args.out_dir / "round.json").resolve()),
                        "lane_count": len(round_payload["lanes"]),
                    },
                    indent=2,
                )
            )
    elif args.command == "score":
        report = build_report(args.round_file, args.answer_key, args.lane_results_dir)
        dump_json(args.output, report)
        if args.notes:
            write_notes(args.notes, report)
        if args.json:
            print(json.dumps({"ok": True, "report": str(args.output)}, indent=2))


if __name__ == "__main__":
    main()
