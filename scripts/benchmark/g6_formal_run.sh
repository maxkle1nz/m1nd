#!/usr/bin/env bash
# M1ND-10 G6 formal blind run — the owner ceremony, as one command.
#
#   scripts/benchmark/g6_formal_run.sh --dry-run
#       Exercise the whole pipeline that precedes measurement, against the public
#       corpus half only. Touches no labels, spawns no owner, claims nothing.
#
#   scripts/benchmark/g6_formal_run.sh --metric-spec ... --sealed-corpus ... ...
#       The formal blind run. Requires every owner-held input; refuses otherwise.
#
# This script orchestrates the existing runner and scorer. It does not
# reimplement any part of their law, and it never reads a label.
set -euo pipefail

usage() {
  cat <<'USAGE'
usage: g6_formal_run.sh [--dry-run] [options]

Mode:
  --dry-run                  exercise the public half; never score, never claim

Owner-held inputs (all required for a formal run):
  --metric-spec PATH         ratified metric spec v2
  --sealed-corpus PATH       sealed held-out corpus (hashed only, never parsed)
  --sealed-corpus-self-digest SHA256
                             the sealed corpus self_digest, supplied by the owner
  --authority-assembly PATH  pinned production authority assembly manifest
  --authority-assembly-digest SHA256
  --authority-provider PATH  authority provider executable
  --binary PATH              pinned candidate m1nd-mcp binary (current lane)
  --baseline-binary PATH     pinned baseline m1nd-mcp binary
  --baseline PATH            previously sealed baseline result artifact
  --baseline-receipt PATH    outcome-blind baseline-ratification receipt
  --run-ledger PATH          sealed-run ledger

Placement:
  --out DIR                  artifact directory (default: docs/benchmarks/<run-id>)
  --snapshot-root DIR        isolated source snapshot root
  --run-id ID                sealed run id (default: derived, unique per run)
  --keep-snapshot            do not delete the materialised snapshot afterwards
  -h, --help                 this text
USAGE
}

REPO_ROOT="$(git -C "$(dirname "${BASH_SOURCE[0]}")" rev-parse --show-toplevel)"
PREFLIGHT="$REPO_ROOT/scripts/benchmark/g6_formal_preflight.sh"
RUNNER="$REPO_ROOT/scripts/benchmark/m1nd10_g6_blind_runner.py"
SCORER="$REPO_ROOT/scripts/benchmark/m1nd10_g6_retrieval.py"
GENERALIZATION_SCORER="$REPO_ROOT/scripts/benchmark/m1nd10_g6_generalization_score.py"
HELD_OUT="$REPO_ROOT/docs/benchmarks/m1nd10-g6-held-out-v2"
GENERALIZATION="$REPO_ROOT/docs/benchmarks/m1nd10-g6-generalization-v2"

DRY_RUN=0
METRIC_SPEC=""
SEALED_CORPUS=""
SEALED_CORPUS_SELF_DIGEST=""
AUTHORITY_ASSEMBLY=""
AUTHORITY_ASSEMBLY_DIGEST=""
AUTHORITY_PROVIDER=""
CANDIDATE_BINARY=""
BASELINE_BINARY=""
BASELINE=""
BASELINE_RECEIPT=""
RUN_LEDGER=""
OUT_DIR=""
SNAPSHOT_ROOT=""
RUN_ID=""
KEEP_SNAPSHOT=0

