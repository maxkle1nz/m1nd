#!/usr/bin/env python3
"""Score blinded bug-hunt benchmark rounds.

Bug-hunt rounds measure seeded defect recall in realistic repo audits. They are
not precision claims: agents may report extra real issues, but only seeded bugs
from the operator-only answer key are adjudicated by this scorer.
"""

import argparse
import json
import os
import shutil
import statistics
import subprocess
from collections import defaultdict
from datetime import datetime, timezone
from pathlib import Path


ROUND_SCHEMA = "m1nd-bug-hunt-round-v0"
LANE_SCHEMA = "m1nd-bug-hunt-audit-result-v0"
ANSWER_KEY_SCHEMA = "m1nd-bug-hunt-answer-key-v0"
REPORT_SCHEMA = "m1nd-bug-hunt-report-v0"
PREFLIGHT_SCHEMA = "m1nd-bug-hunt-round-preflight-v0"
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
    "mission_event",
    "mission_next",
    "mission_verify",
    "mission_handoff",
    "mission_close",
)
MISSION_CONTROL_REQUIRED_STEP_COUNT = len(MISSION_CONTROL_TOKENS)
TOOL_USAGE_COUNT_FIELDS = (
    "shell_command_count",
    "file_read_count",
    "test_or_probe_count",
    "runtime_probe_count",
    "m1nd_call_count",
    "mission_control_call_count",
    "total_observed_action_count",
)

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
    mission_runtime_dir = str(
        Path(lane["events"]).parent.parent / "m1nd-runtime" / lane["lane_id"]
    )
    mission_binary = round_payload.get("mission_control_binary")
    mission_binary_arg = f" --binary {mission_binary}" if mission_binary else ""
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
            "Use Mission Control v1 as the operating loop for this audit.",
            "Mission Control is not a replacement for source reads, tests, compiler output, or runtime proof.",
            "",
            "Required operating loop:",
            "",
            "1. Establish trust with `trust_selftest`, or `session_handshake` scoped to this repo.",
            f"2. Prefer the isolated helper runtime for Mission Control benchmark calls, even when native host tools are visible: `probe_m1nd.py{mission_binary_arg} --no-worktree-artifacts --runtime-dir {mission_runtime_dir} --workspace-root <repo> call <tool> <json>`. This keeps mission state scoped to this lane and avoids a host graph that may contain benchmark operator-only artifacts.",
            "3. Use the native host MCP surface only if `trust_selftest` or `session_handshake` proves the active workspace binding is exactly this lane workspace and the host exposes `mission_start`, `mission_event`, `mission_next`, `mission_verify`, `mission_handoff`, and `mission_close`. Record `mission_transport=\"native_host_mcp\"` when you do.",
            "4. Record `mission_control_unavailable=true` only when neither the native host surface nor the helper surface can call Mission Control. Then fall back to the `m1nd-trained` loop and do not fake mission calls.",
            "5. Start a repo-scoped mission with `mission_start`: `agent_id=<lane_id>`, `repo=<workspace>`, `task=\"bug-hunt audit for behavioral defects\"`, `mode=\"bug_hunt\"`, `budget=\"normal\"`, and `risk=\"medium\"`.",
            "6. Take the starter move, record meaningful actions with `mission_event`, then call `mission_next` after meaningful events.",
            "7. Treat `do_not` entries from `mission_next` as guardrails. If you disagree, record a dissent event explaining the chosen tool and required evidence.",
            "8. When `mission_next` switches to direct proof, stop graph exploration and use direct source reads, rg, tests, compiler output, or focused runtime probes.",
            "9. Call `mission_verify` before finalizing material findings. Reference direct evidence explicitly, such as `event:evt_1`, `file_read:path:line`, `test_run:name`, or `runtime_probe:id`; an unrelated direct event must not validate a graph-only claim.",
            "10. In `bug_hunt`, if `mission_next` returns `move.type=\"direct_sweep\"`, do the requested negative-space sweep before closing: public contracts/docs, boundary values, error paths, async/concurrency behavior, and helper/exported APIs. Record it as `coverage_sweep`, `boundary_sweep`, or `edge_case_sweep`.",
            "11. Call `mission_handoff` before final result writing so the lane has a resumable packet.",
            "12. Call `mission_close` before writing the final lane JSON; preserve gaps, non-claims, event digest, and proof-packet summary.",
            "13. Fill `mission_control_usage` in the lane result with `mission_id`, route, transport, call counts, unavailable state, `do_not` guardrails, verified/rejected claims, direct-proof switches, coverage sweeps, event digest, handoff summary, and proof-packet summary.",
            "14. Also preserve raw m1nd calls in `m1nd_usage` when useful for auditability.",
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
        "Fill `tool_usage_summary` with approximate actual action counts: shell commands, file reads, tests/probes, runtime probes, m1nd calls, Mission Control calls, total observed actions, and notes about any uncertainty.",
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
        "tool_usage_summary": {
            "shell_command_count": 0,
            "file_read_count": 0,
            "test_or_probe_count": 0,
            "runtime_probe_count": 0,
            "m1nd_call_count": 0,
            "mission_control_call_count": 0,
            "total_observed_action_count": 0,
            "notes": "",
        },
        "m1nd_usage": [],
        "mission_control_usage": {
            "mission_id": "",
            "mission_route": "",
            "mission_transport": "",
            "mission_tool_surface_count": None,
            "mission_control_unavailable": False,
            "mission_start_called": False,
            "mission_event_count": 0,
            "mission_next_count": 0,
            "mission_verify_count": 0,
            "mission_handoff_called": False,
            "mission_close_called": False,
            "do_not_guardrails_observed": [],
            "verified_claims": [],
            "rejected_or_insufficient_claims": [],
            "direct_proof_switches": [],
            "coverage_sweeps": [],
            "event_digest": "",
            "handoff_summary": "",
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


def materialize_lane_workspaces(lanes, seeded_repo: Path | None, enabled: bool):
    if not enabled or seeded_repo is None:
        return []
    if not seeded_repo.exists() or not seeded_repo.is_dir():
        raise FileNotFoundError(f"seeded repo not found: {seeded_repo}")
    materialized = []
    for lane in lanes:
        repo_path = Path(lane["repo_path"])
        repo_path.parent.mkdir(parents=True, exist_ok=True)
        if repo_path.exists():
            materialized.append({"lane_id": lane["lane_id"], "repo_path": str(repo_path), "status": "existing"})
            continue
        shutil.copytree(seeded_repo, repo_path)
        materialized.append({"lane_id": lane["lane_id"], "repo_path": str(repo_path), "status": "created"})
    return materialized


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

    materialized_workspaces = materialize_lane_workspaces(
        lanes,
        args.seeded_repo.resolve() if args.seeded_repo else None,
        not getattr(args, "no_materialize_workspaces", False),
    )
    non_claims = [
        "Primary auditors are not told bug count or comparison arm.",
        "This is an internal field simulation, not public benchmark evidence.",
    ]
    if materialized_workspaces:
        non_claims.append(
            "The init command copied the seeded repo into lane workspaces; it does not validate seeded bug quality."
        )
    else:
        non_claims.append(
            "The init command created scaffolding only; the operator must prepare seeded workspaces and answer key."
        )

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
        "mission_control_binary": (
            str(Path(args.mission_control_binary).resolve())
            if getattr(args, "mission_control_binary", None)
            else None
        ),
        "lanes": lanes,
        "materialized_workspaces": materialized_workspaces,
        "invalidated_attempts": [],
        "non_claims": non_claims,
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


def resolve_round_artifact(round_dir: Path, value):
    path = Path(value)
    if path.is_absolute():
        return path
    return round_dir / path


def default_m1nd_binary():
    env_binary = os.environ.get("M1ND_MCP_BINARY") or os.environ.get("M1ND_MCP_BIN")
    if env_binary:
        return env_binary
    binary_name = "m1nd-mcp.exe" if os.name == "nt" else "m1nd-mcp"
    managed_binary = Path.home() / ".m1nd" / "bin" / binary_name
    if managed_binary.exists() and os.access(managed_binary, os.X_OK):
        return str(managed_binary)
    return shutil.which(binary_name)


def rpc_request(process, payload):
    process.stdin.write(json.dumps(payload) + "\n")
    process.stdin.flush()
    line = process.stdout.readline()
    if not line:
        stderr = process.stderr.read().strip() if process.stderr else ""
        raise RuntimeError(f"no response from m1nd-mcp; stderr={stderr}")
    return json.loads(line)


def probe_m1nd_tool_surface(binary=None):
    required_tools = list(MISSION_CONTROL_TOKENS)
    selected_binary = binary or default_m1nd_binary()
    if not selected_binary:
        return {
            "status": "missing_binary",
            "binary": None,
            "tool_count": 0,
            "required_tools": required_tools,
            "missing_required_tools": required_tools,
            "mission_control_available": False,
        }

    process = None
    try:
        process = subprocess.Popen(
            [selected_binary, "--stdio", "--no-gui"],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
        )
        rpc_request(process, {"jsonrpc": "2.0", "id": 0, "method": "initialize", "params": {}})
        tools_response = rpc_request(
            process, {"jsonrpc": "2.0", "id": 1, "method": "tools/list", "params": {}}
        )
        tools = tools_response.get("result", {}).get("tools", [])
        names = sorted(
            tool.get("name")
            for tool in tools
            if isinstance(tool, dict) and isinstance(tool.get("name"), str)
        )
        missing = [tool for tool in required_tools if tool not in names]
        return {
            "status": "ok" if not missing else "missing_required_tools",
            "binary": selected_binary,
            "tool_count": len(names),
            "required_tools": required_tools,
            "missing_required_tools": missing,
            "mission_control_available": not missing,
        }
    except Exception as error:  # noqa: BLE001 - preflight reports tool surface failures.
        return {
            "status": "probe_failed",
            "binary": selected_binary,
            "tool_count": 0,
            "required_tools": required_tools,
            "missing_required_tools": required_tools,
            "mission_control_available": False,
            "error": str(error),
        }
    finally:
        if process and process.poll() is None:
            process.terminate()
            try:
                process.wait(timeout=2)
            except subprocess.TimeoutExpired:
                process.kill()
        if process:
            for stream in (process.stdin, process.stdout, process.stderr):
                if stream:
                    stream.close()


def preflight_round(
    round_file: Path,
    required_modes=None,
    require_live_mission_control=False,
    m1nd_binary=None,
):
    round_file = round_file.resolve()
    round_dir = round_file.parent
    required_modes = required_modes or []
    payload = load_json(round_file)

    blockers = []
    warnings = []
    lanes = payload.get("lanes") or []
    lane_ids = [lane.get("lane_id") for lane in lanes]
    modes = [lane.get("instruction_mode") for lane in lanes]
    duplicate_lane_ids = sorted(
        lane_id for lane_id in set(lane_ids) if lane_ids.count(lane_id) > 1
    )
    invalid_modes = sorted(set(mode for mode in modes if mode not in VALID_INSTRUCTION_MODES))
    missing_required_modes = [
        mode for mode in required_modes if mode not in set(modes)
    ]

    if payload.get("schema") != ROUND_SCHEMA:
        blockers.append(f"unexpected round schema: {payload.get('schema')}")
    if not lanes:
        blockers.append("round has no lanes")
    if duplicate_lane_ids:
        blockers.append(f"duplicate lane ids: {duplicate_lane_ids}")
    if invalid_modes:
        blockers.append(f"invalid instruction modes: {invalid_modes}")
    if missing_required_modes:
        blockers.append(f"missing required instruction modes: {missing_required_modes}")

    answer_key_path = round_dir / "operator-only" / "answer-key.json"
    if not answer_key_path.exists():
        blockers.append("missing operator-only answer key")
    else:
        answer_key = load_json(answer_key_path)
        answer_bugs = answer_key.get("bugs")
        if answer_key.get("schema") != ANSWER_KEY_SCHEMA:
            blockers.append(
                f"unexpected answer-key schema: {answer_key.get('schema')}"
            )
        if not isinstance(answer_bugs, list) or not answer_bugs:
            blockers.append("operator-only answer key has no bugs")
        elif len(answer_bugs) != payload.get("seeded_bug_count"):
            blockers.append(
                "answer-key bug count does not match seeded_bug_count: "
                f"{len(answer_bugs)} != {payload.get('seeded_bug_count')}"
            )

    mission_control_surface = None
    if require_live_mission_control and "m1nd-mission-control" in modes:
        mission_control_surface = probe_m1nd_tool_surface(binary=m1nd_binary)
        if not mission_control_surface["mission_control_available"]:
            blockers.append(
                "live Mission Control tools unavailable: "
                f"{mission_control_surface['missing_required_tools']}"
            )

    lane_reports = []
    for lane in lanes:
        lane_id = lane.get("lane_id")
        mode = lane.get("instruction_mode")
        prompt_path = resolve_round_artifact(round_dir, lane.get("prompt", ""))
        result_path = resolve_round_artifact(round_dir, lane.get("result", ""))
        events_path = resolve_round_artifact(round_dir, lane.get("events", ""))
        lane_blockers = []
        prompt_text = ""
        result_payload = None

        if not prompt_path.exists():
            lane_blockers.append("missing prompt")
        else:
            prompt_text = prompt_path.read_text(encoding="utf-8")
        if not result_path.exists():
            lane_blockers.append("missing result template")
        else:
            result_payload = load_json(result_path)
            if result_payload.get("schema") != LANE_SCHEMA:
                lane_blockers.append("unexpected result schema")
            if result_payload.get("instruction_mode") != mode:
                lane_blockers.append("result instruction_mode does not match round lane")
        if not events_path.exists():
            lane_blockers.append("missing event stream")

        if mode == "m1nd-mission-control":
            required_prompt_tokens = [
                "mission_control_usage",
                "mission_start",
                "mission_event",
                "mission_next",
                "mission_verify",
                "mission_handoff",
                "mission_close",
                "direct_sweep",
                "do not fake mission calls",
            ]
            missing_prompt_tokens = [
                token for token in required_prompt_tokens if token not in prompt_text
            ]
            if missing_prompt_tokens:
                lane_blockers.append(
                    f"mission-control prompt missing tokens: {missing_prompt_tokens}"
                )
            usage = (
                result_payload.get("mission_control_usage")
                if isinstance(result_payload, dict)
                else None
            )
            if not isinstance(usage, dict):
                lane_blockers.append("missing structured mission_control_usage")
            else:
                expected_fields = [
                    "mission_id",
                    "mission_route",
                    "mission_control_unavailable",
                    "mission_start_called",
                    "mission_event_count",
                    "mission_next_count",
                    "mission_verify_count",
                    "mission_handoff_called",
                    "mission_close_called",
                    "do_not_guardrails_observed",
                    "verified_claims",
                    "rejected_or_insufficient_claims",
                    "direct_proof_switches",
                    "coverage_sweeps",
                    "event_digest",
                    "handoff_summary",
                    "proof_packet_summary",
                ]
                missing_fields = [field for field in expected_fields if field not in usage]
                if missing_fields:
                    lane_blockers.append(
                        f"mission_control_usage missing fields: {missing_fields}"
                    )
        if lane_blockers:
            blockers.extend(f"{lane_id}: {item}" for item in lane_blockers)
        lane_reports.append(
            {
                "lane_id": lane_id,
                "instruction_mode": mode,
                "prompt_exists": prompt_path.exists(),
                "result_exists": result_path.exists(),
                "events_exists": events_path.exists(),
                "blockers": lane_blockers,
                "warnings": [],
            }
        )

    arm_lane_counts = {
        mode: modes.count(mode)
        for mode in sorted(set(modes))
        if mode in VALID_INSTRUCTION_MODES
    }
    return {
        "schema": PREFLIGHT_SCHEMA,
        "round_id": payload.get("round_id"),
        "generated_at": now_iso(),
        "ok": not blockers,
        "round_file": str(round_file),
        "lane_count": len(lanes),
        "arm_lane_counts": arm_lane_counts,
        "required_modes": required_modes,
        "blockers": blockers,
        "warnings": warnings,
        "lanes": lane_reports,
        "mission_control_surface": mission_control_surface,
        "non_claims": [
            "preflight checks scaffold shape only; it does not run agents",
            "preflight does not prove seeded bug quality or benchmark outcome",
            "live Mission Control preflight probes only the selected binary; it does not prove already-open worker host cached tool lists",
        ],
    }


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


def nonnegative_count(value):
    if isinstance(value, bool):
        return int(value)
    if isinstance(value, (int, float)) and value >= 0:
        return int(value)
    if isinstance(value, str):
        try:
            parsed = float(value.strip())
        except ValueError:
            return None
        if parsed >= 0:
            return int(parsed)
    return None


def count_commands_matching(commands, needles):
    if not isinstance(commands, list):
        return 0
    count = 0
    for command in commands:
        text = text_of(command).lower()
        if any(needle in text for needle in needles):
            count += 1
    return count


def finding_is_source_backed(finding):
    if not isinstance(finding, dict):
        return False
    return all(
        bool(text_of(finding.get(field)).strip())
        for field in ("file", "evidence", "reproduction_or_test")
    )


def summarize_tool_usage(result, agent_events, mission_control, event_timing):
    structured = result.get("tool_usage_summary")
    has_structured = isinstance(structured, dict)
    structured = structured if has_structured else {}

    commands = result.get("commands_run")
    files = result.get("files_inspected")
    m1nd_usage = result.get("m1nd_usage")

    command_count = len(commands) if isinstance(commands, list) else 0
    file_count = len(files) if isinstance(files, list) else 0
    m1nd_count = len(m1nd_usage) if isinstance(m1nd_usage, list) else 0
    mission_count = 0
    if mission_control.get("applicable"):
        mission_count = sum(
            mission_control.get(f"{token}_count") or 0
            for token in MISSION_CONTROL_TOKENS
        )

    event_runtime_probe_count = sum(
        1
        for event in agent_events
        if event_kind(event)
        in {"runtime_probe", "runtime_probes_completed", "focused_probes", "probe_result"}
    )
    command_probe_count = count_commands_matching(
        commands,
        (
            "pytest",
            "unittest",
            "python3",
            "python -",
            "cargo test",
            "npm test",
            "runtime probe",
            "focused probe",
        ),
    )

    fallbacks = {
        "shell_command_count": command_count,
        "file_read_count": file_count,
        "test_or_probe_count": max(command_probe_count, event_runtime_probe_count),
        "runtime_probe_count": event_runtime_probe_count,
        "m1nd_call_count": m1nd_count,
        "mission_control_call_count": mission_count,
    }
    summary = {}
    for field, fallback in fallbacks.items():
        declared = nonnegative_count(structured.get(field))
        if declared is not None and (declared > 0 or fallback == 0):
            summary[field] = declared
        else:
            summary[field] = fallback

    total = nonnegative_count(structured.get("total_observed_action_count"))
    fallback_total = sum(summary[field] for field in fallbacks)
    if total is None or (total == 0 and fallback_total > 0):
        total = fallback_total
    summary["total_observed_action_count"] = total
    summary["first_good_finding_seconds"] = (
        event_timing.get("first_seeded_finding_event_elapsed_seconds")
        if event_timing.get("first_seeded_finding_event_elapsed_seconds") is not None
        else event_timing.get("first_finding_event_elapsed_seconds")
    )
    summary["structured_declared"] = has_structured
    summary["notes"] = text_of(structured.get("notes")).strip()
    return summary


def summarize_mission_control(result, agent_events, instruction_mode):
    if instruction_mode != "m1nd-mission-control":
        return {
            "applicable": False,
            "unavailable": None,
            "loop_complete": None,
            "direct_proof_switch_count": None,
            "coverage_sweep_count": None,
            "do_not_guardrail_count": None,
            "verified_claim_signal_count": None,
            "rejected_claim_signal_count": None,
            "mission_start_count": None,
            "mission_event_count": None,
            "mission_next_count": None,
            "mission_verify_count": None,
            "mission_handoff_count": None,
            "mission_close_count": None,
            "event_digest_present": None,
            "handoff_summary_present": None,
            "proof_packet_summary_present": None,
            "evidence_packet_complete": None,
            "required_step_count": None,
            "completed_step_count": None,
            "adherence_rate": None,
        }

    structured_payload = result.get("mission_control_usage")
    has_structured_payload = isinstance(structured_payload, dict)
    structured = structured_payload if has_structured_payload else {}
    raw_mission_payload = {
        "m1nd_usage": result.get("m1nd_usage"),
        "notes": result.get("notes"),
        "agent_testimony": result.get("agent_testimony") or result.get("testimony"),
        "events": agent_events,
    }
    unavailable = truthy(structured.get("mission_control_unavailable"))
    if not has_structured_payload:
        unavailable = unavailable or token_count(raw_mission_payload, "mission_control_unavailable") > 0

    summary = {
        "applicable": True,
        "unavailable": unavailable,
        "loop_complete": False,
        "direct_proof_switch_count": count_value(structured.get("direct_proof_switches")),
        "coverage_sweep_count": count_value(structured.get("coverage_sweeps")),
        "do_not_guardrail_count": count_value(structured.get("do_not_guardrails_observed")),
        "verified_claim_signal_count": count_value(structured.get("verified_claims")),
        "rejected_claim_signal_count": count_value(
            structured.get("rejected_or_insufficient_claims")
        ),
    }
    if has_structured_payload:
        summary["mission_start_count"] = count_value(structured.get("mission_start_called"))
        summary["mission_event_count"] = count_value(structured.get("mission_event_count"))
        summary["mission_next_count"] = count_value(structured.get("mission_next_count"))
        summary["mission_verify_count"] = count_value(structured.get("mission_verify_count"))
        summary["mission_handoff_count"] = count_value(
            structured.get("mission_handoff_called")
        )
        summary["mission_close_count"] = count_value(structured.get("mission_close_called"))
        summary["event_digest_present"] = bool(
            text_of(structured.get("event_digest")).strip()
        )
        summary["handoff_summary_present"] = bool(
            text_of(structured.get("handoff_summary")).strip()
        )
        summary["proof_packet_summary_present"] = bool(
            text_of(structured.get("proof_packet_summary")).strip()
        )
    else:
        for token in MISSION_CONTROL_TOKENS:
            summary[f"{token}_count"] = token_count(raw_mission_payload, token)
        summary["direct_proof_switch_count"] = token_count(
            raw_mission_payload, "switch_to_direct_proof"
        ) + token_count(raw_mission_payload, "direct-proof switch") + token_count(
            raw_mission_payload, "direct proof switch"
        )
        summary["coverage_sweep_count"] = (
            token_count(raw_mission_payload, "direct_sweep")
            + token_count(raw_mission_payload, "coverage_sweep")
            + token_count(raw_mission_payload, "boundary_sweep")
            + token_count(raw_mission_payload, "edge_case_sweep")
            + token_count(raw_mission_payload, "negative-space sweep")
        )
        summary["do_not_guardrail_count"] = token_count(
            raw_mission_payload, "do_not"
        ) + token_count(raw_mission_payload, "do not guardrail")
        summary["verified_claim_signal_count"] = (
            token_count(raw_mission_payload, "verified_claim")
            + token_count(raw_mission_payload, "claim_verified")
            + token_count(raw_mission_payload, "verdict=verified")
            + token_count(raw_mission_payload, '"verdict": "verified"')
        )
        summary["rejected_claim_signal_count"] = (
            token_count(raw_mission_payload, "rejected_claim")
            + token_count(raw_mission_payload, "claim_rejected")
            + token_count(raw_mission_payload, "insufficient_evidence")
            + token_count(raw_mission_payload, '"verdict": "rejected"')
        )
        summary["event_digest_present"] = token_count(
            raw_mission_payload, "event_digest"
        ) > 0
        summary["handoff_summary_present"] = token_count(
            raw_mission_payload, "handoff"
        ) > 0
        summary["proof_packet_summary_present"] = token_count(
            raw_mission_payload, "proof_packet"
        ) > 0

    summary["required_step_count"] = MISSION_CONTROL_REQUIRED_STEP_COUNT
    if summary["unavailable"]:
        summary["completed_step_count"] = 0
    else:
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
    summary["evidence_packet_complete"] = (
        summary["loop_complete"]
        and summary["event_digest_present"]
        and summary["handoff_summary_present"]
        and summary["proof_packet_summary_present"]
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
    mission_control = summarize_mission_control(result, agent_events, lane["instruction_mode"])
    tool_usage = summarize_tool_usage(result, agent_events, mission_control, event_timing)
    source_backed_finding_count = sum(1 for finding in findings if finding_is_source_backed(finding))

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
        "tool_usage": tool_usage,
        "source_backed_finding_count": source_backed_finding_count,
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
        mission_completed = [
            lane
            for lane in completed
            if lane.get("mission_control", {}).get("applicable")
        ]
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
            "median_first_good_finding_seconds": median(
                lane.get("tool_usage", {}).get("first_good_finding_seconds")
                for lane in completed
            ),
            "median_total_observed_action_count": median(
                lane.get("tool_usage", {}).get("total_observed_action_count")
                for lane in completed
            ),
            "median_shell_command_count": median(
                lane.get("tool_usage", {}).get("shell_command_count")
                for lane in completed
            ),
            "median_file_read_count": median(
                lane.get("tool_usage", {}).get("file_read_count")
                for lane in completed
            ),
            "median_test_or_probe_count": median(
                lane.get("tool_usage", {}).get("test_or_probe_count")
                for lane in completed
            ),
            "median_runtime_probe_count": median(
                lane.get("tool_usage", {}).get("runtime_probe_count")
                for lane in completed
            ),
            "median_m1nd_call_count": median(
                lane.get("tool_usage", {}).get("m1nd_call_count")
                for lane in completed
            ),
            "median_mission_control_call_count": median(
                lane.get("tool_usage", {}).get("mission_control_call_count")
                for lane in completed
            ),
            "median_source_backed_finding_count": median(
                lane.get("source_backed_finding_count") for lane in completed
            ),
            "total_findings": sum(lane["findings_count"] for lane in completed),
            "extra_unadjudicated_findings_total": sum(
                lane["extra_unadjudicated_findings_count"] for lane in completed
            ),
            "mission_control_loop_complete_lanes": sum(
                1
                for lane in mission_completed
                if lane.get("mission_control", {}).get("loop_complete")
            ),
            "mission_control_unavailable_lanes": sum(
                1
                for lane in mission_completed
                if lane.get("mission_control", {}).get("unavailable")
            ),
            "mission_control_evidence_packet_complete_lanes": sum(
                1
                for lane in mission_completed
                if lane.get("mission_control", {}).get("evidence_packet_complete")
            ),
            "median_mission_event_count": median(
                lane.get("mission_control", {}).get("mission_event_count")
                for lane in mission_completed
            ),
            "median_mission_next_count": median(
                lane.get("mission_control", {}).get("mission_next_count")
                for lane in mission_completed
            ),
            "median_mission_verify_count": median(
                lane.get("mission_control", {}).get("mission_verify_count")
                for lane in mission_completed
            ),
            "median_rejected_claim_signal_count": median(
                lane.get("mission_control", {}).get("rejected_claim_signal_count")
                for lane in mission_completed
            ),
            "median_direct_proof_switch_count": median(
                lane.get("mission_control", {}).get("direct_proof_switch_count")
                for lane in mission_completed
            ),
            "median_coverage_sweep_count": median(
                lane.get("mission_control", {}).get("coverage_sweep_count")
                for lane in mission_completed
            ),
            "median_mission_control_adherence_rate": median(
                lane.get("mission_control", {}).get("adherence_rate")
                for lane in mission_completed
            ),
            "lanes": [lane["lane_id"] for lane in arm_lanes],
        }
    return arms


def mission_control_validity(lanes):
    mission_lanes = [
        lane for lane in lanes if lane.get("instruction_mode") == "m1nd-mission-control"
    ]
    completed = [lane for lane in mission_lanes if lane.get("completed")]
    evaluable = [
        lane
        for lane in completed
        if lane.get("mission_control", {}).get("loop_complete")
        and not lane.get("mission_control", {}).get("unavailable")
    ]
    partial_or_unavailable = [
        lane
        for lane in completed
        if lane.get("mission_control", {}).get("unavailable")
        or not lane.get("mission_control", {}).get("loop_complete")
    ]
    missing = [lane for lane in mission_lanes if not lane.get("completed")]
    all_completed_lanes_evaluable = None
    if mission_lanes:
        all_completed_lanes_evaluable = (
            len(completed) == len(mission_lanes)
            and len(evaluable) == len(completed)
        )
    return {
        "present": bool(mission_lanes),
        "lane_count": len(mission_lanes),
        "completed_lane_count": len(completed),
        "evaluable_lane_count": len(evaluable),
        "partial_or_unavailable_lane_count": len(partial_or_unavailable),
        "missing_result_lane_count": len(missing),
        "all_completed_lanes_evaluable": all_completed_lanes_evaluable,
        "evaluable_lane_ids": [lane["lane_id"] for lane in evaluable],
        "partial_or_unavailable_lane_ids": [lane["lane_id"] for lane in partial_or_unavailable],
        "missing_result_lane_ids": [lane["lane_id"] for lane in missing],
        "non_claim": "Mission Control recall should be interpreted only for lanes that completed the mission loop.",
    }


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
    mission_validity = mission_control_validity(lanes)
    public_claim_blockers = [
        "single internal round",
        "one fixture repo",
        "seeded recall only; extra findings are not independently judged",
    ]
    if len(set(arm_lane_counts.values())) != 1:
        public_claim_blockers.append(
            "instruction-mode lane counts are not balanced; compare rates rather than raw totals"
        )
    if (
        mission_validity["present"]
        and not mission_validity["all_completed_lanes_evaluable"]
    ):
        public_claim_blockers.append(
            "Mission Control arm is not fully evaluable; at least one lane is missing, unavailable, or did not complete start/event/next/verify/handoff/close"
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
            "mission_control_validity": mission_validity,
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
                "median_first_good_finding_seconds": payload[
                    "median_first_good_finding_seconds"
                ],
                "median_total_observed_action_count": payload[
                    "median_total_observed_action_count"
                ],
                "median_source_backed_finding_count": payload[
                    "median_source_backed_finding_count"
                ],
            }
            for arm, payload in arms.items()
        },
        "invalidated_attempts": round_payload.get("invalidated_attempts", []),
        "public_claim_worthy": False,
        "public_claim_blockers": public_claim_blockers,
        "non_claims": sorted(set(NON_CLAIMS + answer_key.get("non_claims", []))),
    }


