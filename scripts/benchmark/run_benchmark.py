#!/usr/bin/env python3
import argparse
import json
import math
from datetime import datetime, timezone
from pathlib import Path


def load_json(path: Path):
    with path.open("r", encoding="utf-8") as f:
        return json.load(f)


def dump_json(path: Path, payload):
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("w", encoding="utf-8") as f:
        json.dump(payload, f, indent=2, ensure_ascii=False)
        f.write("\n")


def chars_from_event(event):
    if "payload_chars" in event and isinstance(event["payload_chars"], int):
        return max(event["payload_chars"], 0)

    chars = 0
    for key in ("query", "target", "notes", "surfaced_text"):
        value = event.get(key)
        if isinstance(value, str):
            chars += len(value)
    for key in ("surfaced_files", "opened_files"):
        value = event.get(key)
        if isinstance(value, list):
            chars += sum(len(str(item)) for item in value)
    return chars


def normalize_event(index, event):
    normalized = dict(event)
    normalized.setdefault("event_index", index)
    normalized.setdefault("tool_name", "unknown")
    normalized.setdefault("elapsed_ms", 0)
    normalized.setdefault("opened_files", [])
    normalized.setdefault("surfaced_files", [])
    normalized["payload_chars"] = chars_from_event(normalized)
    return normalized


def iter_progress_entries(event):
    entries = event.get("progress_events")
    if isinstance(entries, list) and entries:
        normalized = []
        for item in entries:
            if not isinstance(item, dict):
                continue
            normalized.append(
                {
                    "event_type": item.get("event_type"),
                    "progress_delivery": item.get("progress_delivery")
                    or event.get("progress_delivery"),
                    "batch_id": item.get("batch_id") or event.get("batch_id"),
                    "phase": item.get("phase") or event.get("active_phase"),
                    "phase_index": item.get("phase_index"),
                    "progress_pct": item.get("progress_pct"),
                    "current_file": item.get("current_file"),
                    "next_phase": item.get("next_phase"),
                    "proof_state": item.get("proof_state"),
                    "next_suggested_tool": item.get("next_suggested_tool"),
                    "next_suggested_target": item.get("next_suggested_target"),
                    "next_step_hint": item.get("next_step_hint"),
                    "elapsed_ms": item.get("elapsed_ms"),
                    "message": item.get("message"),
                }
            )
        if normalized:
            return normalized

    progress_pct = event.get("progress_pct")
    active_phase = event.get("active_phase")
    next_phase = event.get("next_phase")
    if isinstance(progress_pct, (int, float)) or isinstance(active_phase, str):
        return [
            {
                "event_type": "snapshot",
                "progress_delivery": event.get("progress_delivery") or "snapshot",
                "batch_id": event.get("batch_id"),
                "phase": active_phase,
                "phase_index": event.get("completed_phase_count"),
                "progress_pct": progress_pct,
                "current_file": event.get("current_file"),
                "next_phase": next_phase,
                "proof_state": event.get("proof_state"),
                "next_suggested_tool": event.get("next_suggested_tool"),
                "next_suggested_target": event.get("next_suggested_target"),
                "next_step_hint": event.get("next_step_hint"),
                "elapsed_ms": event.get("elapsed_ms"),
                "message": event.get("status_message") or event.get("notes"),
            }
        ]

    return []


