#!/usr/bin/env python3
import argparse
import json
from collections import Counter, defaultdict
from pathlib import Path


def load_run(path: Path):
    with path.open("r", encoding="utf-8") as f:
        return json.load(f)


def dump_json(path: Path, payload):
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("w", encoding="utf-8") as f:
        json.dump(payload, f, indent=2, ensure_ascii=False)
        f.write("\n")


def safe_rate(numerator, denominator):
    if denominator:
        return round(numerator / denominator, 4)
    return None


def counter_to_sorted_dict(values):
    return dict(sorted(Counter(values).items()))


def summarize_runs(runs, input_price_per_1m=None, time_value_per_hour_usd=None):
    by_scenario = defaultdict(dict)
    for run in runs:
        if "scenario_id" not in run or "mode" not in run:
            continue
        by_scenario[run["scenario_id"]][run["mode"]] = run

    scenarios = []
    aggregate_manual = 0
    aggregate_warm = 0
    aggregate_manual_time = 0.0
    aggregate_warm_time = 0.0
    aggregate_manual_search_iterations = 0
    aggregate_warm_search_iterations = 0
    aggregate_manual_repeat_reads = 0
    aggregate_warm_repeat_reads = 0
    aggregate_manual_false_starts = 0
    aggregate_warm_false_starts = 0
    aggregate_manual_guidance_events = 0
    aggregate_warm_guidance_events = 0
    aggregate_manual_guidance_followed = 0
    aggregate_warm_guidance_followed = 0
    aggregate_manual_progress_events = 0
    aggregate_warm_progress_events = 0
    aggregate_manual_progress_event_types = set()
    aggregate_warm_progress_event_types = set()
    aggregate_manual_progress_delivery_modes = set()
    aggregate_warm_progress_delivery_modes = set()
    aggregate_manual_live_progress_events = 0
    aggregate_warm_live_progress_events = 0
    aggregate_manual_replay_progress_events = 0
    aggregate_warm_replay_progress_events = 0
    aggregate_manual_snapshot_progress_events = 0
    aggregate_warm_snapshot_progress_events = 0
    aggregate_manual_progress_guidance_events = 0
    aggregate_warm_progress_guidance_events = 0
    aggregate_manual_progress_guidance_followed = 0
    aggregate_warm_progress_guidance_followed = 0
    aggregate_manual_recovery_events = 0
    aggregate_warm_recovery_events = 0
    aggregate_manual_recovery_followed = 0
    aggregate_warm_recovery_followed = 0
    aggregate_manual_missing_signals = 0
    aggregate_warm_missing_signals = 0
    aggregate_manual_missing_resolved = 0
    aggregate_warm_missing_resolved = 0
    aggregate_manual_proof_states = Counter()
    aggregate_warm_proof_states = Counter()
    compared = 0

    for scenario_id, modes in sorted(by_scenario.items()):
        manual = modes.get("manual")
        warm = modes.get("m1nd_warm")
        entry = {
            "scenario_id": scenario_id,
            "scenario_name": (manual or warm or next(iter(modes.values())))["scenario_name"],
            "modes_present": sorted(modes.keys()),
        }
        if manual:
            manual_proof_state_counts = counter_to_sorted_dict(manual.get("proof_states", []))
            entry["manual"] = {
                "token_proxy": manual["token_proxy"],
                "execution_origin": manual.get("execution_origin"),
                "time_to_first_good_answer_ms": manual["time_to_first_good_answer_ms"],
                "time_to_full_proof_ms": manual["time_to_full_proof_ms"],
                "files_opened": manual["files_opened"],
                "repeat_reads": manual["repeat_reads"],
                "search_iterations": manual["search_iterations"],
                "false_start_count": manual.get("false_start_count", 0),
                "guidance_events": manual.get("guidance_events", 0),
                "guidance_followed": manual.get("guidance_followed", 0),
                "guidance_followthrough_rate": safe_rate(
                    manual.get("guidance_followed", 0), manual.get("guidance_events", 0)
                ),
                "final_proof_state": manual.get("final_proof_state"),
                "proof_state_counts": manual_proof_state_counts,
                "progress_events": manual.get("progress_events", 0),
                "max_progress_pct": manual.get("max_progress_pct", 0.0),
                "progress_event_types": manual.get("progress_event_types", []),
                "progress_delivery_modes": manual.get("progress_delivery_modes", []),
                "live_progress_events": manual.get("live_progress_events", 0),
                "replay_progress_events": manual.get("replay_progress_events", 0),
                "snapshot_progress_events": manual.get("snapshot_progress_events", 0),
                "progress_guidance_events": manual.get("progress_guidance_events", 0),
                "progress_guidance_followed": manual.get(
                    "progress_guidance_followed", 0
                ),
                "progress_guidance_followthrough_rate": safe_rate(
                    manual.get("progress_guidance_followed", 0),
                    manual.get("progress_guidance_events", 0),
                ),
                "recovery_events": manual.get("recovery_events", 0),
                "recovery_followed": manual.get("recovery_followed", 0),
                "recovery_followthrough_rate": safe_rate(
                    manual.get("recovery_followed", 0), manual.get("recovery_events", 0)
                ),
                "missing_signals": manual.get("missing_signals", 0),
                "missing_resolved": manual.get("missing_resolved", 0),
                "missing_resolution_rate": safe_rate(
                    manual.get("missing_resolved", 0), manual.get("missing_signals", 0)
                ),
            }
        if warm:
            warm_proof_state_counts = counter_to_sorted_dict(warm.get("proof_states", []))
            entry["m1nd_warm"] = {
                "token_proxy": warm["token_proxy"],
                "execution_origin": warm.get("execution_origin"),
                "time_to_first_good_answer_ms": warm["time_to_first_good_answer_ms"],
                "time_to_full_proof_ms": warm["time_to_full_proof_ms"],
                "files_opened": warm["files_opened"],
                "repeat_reads": warm["repeat_reads"],
                "search_iterations": warm["search_iterations"],
                "false_start_count": warm.get("false_start_count", 0),
                "guidance_events": warm.get("guidance_events", 0),
                "guidance_followed": warm.get("guidance_followed", 0),
                "guidance_followthrough_rate": safe_rate(
                    warm.get("guidance_followed", 0), warm.get("guidance_events", 0)
                ),
                "final_proof_state": warm.get("final_proof_state"),
                "proof_state_counts": warm_proof_state_counts,
                "progress_events": warm.get("progress_events", 0),
                "max_progress_pct": warm.get("max_progress_pct", 0.0),
                "progress_event_types": warm.get("progress_event_types", []),
                "progress_delivery_modes": warm.get("progress_delivery_modes", []),
                "live_progress_events": warm.get("live_progress_events", 0),
                "replay_progress_events": warm.get("replay_progress_events", 0),
                "snapshot_progress_events": warm.get("snapshot_progress_events", 0),
                "progress_guidance_events": warm.get("progress_guidance_events", 0),
                "progress_guidance_followed": warm.get(
                    "progress_guidance_followed", 0
                ),
                "progress_guidance_followthrough_rate": safe_rate(
                    warm.get("progress_guidance_followed", 0),
                    warm.get("progress_guidance_events", 0),
                ),
                "recovery_events": warm.get("recovery_events", 0),
                "recovery_followed": warm.get("recovery_followed", 0),
                "recovery_followthrough_rate": safe_rate(
                    warm.get("recovery_followed", 0), warm.get("recovery_events", 0)
                ),
                "missing_signals": warm.get("missing_signals", 0),
                "missing_resolved": warm.get("missing_resolved", 0),
                "missing_resolution_rate": safe_rate(
                    warm.get("missing_resolved", 0), warm.get("missing_signals", 0)
                ),
            }
        if manual and warm:
            token_delta = manual["token_proxy"] - warm["token_proxy"]
            time_delta = manual["time_to_first_good_answer_ms"] - warm["time_to_first_good_answer_ms"]
            entry["comparison"] = {
                "token_delta": token_delta,
                "token_savings_pct": round((token_delta / manual["token_proxy"]) * 100, 2)
                if manual["token_proxy"]
                else None,
                "first_good_answer_delta_ms": round(time_delta, 3),
                "search_iteration_delta": manual["search_iterations"] - warm["search_iterations"],
                "repeat_read_delta": manual["repeat_reads"] - warm["repeat_reads"],
                "false_start_delta": manual.get("false_start_count", 0)
                - warm.get("false_start_count", 0),
                "guidance_event_delta": manual.get("guidance_events", 0)
                - warm.get("guidance_events", 0),
                "guidance_followed_delta": manual.get("guidance_followed", 0)
                - warm.get("guidance_followed", 0),
                "progress_event_delta": manual.get("progress_events", 0)
                - warm.get("progress_events", 0),
                "live_progress_event_delta": manual.get("live_progress_events", 0)
                - warm.get("live_progress_events", 0),
                "replay_progress_event_delta": manual.get("replay_progress_events", 0)
                - warm.get("replay_progress_events", 0),
                "snapshot_progress_event_delta": manual.get("snapshot_progress_events", 0)
                - warm.get("snapshot_progress_events", 0),
                "progress_guidance_event_delta": manual.get(
                    "progress_guidance_events", 0
                )
                - warm.get("progress_guidance_events", 0),
                "progress_guidance_followed_delta": manual.get(
                    "progress_guidance_followed", 0
                )
                - warm.get("progress_guidance_followed", 0),
                "recovery_event_delta": manual.get("recovery_events", 0)
                - warm.get("recovery_events", 0),
                "recovery_followed_delta": manual.get("recovery_followed", 0)
                - warm.get("recovery_followed", 0),
                "missing_signal_delta": manual.get("missing_signals", 0)
                - warm.get("missing_signals", 0),
                "missing_resolved_delta": manual.get("missing_resolved", 0)
                - warm.get("missing_resolved", 0),
            }
            aggregate_manual += manual["token_proxy"]
            aggregate_warm += warm["token_proxy"]
            aggregate_manual_time += manual["time_to_first_good_answer_ms"]
            aggregate_warm_time += warm["time_to_first_good_answer_ms"]
            aggregate_manual_search_iterations += manual["search_iterations"]
            aggregate_warm_search_iterations += warm["search_iterations"]
            aggregate_manual_repeat_reads += manual["repeat_reads"]
            aggregate_warm_repeat_reads += warm["repeat_reads"]
            aggregate_manual_false_starts += manual.get("false_start_count", 0)
            aggregate_warm_false_starts += warm.get("false_start_count", 0)
            aggregate_manual_guidance_events += manual.get("guidance_events", 0)
            aggregate_warm_guidance_events += warm.get("guidance_events", 0)
            aggregate_manual_guidance_followed += manual.get("guidance_followed", 0)
            aggregate_warm_guidance_followed += warm.get("guidance_followed", 0)
            aggregate_manual_progress_events += manual.get("progress_events", 0)
            aggregate_warm_progress_events += warm.get("progress_events", 0)
            aggregate_manual_progress_event_types.update(manual.get("progress_event_types", []))
            aggregate_warm_progress_event_types.update(warm.get("progress_event_types", []))
            aggregate_manual_progress_delivery_modes.update(
                manual.get("progress_delivery_modes", [])
            )
            aggregate_warm_progress_delivery_modes.update(
                warm.get("progress_delivery_modes", [])
            )
            aggregate_manual_live_progress_events += manual.get("live_progress_events", 0)
            aggregate_warm_live_progress_events += warm.get("live_progress_events", 0)
            aggregate_manual_replay_progress_events += manual.get(
                "replay_progress_events", 0
            )
            aggregate_warm_replay_progress_events += warm.get(
                "replay_progress_events", 0
            )
            aggregate_manual_snapshot_progress_events += manual.get(
                "snapshot_progress_events", 0
            )
            aggregate_warm_snapshot_progress_events += warm.get(
                "snapshot_progress_events", 0
            )
            aggregate_manual_progress_guidance_events += manual.get(
                "progress_guidance_events", 0
            )
            aggregate_warm_progress_guidance_events += warm.get(
                "progress_guidance_events", 0
            )
            aggregate_manual_progress_guidance_followed += manual.get(
                "progress_guidance_followed", 0
            )
            aggregate_warm_progress_guidance_followed += warm.get(
                "progress_guidance_followed", 0
            )
            aggregate_manual_recovery_events += manual.get("recovery_events", 0)
            aggregate_warm_recovery_events += warm.get("recovery_events", 0)
            aggregate_manual_recovery_followed += manual.get("recovery_followed", 0)
            aggregate_warm_recovery_followed += warm.get("recovery_followed", 0)
            aggregate_manual_missing_signals += manual.get("missing_signals", 0)
            aggregate_warm_missing_signals += warm.get("missing_signals", 0)
            aggregate_manual_missing_resolved += manual.get("missing_resolved", 0)
            aggregate_warm_missing_resolved += warm.get("missing_resolved", 0)
            aggregate_manual_proof_states.update(manual.get("proof_states", []))
            aggregate_warm_proof_states.update(warm.get("proof_states", []))
            compared += 1
        scenarios.append(entry)

    summary = {
        "run_count": len(runs),
        "compared_scenarios": compared,
        "scenarios": scenarios,
    }

    if compared:
        token_delta = aggregate_manual - aggregate_warm
        summary["aggregate"] = {
            "manual_token_proxy": aggregate_manual,
            "m1nd_warm_token_proxy": aggregate_warm,
            "token_delta": token_delta,
            "token_savings_pct": round((token_delta / aggregate_manual) * 100, 2)
            if aggregate_manual
            else None,
            "manual_first_good_answer_ms": round(aggregate_manual_time, 3),
            "m1nd_warm_first_good_answer_ms": round(aggregate_warm_time, 3),
            "first_good_answer_delta_ms": round(
                aggregate_manual_time - aggregate_warm_time, 3
            ),
            "manual_search_iterations": aggregate_manual_search_iterations,
            "m1nd_warm_search_iterations": aggregate_warm_search_iterations,
            "search_iteration_delta": aggregate_manual_search_iterations
            - aggregate_warm_search_iterations,
            "manual_repeat_reads": aggregate_manual_repeat_reads,
            "m1nd_warm_repeat_reads": aggregate_warm_repeat_reads,
            "repeat_read_delta": aggregate_manual_repeat_reads
            - aggregate_warm_repeat_reads,
            "manual_false_starts": aggregate_manual_false_starts,
            "m1nd_warm_false_starts": aggregate_warm_false_starts,
            "false_start_delta": aggregate_manual_false_starts
            - aggregate_warm_false_starts,
            "manual_guidance_events": aggregate_manual_guidance_events,
            "m1nd_warm_guidance_events": aggregate_warm_guidance_events,
            "guidance_event_delta": aggregate_manual_guidance_events
            - aggregate_warm_guidance_events,
            "manual_guidance_followed": aggregate_manual_guidance_followed,
            "m1nd_warm_guidance_followed": aggregate_warm_guidance_followed,
            "manual_guidance_followthrough_rate": safe_rate(
                aggregate_manual_guidance_followed, aggregate_manual_guidance_events
            ),
            "m1nd_warm_guidance_followthrough_rate": safe_rate(
                aggregate_warm_guidance_followed, aggregate_warm_guidance_events
            ),
            "guidance_followed_delta": aggregate_manual_guidance_followed
            - aggregate_warm_guidance_followed,
            "manual_progress_events": aggregate_manual_progress_events,
            "m1nd_warm_progress_events": aggregate_warm_progress_events,
            "progress_event_delta": aggregate_manual_progress_events
            - aggregate_warm_progress_events,
            "manual_progress_event_types": sorted(aggregate_manual_progress_event_types),
            "m1nd_warm_progress_event_types": sorted(aggregate_warm_progress_event_types),
            "manual_progress_delivery_modes": sorted(
                aggregate_manual_progress_delivery_modes
            ),
            "m1nd_warm_progress_delivery_modes": sorted(
                aggregate_warm_progress_delivery_modes
            ),
            "manual_live_progress_events": aggregate_manual_live_progress_events,
            "m1nd_warm_live_progress_events": aggregate_warm_live_progress_events,
            "live_progress_event_delta": aggregate_manual_live_progress_events
            - aggregate_warm_live_progress_events,
            "manual_replay_progress_events": aggregate_manual_replay_progress_events,
            "m1nd_warm_replay_progress_events": aggregate_warm_replay_progress_events,
            "replay_progress_event_delta": aggregate_manual_replay_progress_events
            - aggregate_warm_replay_progress_events,
            "manual_snapshot_progress_events": aggregate_manual_snapshot_progress_events,
            "m1nd_warm_snapshot_progress_events": aggregate_warm_snapshot_progress_events,
            "snapshot_progress_event_delta": aggregate_manual_snapshot_progress_events
            - aggregate_warm_snapshot_progress_events,
            "manual_progress_guidance_events": aggregate_manual_progress_guidance_events,
            "m1nd_warm_progress_guidance_events": aggregate_warm_progress_guidance_events,
            "progress_guidance_event_delta": aggregate_manual_progress_guidance_events
            - aggregate_warm_progress_guidance_events,
            "manual_progress_guidance_followed": aggregate_manual_progress_guidance_followed,
            "m1nd_warm_progress_guidance_followed": aggregate_warm_progress_guidance_followed,
            "manual_progress_guidance_followthrough_rate": safe_rate(
                aggregate_manual_progress_guidance_followed,
                aggregate_manual_progress_guidance_events,
            ),
            "m1nd_warm_progress_guidance_followthrough_rate": safe_rate(
                aggregate_warm_progress_guidance_followed,
                aggregate_warm_progress_guidance_events,
            ),
            "progress_guidance_followed_delta": aggregate_manual_progress_guidance_followed
            - aggregate_warm_progress_guidance_followed,
            "manual_recovery_events": aggregate_manual_recovery_events,
            "m1nd_warm_recovery_events": aggregate_warm_recovery_events,
            "recovery_event_delta": aggregate_manual_recovery_events
            - aggregate_warm_recovery_events,
            "manual_recovery_followed": aggregate_manual_recovery_followed,
            "m1nd_warm_recovery_followed": aggregate_warm_recovery_followed,
            "manual_recovery_followthrough_rate": safe_rate(
                aggregate_manual_recovery_followed, aggregate_manual_recovery_events
            ),
            "m1nd_warm_recovery_followthrough_rate": safe_rate(
                aggregate_warm_recovery_followed, aggregate_warm_recovery_events
            ),
            "recovery_followed_delta": aggregate_manual_recovery_followed
            - aggregate_warm_recovery_followed,
            "manual_missing_signals": aggregate_manual_missing_signals,
            "m1nd_warm_missing_signals": aggregate_warm_missing_signals,
            "missing_signal_delta": aggregate_manual_missing_signals
            - aggregate_warm_missing_signals,
            "manual_missing_resolved": aggregate_manual_missing_resolved,
            "m1nd_warm_missing_resolved": aggregate_warm_missing_resolved,
            "manual_missing_resolution_rate": safe_rate(
                aggregate_manual_missing_resolved, aggregate_manual_missing_signals
            ),
            "m1nd_warm_missing_resolution_rate": safe_rate(
                aggregate_warm_missing_resolved, aggregate_warm_missing_signals
            ),
            "missing_resolved_delta": aggregate_manual_missing_resolved
            - aggregate_warm_missing_resolved,
            "manual_proof_state_counts": dict(sorted(aggregate_manual_proof_states.items())),
            "m1nd_warm_proof_state_counts": dict(sorted(aggregate_warm_proof_states.items())),
        }
        if input_price_per_1m is not None:
            summary["aggregate"]["input_price_per_1m"] = input_price_per_1m
            summary["aggregate"]["estimated_input_cost_saved_usd"] = round(
                (token_delta / 1_000_000.0) * input_price_per_1m,
                6,
            )
        if time_value_per_hour_usd is not None:
            delta_hours = (aggregate_manual_time - aggregate_warm_time) / 1000.0 / 3600.0
            summary["aggregate"]["time_value_per_hour_usd"] = time_value_per_hour_usd
            summary["aggregate"]["estimated_time_value_saved_usd"] = round(
                delta_hours * time_value_per_hour_usd,
                6,
            )

    return summary


def main():
    parser = argparse.ArgumentParser(description="Summarize benchmark run JSON files.")
    parser.add_argument("--runs-dir", required=True, help="Directory with benchmark run JSON files")
    parser.add_argument("--output", required=True, help="Where to write the summary JSON")
    parser.add_argument("--input-price-per-1m", type=float)
    parser.add_argument("--time-value-per-hour-usd", type=float)
    args = parser.parse_args()

    runs_dir = Path(args.runs_dir)
    run_files = sorted(
        path for path in runs_dir.glob("*.json") if path.name != "summary.json"
    )
    runs = [load_run(path) for path in run_files]
    summary = summarize_runs(
        runs,
        input_price_per_1m=args.input_price_per_1m,
        time_value_per_hour_usd=args.time_value_per_hour_usd,
    )
    dump_json(Path(args.output), summary)
    print(json.dumps(summary.get("aggregate", {"run_count": len(runs)}), indent=2))


if __name__ == "__main__":
    main()
