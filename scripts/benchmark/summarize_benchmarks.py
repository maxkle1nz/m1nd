#!/usr/bin/env python3
import argparse
import json
from collections import defaultdict
from pathlib import Path


def load_run(path: Path):
    with path.open("r", encoding="utf-8") as f:
        return json.load(f)


def dump_json(path: Path, payload):
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("w", encoding="utf-8") as f:
        json.dump(payload, f, indent=2, ensure_ascii=False)
        f.write("\n")


def summarize_runs(runs):
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
            entry["manual"] = {
                "token_proxy": manual["token_proxy"],
                "time_to_first_good_answer_ms": manual["time_to_first_good_answer_ms"],
                "time_to_full_proof_ms": manual["time_to_full_proof_ms"],
                "files_opened": manual["files_opened"],
                "search_iterations": manual["search_iterations"],
            }
        if warm:
            entry["m1nd_warm"] = {
                "token_proxy": warm["token_proxy"],
                "time_to_first_good_answer_ms": warm["time_to_first_good_answer_ms"],
                "time_to_full_proof_ms": warm["time_to_full_proof_ms"],
                "files_opened": warm["files_opened"],
                "search_iterations": warm["search_iterations"],
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
            }
            aggregate_manual += manual["token_proxy"]
            aggregate_warm += warm["token_proxy"]
            aggregate_manual_time += manual["time_to_first_good_answer_ms"]
            aggregate_warm_time += warm["time_to_first_good_answer_ms"]
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
        }

    return summary


def main():
    parser = argparse.ArgumentParser(description="Summarize benchmark run JSON files.")
    parser.add_argument("--runs-dir", required=True, help="Directory with benchmark run JSON files")
    parser.add_argument("--output", required=True, help="Where to write the summary JSON")
    args = parser.parse_args()

    runs_dir = Path(args.runs_dir)
    run_files = sorted(
        path for path in runs_dir.glob("*.json") if path.name != "summary.json"
    )
    runs = [load_run(path) for path in run_files]
    summary = summarize_runs(runs)
    dump_json(Path(args.output), summary)
    print(json.dumps(summary.get("aggregate", {"run_count": len(runs)}), indent=2))


if __name__ == "__main__":
    main()