def summarize_events(events):
    files_open_sequence = []
    search_iterations = 0
    chars_surfaced = 0
    guidance_events = 0
    guidance_followed = 0
    reactivated_nodes = 0
    resume_hints = 0
    proof_states = []
    progress_events = 0
    max_progress_pct = 0.0
    active_phases = []
    next_phases = []
    progress_event_types = []
    progress_delivery_modes = []
    live_progress_events = 0
    replay_progress_events = 0
    snapshot_progress_events = 0
    progress_guidance_events = 0
    progress_guidance_followed = 0
    recovery_events = 0
    recovery_followed = 0

    for event in events:
        chars_surfaced += event["payload_chars"]
        tool_name = str(event.get("tool_name", "")).lower()
        if any(token in tool_name for token in ("search", "seek", "grep", "glob", "rg")):
            search_iterations += 1
        suggested_tool = event.get("next_suggested_tool")
        if isinstance(suggested_tool, str) and suggested_tool:
            guidance_events += 1
            next_tool_used = str(event.get("next_tool_used", "")).strip()
            if next_tool_used and next_tool_used == suggested_tool:
                guidance_followed += 1
        reactivated = event.get("reactivated_node_ids")
        if isinstance(reactivated, list):
            reactivated_nodes += len(reactivated)
        hints = event.get("resume_hints")
        if isinstance(hints, list):
            resume_hints += len(hints)
        proof_state = event.get("proof_state")
        if isinstance(proof_state, str) and proof_state:
            proof_states.append(proof_state)
        hint = event.get("hint")
        suggested_next_step = event.get("suggested_next_step")
        example = event.get("example")
        if (
            isinstance(hint, str)
            and hint.strip()
            or isinstance(suggested_next_step, str)
            and suggested_next_step.strip()
            or isinstance(example, (dict, list))
        ):
            recovery_events += 1
            next_tool_used = str(event.get("next_tool_used", "")).strip()
            recovery_followed_flag = event.get("recovery_followed")
            if isinstance(recovery_followed_flag, bool):
                if recovery_followed_flag:
                    recovery_followed += 1
            elif next_tool_used and isinstance(suggested_tool, str) and suggested_tool:
                if next_tool_used == suggested_tool:
                    recovery_followed += 1
        for progress_entry in iter_progress_entries(event):
            progress_events += 1
            progress_pct = progress_entry.get("progress_pct")
            if isinstance(progress_pct, (int, float)):
                max_progress_pct = max(max_progress_pct, float(progress_pct))
            active_phase = progress_entry.get("phase")
            if isinstance(active_phase, str) and active_phase:
                active_phases.append(active_phase)
            next_phase = progress_entry.get("next_phase")
            if isinstance(next_phase, str) and next_phase:
                next_phases.append(next_phase)
            event_type = progress_entry.get("event_type")
            if isinstance(event_type, str) and event_type:
                progress_event_types.append(event_type)
            progress_suggested_tool = progress_entry.get("next_suggested_tool")
            if isinstance(progress_suggested_tool, str) and progress_suggested_tool:
                progress_guidance_events += 1
                next_tool_used = str(event.get("next_tool_used", "")).strip()
                if next_tool_used and next_tool_used == progress_suggested_tool:
                    progress_guidance_followed += 1
            progress_proof_state = progress_entry.get("proof_state")
            if isinstance(progress_proof_state, str) and progress_proof_state:
                proof_states.append(progress_proof_state)
            progress_delivery = progress_entry.get("progress_delivery")
            if isinstance(progress_delivery, str) and progress_delivery:
                progress_delivery_modes.append(progress_delivery)
                if progress_delivery == "live":
                    live_progress_events += 1
                elif progress_delivery == "replay":
                    replay_progress_events += 1
                elif progress_delivery == "snapshot":
                    snapshot_progress_events += 1
        for key in ("opened_files", "surfaced_files"):
            for path in event.get(key, []):
                files_open_sequence.append(str(path))

    files_opened = len(set(files_open_sequence))
    repeat_reads = max(len(files_open_sequence) - files_opened, 0)

    return {
        "files_opened": files_opened,
        "repeat_reads": repeat_reads,
        "search_iterations": search_iterations,
        "chars_surfaced": chars_surfaced,
        "token_proxy": math.ceil(chars_surfaced / 4) if chars_surfaced else 0,
        "guidance_events": guidance_events,
        "guidance_followed": guidance_followed,
        "reactivated_nodes": reactivated_nodes,
        "resume_hints": resume_hints,
        "proof_states": proof_states,
        "progress_events": progress_events,
        "max_progress_pct": round(max_progress_pct, 2),
        "active_phases": active_phases,
        "next_phases": next_phases,
        "progress_event_types": progress_event_types,
        "progress_delivery_modes": progress_delivery_modes,
        "live_progress_events": live_progress_events,
        "replay_progress_events": replay_progress_events,
        "snapshot_progress_events": snapshot_progress_events,
        "progress_guidance_events": progress_guidance_events,
        "progress_guidance_followed": progress_guidance_followed,
        "recovery_events": recovery_events,
        "recovery_followed": recovery_followed,
    }