while [ $# -gt 0 ]; do
  case "$1" in
    --dry-run) DRY_RUN=1; shift ;;
    --metric-spec) METRIC_SPEC="${2:?}"; shift 2 ;;
    --sealed-corpus) SEALED_CORPUS="${2:?}"; shift 2 ;;
    --sealed-corpus-self-digest) SEALED_CORPUS_SELF_DIGEST="${2:?}"; shift 2 ;;
    --authority-assembly) AUTHORITY_ASSEMBLY="${2:?}"; shift 2 ;;
    --authority-assembly-digest) AUTHORITY_ASSEMBLY_DIGEST="${2:?}"; shift 2 ;;
    --authority-provider) AUTHORITY_PROVIDER="${2:?}"; shift 2 ;;
    --binary) CANDIDATE_BINARY="${2:?}"; shift 2 ;;
    --baseline-binary) BASELINE_BINARY="${2:?}"; shift 2 ;;
    --baseline) BASELINE="${2:?}"; shift 2 ;;
    --baseline-receipt) BASELINE_RECEIPT="${2:?}"; shift 2 ;;
    --run-ledger) RUN_LEDGER="${2:?}"; shift 2 ;;
    --out) OUT_DIR="${2:?}"; shift 2 ;;
    --snapshot-root) SNAPSHOT_ROOT="${2:?}"; shift 2 ;;
    --run-id) RUN_ID="${2:?}"; shift 2 ;;
    --keep-snapshot) KEEP_SNAPSHOT=1; shift ;;
    -h|--help) usage; exit 0 ;;
    *) echo "unknown option: $1" >&2; usage >&2; exit 2 ;;
  esac
done

say() { printf '%s\n' "$*"; }
rule() { printf '%s\n' "--------------------------------------------------------------"; }

# ---------------------------------------------------------------- step 1: preflight
say "M1ND-10 G6 formal blind run"
say "mode: $([ "$DRY_RUN" -eq 1 ] && echo 'DRY RUN (public half only)' || echo 'FORMAL')"
rule
say "step 1/5  integrity preflight"

PREFLIGHT_ARGS=()
[ -n "$METRIC_SPEC" ] && PREFLIGHT_ARGS+=(--metric-spec "$METRIC_SPEC")
[ -n "$SEALED_CORPUS" ] && PREFLIGHT_ARGS+=(--sealed-corpus "$SEALED_CORPUS")
[ -n "$AUTHORITY_ASSEMBLY" ] && PREFLIGHT_ARGS+=(--authority-assembly "$AUTHORITY_ASSEMBLY")
[ -n "$AUTHORITY_PROVIDER" ] && PREFLIGHT_ARGS+=(--authority-provider "$AUTHORITY_PROVIDER")
[ -n "$CANDIDATE_BINARY" ] && PREFLIGHT_ARGS+=(--binary "$CANDIDATE_BINARY")
[ -n "$BASELINE_BINARY" ] && PREFLIGHT_ARGS+=(--baseline-binary "$BASELINE_BINARY")
[ -n "$BASELINE" ] && PREFLIGHT_ARGS+=(--baseline "$BASELINE")
[ -n "$BASELINE_RECEIPT" ] && PREFLIGHT_ARGS+=(--baseline-receipt "$BASELINE_RECEIPT")
[ -n "$RUN_LEDGER" ] && PREFLIGHT_ARGS+=(--run-ledger "$RUN_LEDGER")

set +e
bash "$PREFLIGHT" ${PREFLIGHT_ARGS[@]+"${PREFLIGHT_ARGS[@]}"}
PREFLIGHT_STATUS=$?
set -e

if [ "$PREFLIGHT_STATUS" -eq 1 ]; then
  rule
  say "ABORTED: the repository half of the preflight failed. Nothing ran."
  exit 1
fi
if [ "$DRY_RUN" -eq 0 ] && [ "$PREFLIGHT_STATUS" -ne 0 ]; then
  rule
  say "ABORTED: the formal run needs every owner-held input listed above."
  say "Re-run with --dry-run to exercise the public half in the meantime."
  exit 1
fi