def next_product_actions(report):
    mission_validity = report["comparability"]["mission_control_validity"]
    actions = []
    if mission_validity["present"]:
        if mission_validity["all_completed_lanes_evaluable"]:
            mission_control_arm = report.get("arms", {}).get("m1nd-mission-control", {})
            if (
                mission_control_arm.get("seeded_recall_rate") == 1.0
                and (mission_control_arm.get("median_coverage_sweep_count") or 0) > 0
            ):
                actions.append(
                    "Repeat the Mission Control direct-sweep calibration on another fixture before treating it as a generalized improvement."
                )
            else:
                actions.append(
                    "Analyze conservative Mission Control misses and tune mission_next/mission_verify stopping rules."
                )
        else:
            actions.append(
                "Fix worker-host Mission Control exposure before rerunning Mission Control comparisons."
            )
    actions.extend(
        [
            "Keep improving the compact trained-agent loop as a default universal agent pack behavior.",
            "Add cleaner state placement so m1nd benchmark/probe flows do not write sidecar metadata into target repos.",
            "Use first-good-finding time, observed action counts, and source-backed finding counts as standard internal benchmark dimensions.",
            "Add a judge pass for extra findings so future reports can separate true extras from noise.",
        ]
    )
    return actions


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
        lines.append(
            f"  Timing/actions: median first-good finding `{payload['median_first_good_finding_seconds']}`s, "
            f"median observed actions `{payload['median_total_observed_action_count']}`, "
            f"median shell commands `{payload['median_shell_command_count']}`, "
            f"median file reads `{payload['median_file_read_count']}`, "
            f"median tests/probes `{payload['median_test_or_probe_count']}`."
        )
        if arm == "m1nd-mission-control" or payload["mission_control_loop_complete_lanes"]:
            lines.append(
                f"  Mission Control: loop-complete lanes `{payload['mission_control_loop_complete_lanes']}/{payload['completed_lane_count']}`, "
                f"unavailable lanes `{payload['mission_control_unavailable_lanes']}`, "
                f"median `mission_next` count `{payload['median_mission_next_count']}`, "
                f"median direct-proof switches `{payload['median_direct_proof_switch_count']}`, "
                f"median coverage sweeps `{payload['median_coverage_sweep_count']}`, "
                f"median adherence `{payload['median_mission_control_adherence_rate']}`."
            )

    mission_validity = report["comparability"]["mission_control_validity"]
    if mission_validity["present"]:
        lines.extend(
            [
                "",
                "## Mission Control Validity",
                "",
                f"- Evaluable lanes: `{mission_validity['evaluable_lane_count']}/{mission_validity['lane_count']}`.",
                f"- Partial or unavailable lanes: `{mission_validity['partial_or_unavailable_lane_ids']}`.",
                f"- Missing result lanes: `{mission_validity['missing_result_lane_ids']}`.",
            ]
        )
        if not mission_validity["all_completed_lanes_evaluable"]:
            lines.append(
                "- Mission Control recall is fallback evidence only until completed Mission Control lanes are evaluable."
            )

    next_actions = next_product_actions(report)
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
            *[f"- {action}" for action in next_actions],
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
    init_parser.add_argument(
        "--mission-control-binary",
        help="Optional m1nd-mcp binary path to embed in Mission Control lane prompts.",
    )
    init_parser.add_argument(
        "--no-materialize-workspaces",
        action="store_true",
        help="Create prompts/templates only; do not copy --seeded-repo into per-lane workspaces.",
    )
    init_parser.add_argument("--json", action="store_true")

    score_parser = subparsers.add_parser("score")
    score_parser.add_argument("--round-file", required=True, type=Path)
    score_parser.add_argument("--answer-key", required=True, type=Path)
    score_parser.add_argument("--lane-results-dir", required=True, type=Path)
    score_parser.add_argument("--output", required=True, type=Path)
    score_parser.add_argument("--notes", type=Path)
    score_parser.add_argument("--json", action="store_true")

    preflight_parser = subparsers.add_parser("preflight")
    preflight_parser.add_argument("--round-file", required=True, type=Path)
    preflight_parser.add_argument(
        "--require-mode",
        action="append",
        choices=VALID_INSTRUCTION_MODES,
        default=[],
    )
    preflight_parser.add_argument(
        "--require-live-mission-control",
        action="store_true",
        help="Probe the selected m1nd-mcp binary and require mission_start/event/next/verify/handoff/close.",
    )
    preflight_parser.add_argument(
        "--m1nd-binary",
        help="m1nd-mcp binary to probe when --require-live-mission-control is set.",
    )
    preflight_parser.add_argument("--json", action="store_true")

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
    elif args.command == "preflight":
        preflight = preflight_round(
            args.round_file,
            required_modes=args.require_mode,
            require_live_mission_control=args.require_live_mission_control,
            m1nd_binary=args.m1nd_binary,
        )
        if args.json:
            print(json.dumps(preflight, indent=2))
        if not preflight["ok"]:
            raise SystemExit(1)


if __name__ == "__main__":
    main()