def build_run(args):
    scenario = load_json(Path(args.scenario))
    events = []
    if args.events:
        raw_events = load_json(Path(args.events))
        if not isinstance(raw_events, list):
            raise SystemExit("--events JSON must be a list")
        events = [normalize_event(i + 1, event) for i, event in enumerate(raw_events)]

    derived = summarize_events(events)

    run = {
        "recorded_at": datetime.now(timezone.utc).isoformat(),
        "scenario_id": scenario["scenario_id"],
        "scenario_name": scenario["scenario_name"],
        "scenario_tags": scenario.get("tags", []),
        "mode": args.mode,
        "execution_origin": args.execution_origin,
        "source_ref": args.source_ref,
        "cold_graph_time_ms": args.cold_graph_time_ms,
        "warm_graph_time_ms": args.warm_graph_time_ms,
        "time_to_first_good_answer_ms": args.time_to_first_good_answer_ms,
        "time_to_full_proof_ms": args.time_to_full_proof_ms,
        "answer_quality": args.answer_quality,
        "plan_changed": args.plan_changed,
        "false_start_count": args.false_start_count,
        "tests_identified_before_edit": args.tests_identified_before_edit,
        "public_claim_worthy": args.public_claim_worthy,
        "workflow_notes": args.workflow_notes,
        "notes": args.notes,
        "events": events,
        "files_opened": derived["files_opened"],
        "repeat_reads": derived["repeat_reads"],
        "search_iterations": derived["search_iterations"],
        "chars_surfaced": derived["chars_surfaced"],
        "token_proxy": derived["token_proxy"],
        "guidance_events": derived["guidance_events"],
        "guidance_followed": derived["guidance_followed"],
        "reactivated_nodes": derived["reactivated_nodes"],
        "resume_hints": derived["resume_hints"],
        "proof_states": derived["proof_states"],
        "final_proof_state": derived["proof_states"][-1] if derived["proof_states"] else None,
        "progress_events": derived["progress_events"],
        "max_progress_pct": derived["max_progress_pct"],
        "active_phases": derived["active_phases"],
        "next_phases": derived["next_phases"],
        "progress_event_types": derived["progress_event_types"],
        "progress_delivery_modes": derived["progress_delivery_modes"],
        "live_progress_events": derived["live_progress_events"],
        "replay_progress_events": derived["replay_progress_events"],
        "snapshot_progress_events": derived["snapshot_progress_events"],
        "progress_guidance_events": derived["progress_guidance_events"],
        "progress_guidance_followed": derived["progress_guidance_followed"],
        "recovery_events": derived["recovery_events"],
        "recovery_followed": derived["recovery_followed"],
        "repo_path": scenario.get("repo_path"),
        "question": scenario.get("question"),
        "expected_strength": scenario.get("expected_strength"),
    }
    return run


def main():
    parser = argparse.ArgumentParser(description="Record a benchmark run for m1nd scenarios.")
    parser.add_argument("--scenario", required=True, help="Path to scenario JSON")
    parser.add_argument("--mode", required=True, choices=["manual", "m1nd_cold", "m1nd_warm"])
    parser.add_argument("--output", required=True, help="Where to write the run JSON")
    parser.add_argument("--events", help="Optional path to tool-event JSON array")
    parser.add_argument(
        "--execution-origin",
        choices=["live", "replay", "snapshot"],
        default="snapshot",
        help="How the benchmark evidence was captured",
    )
    parser.add_argument(
        "--source-ref",
        default="",
        help="Optional path or identifier for the event/log source that produced the run",
    )
    parser.add_argument("--cold-graph-time-ms", type=float)
    parser.add_argument("--warm-graph-time-ms", type=float)
    parser.add_argument("--time-to-first-good-answer-ms", type=float, required=True)
    parser.add_argument("--time-to-full-proof-ms", type=float, required=True)
    parser.add_argument(
        "--answer-quality",
        default="medium",
        choices=["low", "medium", "high", "very_high"],
    )
    parser.add_argument("--plan-changed", action="store_true")
    parser.add_argument("--false-start-count", type=int, default=0)
    parser.add_argument("--tests-identified-before-edit", type=int, default=0)
    parser.add_argument("--public-claim-worthy", action="store_true")
    parser.add_argument("--workflow-notes", default="")
    parser.add_argument("--notes", default="")
    args = parser.parse_args()

    run = build_run(args)
    dump_json(Path(args.output), run)
    print(json.dumps(
        {
            "scenario_id": run["scenario_id"],
            "mode": run["mode"],
            "execution_origin": run["execution_origin"],
            "token_proxy": run["token_proxy"],
            "files_opened": run["files_opened"],
            "repeat_reads": run["repeat_reads"],
            "search_iterations": run["search_iterations"],
            "guidance_events": run["guidance_events"],
            "guidance_followed": run["guidance_followed"],
            "progress_events": run["progress_events"],
            "max_progress_pct": run["max_progress_pct"],
            "live_progress_events": run["live_progress_events"],
            "replay_progress_events": run["replay_progress_events"],
            "snapshot_progress_events": run["snapshot_progress_events"],
            "progress_guidance_events": run["progress_guidance_events"],
            "progress_guidance_followed": run["progress_guidance_followed"],
            "output": str(Path(args.output)),
        },
        indent=2,
    ))


if __name__ == "__main__":
    main()