if [ "$DRY_RUN" -eq 0 ]; then
  # Two owner-held values are digests, not files, so the preflight cannot see them.
  MISSING_VALUES=()
  [ -z "$SEALED_CORPUS_SELF_DIGEST" ] && MISSING_VALUES+=(--sealed-corpus-self-digest)
  [ -z "$AUTHORITY_ASSEMBLY_DIGEST" ] && MISSING_VALUES+=(--authority-assembly-digest)
  if [ ${#MISSING_VALUES[@]} -gt 0 ]; then
    rule
    say "ABORTED: missing required value(s): ${MISSING_VALUES[*]}"
    exit 1
  fi
fi

# ---------------------------------------------------------------- step 2: identity
rule
say "step 2/5  run identity"

CORPUS_ID="$(python3 -c 'import json,sys;print(json.load(open(sys.argv[1]))["corpus_id"])' \
  "$HELD_OUT/public/queries.json")"
HEAD_COMMIT="$(git -C "$REPO_ROOT" rev-parse HEAD)"
if [ "$DRY_RUN" -eq 0 ] && [ -n "$(git -C "$REPO_ROOT" status --porcelain)" ]; then
  say "ABORTED: the working tree is dirty. A formal run must bind one immutable"
  say "candidate revision; a dirty tree already produced a discarded run"
  say "(docs/benchmarks/m1nd10-g6-failed-b59-dirty-af02c141.json)."
  exit 1
fi
SYSTEM_REVISION="git:$HEAD_COMMIT"
if [ -z "$RUN_ID" ]; then
  RUN_ID="g6-$([ "$DRY_RUN" -eq 1 ] && echo dry || echo formal)-$(date -u +%Y%m%dT%H%M%SZ)-${HEAD_COMMIT:0:12}"
fi
if [ -z "$OUT_DIR" ]; then
  if [ "$DRY_RUN" -eq 1 ]; then
    # A dry run proves nothing claimable, so it never lands in the repository.
    OUT_DIR="${TMPDIR:-/tmp}/m1nd10-$RUN_ID"
  else
    OUT_DIR="$REPO_ROOT/docs/benchmarks/m1nd10-$RUN_ID"
  fi
fi
[ -z "$SNAPSHOT_ROOT" ] && SNAPSHOT_ROOT="${TMPDIR:-/tmp}/m1nd-g6-snapshot-$RUN_ID"

# The runner refuses any path with a symlink component, and a macOS $TMPDIR is
# reached through one. Resolve the parents; keep the leaves fresh and absent.
resolve_leaf() {
  mkdir -p "$(dirname "$1")"
  python3 -c 'import pathlib,sys
leaf = pathlib.Path(sys.argv[1])
print(leaf.parent.resolve() / leaf.name)' "$1"
}
OUT_DIR="$(resolve_leaf "$OUT_DIR")"
SNAPSHOT_ROOT="$(resolve_leaf "$SNAPSHOT_ROOT")"

say "corpus_id        $CORPUS_ID"
say "run_id           $RUN_ID"
say "system_revision  $SYSTEM_REVISION"
say "artifacts        $OUT_DIR"
say "snapshot root    $SNAPSHOT_ROOT"

mkdir -p "$OUT_DIR"
[ "$DRY_RUN" -eq 0 ] && mkdir -p "$OUT_DIR/runner-results"

# --------------------------------------------------- step 3: isolated source snapshot
rule
say "step 3/5  materialise and verify the isolated source snapshot"

if [ -e "$SNAPSHOT_ROOT" ]; then
  say "ABORTED: $SNAPSHOT_ROOT already exists; the snapshot root must be fresh."
  exit 1
fi

cleanup() {
  if [ "$KEEP_SNAPSHOT" -eq 0 ] && [ -d "$SNAPSHOT_ROOT" ]; then
    rm -rf "$SNAPSHOT_ROOT"
  fi
}
trap cleanup EXIT

G6_REPO_ROOT="$REPO_ROOT" \
G6_QUERIES="$HELD_OUT/public/queries.json" \
G6_SNAPSHOT_ROOT="$SNAPSHOT_ROOT" \
python3 - <<'MATERIALISE'
import json
import os
import pathlib
import subprocess
import sys

root = pathlib.Path(os.environ["G6_REPO_ROOT"])
queries = json.loads(pathlib.Path(os.environ["G6_QUERIES"]).read_text(encoding="utf-8"))
snapshot = pathlib.Path(os.environ["G6_SNAPSHOT_ROOT"])
manifest = queries["source_manifest"]
commit = manifest["source_commit"]

# The snapshot must be an isolated live tree outside every Git worktree, holding
# exactly the manifest file set — the runner rejects a missing or an extra file.
for candidate in (snapshot, *snapshot.parents):
    if os.path.lexists(candidate / ".git"):
        print(f"ABORTED: {candidate} is inside a Git worktree", file=sys.stderr)
        sys.exit(1)

snapshot.mkdir(parents=True, mode=0o700)
written = 0
for repo in manifest["repos"]:
    for entry in repo["files"]:
        destination = snapshot / repo["source_root"] / entry["path"]
        destination.parent.mkdir(parents=True, exist_ok=True)
        blob = subprocess.run(
            [
                "git",
                "-C",
                str(root),
                "cat-file",
                "blob",
                f"{commit}:{repo['source_root']}/{entry['path']}",
            ],
            capture_output=True,
            check=False,
        )
        if blob.returncode != 0:
            print(
                f"ABORTED: {repo['repo_id']}/{entry['path']} is absent at {commit}",
                file=sys.stderr,
            )
            sys.exit(1)
        destination.write_bytes(blob.stdout)
        written += 1
print(f"materialised {written} files from immutable Git objects at {commit}")
MATERIALISE

G6_REPO_ROOT="$REPO_ROOT" \
G6_HELD_OUT="$HELD_OUT" \
G6_GENERALIZATION="$GENERALIZATION" \
G6_SNAPSHOT_ROOT="$SNAPSHOT_ROOT" \
python3 - <<'VERIFY'
import json
import os
import pathlib
import sys

root = pathlib.Path(os.environ["G6_REPO_ROOT"])
sys.path.insert(0, str(root / "scripts" / "benchmark"))
import m1nd10_g6_blind_runner as runner  # noqa: E402

snapshot = pathlib.Path(os.environ["G6_SNAPSHOT_ROOT"])
for label, corpus in (
    ("held-out-v2", pathlib.Path(os.environ["G6_HELD_OUT"])),
    ("generalization-v2", pathlib.Path(os.environ["G6_GENERALIZATION"])),
):
    queries = json.loads((corpus / "public" / "queries.json").read_text(encoding="utf-8"))
    report = runner.verify_public_source_snapshot(queries, snapshot)
    print(
        f"{label:<18} checked={report['checked_files']} "
        f"missing={report['missing_files']} mismatched={report['digest_mismatches']} "
        f"extra={report['extra_files']} bytes={report['checked_bytes']} "
        f"lines={report['checked_lines']}"
    )
VERIFY

# ----------------------------------------------------------- step 4: run and score
rule
if [ "$DRY_RUN" -eq 1 ]; then
  say "step 4/5  exercise the pre-measurement pipeline (public half, zero labels)"

  G6_REPO_ROOT="$REPO_ROOT" \
  G6_HELD_OUT="$HELD_OUT" \
  G6_SNAPSHOT_ROOT="$SNAPSHOT_ROOT" \
  G6_OUT_DIR="$OUT_DIR" \
  python3 - <<'PIPELINE'
import argparse
import json
import os
import pathlib
import sys

root = pathlib.Path(os.environ["G6_REPO_ROOT"])
sys.path.insert(0, str(root / "scripts" / "benchmark"))
import m1nd10_g6_blind_runner as runner  # noqa: E402

held_out = pathlib.Path(os.environ["G6_HELD_OUT"])
snapshot = pathlib.Path(os.environ["G6_SNAPSHOT_ROOT"])
out_dir = pathlib.Path(os.environ["G6_OUT_DIR"])
queries = json.loads((held_out / "public" / "queries.json").read_text(encoding="utf-8"))

stages: list[dict[str, object]] = []


def stage(name: str, thunk) -> None:
    try:
        stages.append({"stage": name, "state": "PASS", "detail": thunk()})
    except Exception as error:  # noqa: BLE001 - a dry run reports, never raises
        stages.append({"stage": name, "state": "FAIL", "detail": str(error)})


stage(
    "validate_public_queries",
    lambda: f"{len(runner.validate_public_queries(queries))} tasks accepted; "
    f"self, corpus, manifest and file-set digests recomputed",
)
stage(
    "public_source_revision",
    lambda: runner.public_source_revision(queries),
)

plan_root = out_dir / "dry-run-plan"
plan_root.mkdir(parents=True, exist_ok=True)
namespace = argparse.Namespace(
    queries=held_out / "public" / "queries.json",
    metric_spec=root / "docs" / "benchmarks" / "m1nd10-g6-metric-spec-v1.json",
    binary=pathlib.Path("/bin/sh"),
    authority_provider=None,
    authority_assembly=None,
    source_root=snapshot,
    runtime_dir=plan_root / "runtime",
    registry_dir=plan_root / "registry",
    output=plan_root / "result.json",
    checkpoint=None,
)
stage(
    "validate_runner_paths",
    lambda: json.dumps(runner.validate_runner_paths(namespace)["paths"], sort_keys=True),
)


def owner_plan() -> str:
    specs = runner.build_owner_specs(
        queries, snapshot, namespace.runtime_dir, namespace.registry_dir, 0
    )
    return "; ".join(f"{spec.repo_id} -> {spec.root.name}" for spec in specs)


stage("build_owner_specs", owner_plan)

width = max(len(str(entry["stage"])) for entry in stages)
for entry in stages:
    print(f"{entry['state']:<6} {str(entry['stage']):<{width}}  {entry['detail']}")

(out_dir / "dry-run-pipeline.json").write_text(
    json.dumps(
        {"schema": "m1nd10-g6-dry-run-pipeline-v1", "stages": stages},
        indent=2,
        sort_keys=True,
    )
    + "\n",
    encoding="utf-8",
)
if any(entry["state"] == "FAIL" for entry in stages):
    sys.exit(1)
PIPELINE

  rule
  say "step 5/5  prove the ceremony stops at the owner's boundary"
  say ""
  say "The runner and the scorer are invoked for real. Every one must refuse: the"
  say "custody-bound inputs do not exist yet, and a dry run must never fake past"
  say "them. Their verbatim refusals are the proof. Each names the first blocker"
  say "it reaches; the preflight above is what enumerates them all."
  say ""

  DRY_PROBE="$OUT_DIR/dry-run-plan"
  set +e
  say "\$ m1nd10_g6_blind_runner.py --lane current ...   (custody-bound inputs absent)"
  python3 "$RUNNER" \
    --queries "$HELD_OUT/public/queries.json" \
    --metric-spec "$REPO_ROOT/docs/benchmarks/m1nd10-g6-metric-spec-v1.json" \
    --sealed-corpus-self-digest "sha256:$(printf '0%.0s' $(seq 64))" \
    --binary /bin/sh \
    --source-root "$SNAPSHOT_ROOT" \
    --runtime-dir "$DRY_PROBE/runtime" \
    --registry-dir "$DRY_PROBE/registry" \
    --expected-authority-assembly-digest "$(printf '0%.0s' $(seq 64))" \
    --lane current \
    --run-id "$RUN_ID-probe" \
    --system-revision "$SYSTEM_REVISION" \
    --output "$DRY_PROBE/result.json"
  RUNNER_EXIT=$?
  say "runner exit: $RUNNER_EXIT (non-zero is the expected, honest refusal)"
  say ""

  say "\$ m1nd10_g6_retrieval.py ...   (no sealed corpus, baseline, receipt, ledger)"
  python3 "$SCORER" \
    --spec "$REPO_ROOT/docs/benchmarks/m1nd10-g6-metric-spec-v1.json" \
    --public "$HELD_OUT/public/queries.json" \
    --cases "$HELD_OUT/operator-only/corpus.json" \
    --results "$DRY_PROBE/result.json" \
    --baseline "$DRY_PROBE/baseline.json" \
    --baseline-receipt "$DRY_PROBE/baseline-receipt.json" \
    --run-ledger "$DRY_PROBE/ledger.json" \
    --runner "$RUNNER" \
    --current-binary /bin/sh \
    --baseline-binary /bin/sh \
    --output "$DRY_PROBE/report.json"
  SCORER_EXIT=$?
  say "scorer exit: $SCORER_EXIT (non-zero is the expected, honest refusal)"
  say ""

  say "\$ m1nd10_g6_generalization_score.py ...   (supplemental guard, no labels)"
  python3 "$GENERALIZATION_SCORER" \
    --spec "$REPO_ROOT/docs/benchmarks/m1nd10-g6-metric-spec-v1.json" \
    --cases "$GENERALIZATION/operator-only/corpus.json" \
    --results "$DRY_PROBE/result.json" \
    --output "$DRY_PROBE/generalization-report.json"
  GENERALIZATION_EXIT=$?
  say "generalization scorer exit: $GENERALIZATION_EXIT"
  set -e

  rule
  say "DRY RUN COMPLETE"
  say ""
  say "Proved: the public corpus is intact, the isolated snapshot materialises"
  say "byte-exactly from immutable Git objects, the runner's own validators accept"
  say "the corpus and plan all four owners, and every scoring path fails closed"
  say "without the owner's custody-bound inputs."
  say ""
  say "NOT proved, and not claimable: no owner was spawned, no query was measured,"
  say "no label was read, no threshold was evaluated. G6 stays COMPONENT_PASS."
  say "artifacts: $OUT_DIR"
  exit 0
fi

say "step 4/5  formal blind run — 220 tasks over four owners"

python3 "$RUNNER" \
  --queries "$HELD_OUT/public/queries.json" \
  --metric-spec "$METRIC_SPEC" \
  --sealed-corpus-self-digest "$SEALED_CORPUS_SELF_DIGEST" \
  --binary "$CANDIDATE_BINARY" \
  --source-root "$SNAPSHOT_ROOT" \
  --runtime-dir "$OUT_DIR/runner-results/runtime" \
  --registry-dir "$OUT_DIR/runner-results/registry" \
  --authority-provider "$AUTHORITY_PROVIDER" \
  --authority-assembly "$AUTHORITY_ASSEMBLY" \
  --expected-authority-assembly-digest "$AUTHORITY_ASSEMBLY_DIGEST" \
  --lane current \
  --run-id "$RUN_ID" \
  --system-revision "$SYSTEM_REVISION" \
  --checkpoint "$OUT_DIR/runner-results/checkpoint.json" \
  --output "$OUT_DIR/runner-results/current.json"

rule
say "step 5/5  score against the ratified thresholds"

set +e
python3 "$SCORER" \
  --spec "$METRIC_SPEC" \
  --public "$HELD_OUT/public/queries.json" \
  --cases "$SEALED_CORPUS" \
  --results "$OUT_DIR/runner-results/current.json" \
  --baseline "$BASELINE" \
  --baseline-receipt "$BASELINE_RECEIPT" \
  --run-ledger "$RUN_LEDGER" \
  --runner "$RUNNER" \
  --current-binary "$CANDIDATE_BINARY" \
  --baseline-binary "$BASELINE_BINARY" \
  --output "$OUT_DIR/report.json" >/dev/null
SCORE_EXIT=$?
set -e

G6_REPORT="$OUT_DIR/report.json" \
G6_SPEC="$METRIC_SPEC" \
G6_OUT_DIR="$OUT_DIR" \
G6_RUN_ID="$RUN_ID" \
G6_SYSTEM_REVISION="$SYSTEM_REVISION" \
G6_RUNNER="$RUNNER" \
G6_SCORER="$SCORER" \
G6_CANDIDATE_BINARY="$CANDIDATE_BINARY" \
G6_BASELINE_BINARY="$BASELINE_BINARY" \
G6_RESULT="$OUT_DIR/runner-results/current.json" \
python3 - <<'VERDICT'
import hashlib
import json
import os
import pathlib

report = json.loads(pathlib.Path(os.environ["G6_REPORT"]).read_text(encoding="utf-8"))
spec = json.loads(pathlib.Path(os.environ["G6_SPEC"]).read_text(encoding="utf-8"))
out_dir = pathlib.Path(os.environ["G6_OUT_DIR"])
thresholds = spec.get("thresholds", {})
latency = spec.get("latency_slo_ms", {})
metrics = report.get("metrics", {})


def sha256_file(key: str) -> str | None:
    raw = os.environ.get(key, "")
    path = pathlib.Path(raw) if raw else None
    if path is None or not path.is_file():
        return None
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return "sha256:" + digest.hexdigest()


COMPARISONS = (
    ("top-5 anchor recall", "top5_anchor_recall", "top5_anchor_recall_min", ">=", thresholds),
    ("abstention recall", "abstention_recall", "abstention_recall_min", ">=", thresholds),
    (
        "wrong-ground act rate",
        "wrong_ground_action_rate",
        "wrong_ground_action_rate_max",
        "<=",
        thresholds,
    ),
    ("north p95 (ms)", "north_p95_ms", "north_p95", "<=", latency),
    ("seek p95 (ms)", "seek_p95_ms", "seek_p95", "<=", latency),
)

print("measured vs ratified")
for label, metric_key, spec_key, sense, source in COMPARISONS:
    measured = metrics.get(metric_key)
    ratified = source.get(spec_key)
    if measured is None or ratified is None:
        print(f"  {label:<24} measured=<absent>  ratified={ratified}")
        continue
    ok = measured >= ratified if sense == ">=" else measured <= ratified
    print(
        f"  {label:<24} measured={measured}  {sense} ratified={ratified}  "
        f"{'PASS' if ok else 'FAIL'}"
    )

p_value = metrics.get("regression_sign_test_p")
alpha = thresholds.get("regression_significance_alpha")
if p_value is not None:
    print(
        f"  {'paired regression':<24} p={p_value}  alpha={alpha}  "
        f"improvements={metrics.get('paired_improvements')} "
        f"regressions={metrics.get('paired_regressions')}"
    )

receipt = {
    "schema": "m1nd10-g6-formal-ceremony-receipt-v1",
    "run_id": os.environ["G6_RUN_ID"],
    "system_revision": os.environ["G6_SYSTEM_REVISION"],
    "status": report.get("status"),
    "claimable": report.get("claimable"),
    "corpus_id": report.get("corpus_id"),
    "blockers": report.get("blockers", []),
    "metrics": metrics,
    "checks": report.get("checks", {}),
    "bindings": {
        "metric_spec_digest": sha256_file("G6_SPEC"),
        "runner_digest": sha256_file("G6_RUNNER"),
        "scorer_digest": sha256_file("G6_SCORER"),
        "current_binary_digest": sha256_file("G6_CANDIDATE_BINARY"),
        "baseline_binary_digest": sha256_file("G6_BASELINE_BINARY"),
        "result_digest": sha256_file("G6_RESULT"),
        "report_digest": sha256_file("G6_REPORT"),
    },
    "gate_receipt_v1_minted": False,
    "gate_receipt_v1_note": (
        "A GateReceiptV1 (m1nd-control::release) binds a ratified custody floor and "
        "is minted by the release authority, not by this ceremony. This artifact is "
        "the evidence receipt the release authority consumes."
    ),
}
(out_dir / "receipt.json").write_text(
    json.dumps(receipt, indent=2, sort_keys=True) + "\n", encoding="utf-8"
)

print()
print(f"VERDICT: {report.get('status')} (claimable={report.get('claimable')})")
for blocker in report.get("blockers", []):
    print(f"  blocker: {blocker}")
print(f"report:  {out_dir / 'report.json'}")
print(f"receipt: {out_dir / 'receipt.json'}")
VERDICT

rule
if [ "$SCORE_EXIT" -eq 0 ]; then
  say "G6 formal blind run: PASS. Commit the report and the receipt; the raw"
  say "result under runner-results/ stays operator-only (already gitignored)."
else
  say "G6 formal blind run: not PASS. Preserve this evidence exactly as it is."
  say "The metric spec forbids re-running until pass on the same revision:"
  say "  same_revision_rerun_policy = one_sealed_run_only_no_rerun_until_pass"
fi
exit "$SCORE_EXIT"
