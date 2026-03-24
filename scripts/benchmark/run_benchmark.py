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


def summarize_events(events):
    files_open_sequence = []
    search_iterations = 0
    chars_surfaced = 0
    guidance_events = 0
    guidance_followed = 0
    reactivated_nodes = 0
    resume_hints = 0
    proof_states = []

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
            "token_proxy": run["token_proxy"],
            "files_opened": run["files_opened"],
            "repeat_reads": run["repeat_reads"],
            "search_iterations": run["search_iterations"],
            "guidance_events": run["guidance_events"],
            "guidance_followed": run["guidance_followed"],
            "output": str(Path(args.output)),
        },
        indent=2,
    ))


if __name__ == "__main__":
    main()
