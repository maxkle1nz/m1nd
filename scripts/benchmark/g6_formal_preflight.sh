#!/usr/bin/env bash
# M1ND-10 G6 formal blind run — integrity preflight.
#
# Verifies everything that can be verified BEFORE the owner ceremony spends any
# runtime: the public corpus against its pinned digests, the frozen contracts,
# the materialisability of the isolated source snapshot, the runner/scorer
# toolchain, and the presence plus validity of every owner-held input.
#
# It never reads, parses, or prints label-bearing content. The sealed corpus is
# only ever hashed and compared against its pinned digest.
#
# Exit codes:
#   0  READY               — every check passed; the formal run may proceed.
#   3  READY_PUBLIC_ONLY   — the repository half is sound; owner-held inputs are
#                            missing. A dry run may proceed; a formal run may not.
#   1  FAIL                — a repository-side check failed. Fix before anything.
#   2  usage error.
set -euo pipefail

usage() {
  cat <<'USAGE'
usage: g6_formal_preflight.sh [options]

Owner-held inputs (all optional here; all required for a formal run):
  --metric-spec PATH         ratified metric spec v2 (m1nd10-g6-metric-spec-v2)
  --sealed-corpus PATH       sealed held-out corpus (hashed only, never parsed)
  --authority-assembly PATH  pinned production authority assembly manifest
  --authority-provider PATH  authority provider executable
  --binary PATH              pinned candidate m1nd-mcp binary (current lane)
  --baseline-binary PATH     pinned baseline m1nd-mcp binary
  --baseline PATH            previously sealed baseline result artifact
  --baseline-receipt PATH    outcome-blind baseline-ratification receipt
  --run-ledger PATH          sealed-run ledger

Other options:
  --json PATH                also write the machine-readable preflight report
  -h, --help                 this text
USAGE
}

REPO_ROOT="$(git -C "$(dirname "${BASH_SOURCE[0]}")" rev-parse --show-toplevel)"

METRIC_SPEC=""
SEALED_CORPUS=""
AUTHORITY_ASSEMBLY=""
AUTHORITY_PROVIDER=""
CANDIDATE_BINARY=""
BASELINE_BINARY=""
BASELINE=""
BASELINE_RECEIPT=""
RUN_LEDGER=""
JSON_OUT=""

while [ $# -gt 0 ]; do
  case "$1" in
    --metric-spec) METRIC_SPEC="${2:?}"; shift 2 ;;
    --sealed-corpus) SEALED_CORPUS="${2:?}"; shift 2 ;;
    --authority-assembly) AUTHORITY_ASSEMBLY="${2:?}"; shift 2 ;;
    --authority-provider) AUTHORITY_PROVIDER="${2:?}"; shift 2 ;;
    --binary) CANDIDATE_BINARY="${2:?}"; shift 2 ;;
    --baseline-binary) BASELINE_BINARY="${2:?}"; shift 2 ;;
    --baseline) BASELINE="${2:?}"; shift 2 ;;
    --baseline-receipt) BASELINE_RECEIPT="${2:?}"; shift 2 ;;
    --run-ledger) RUN_LEDGER="${2:?}"; shift 2 ;;
    --json) JSON_OUT="${2:?}"; shift 2 ;;
    -h|--help) usage; exit 0 ;;
    *) echo "unknown option: $1" >&2; usage >&2; exit 2 ;;
  esac
done

export G6_REPO_ROOT="$REPO_ROOT"
export G6_METRIC_SPEC="$METRIC_SPEC"
export G6_SEALED_CORPUS="$SEALED_CORPUS"
export G6_AUTHORITY_ASSEMBLY="$AUTHORITY_ASSEMBLY"
export G6_AUTHORITY_PROVIDER="$AUTHORITY_PROVIDER"
export G6_CANDIDATE_BINARY="$CANDIDATE_BINARY"
export G6_BASELINE_BINARY="$BASELINE_BINARY"
export G6_BASELINE="$BASELINE"
export G6_BASELINE_RECEIPT="$BASELINE_RECEIPT"
export G6_RUN_LEDGER="$RUN_LEDGER"
export G6_JSON_OUT="$JSON_OUT"

exec python3 - <<'PREFLIGHT'
import hashlib
import json
import os
import pathlib
import subprocess
import sys

ROOT = pathlib.Path(os.environ["G6_REPO_ROOT"]).resolve()
BENCH = ROOT / "docs" / "benchmarks"
HELD_OUT = BENCH / "m1nd10-g6-held-out-v2"
GENERALIZATION = BENCH / "m1nd10-g6-generalization-v2"
METRIC_SPEC_V1 = BENCH / "m1nd10-g6-metric-spec-v1.json"
RUNNER = ROOT / "scripts" / "benchmark" / "m1nd10_g6_blind_runner.py"
SCORER = ROOT / "scripts" / "benchmark" / "m1nd10_g6_retrieval.py"

# Frozen-contract pins. PRD/UML repeat .github/workflows/ci.yml `contract-gates`
# verbatim; the metric-spec pin is this ceremony's own, on the same mechanism.
FROZEN_PINS = {
    "docs/M1ND-10-PRD.md": (
        "2745560daf6e5cf6237b84663f895e81e2c4979de4190dfef649b032b680f87b"
    ),
    "docs/M1ND-10-UML.md": (
        "d5bc29776f516c300cb1a0668f0a53844286f75395fd0ad1e875b52ea3a067a5"
    ),
    "docs/benchmarks/m1nd10-g6-metric-spec-v1.json": (
        "4b22e391154d219de4efbb8787ef8340a61d1b27e093bc9c9bc3e4fdb13ca8f0"
    ),
}

PASS, FAIL, MISSING = "PASS", "FAIL", "OWNER_INPUT_MISSING"
checks: list[dict[str, object]] = []


def record(name: str, state: str, detail: str) -> None:
    checks.append({"check": name, "state": state, "detail": detail})


def sha256_file(path: pathlib.Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def canonical(value: object) -> bytes:
    return json.dumps(
        value, sort_keys=True, separators=(",", ":"), ensure_ascii=False
    ).encode("utf-8")


def load(path: pathlib.Path) -> dict:
    return json.loads(path.read_text(encoding="utf-8"))


# 1. Runner and scorer must import; the ceremony reuses their law, never a copy.
sys.path.insert(0, str(RUNNER.parent))
try:
    import m1nd10_g6_blind_runner as runner_module

    record(
        "toolchain.runner_import",
        PASS,
        f"python {sys.version_info.major}.{sys.version_info.minor}; "
        f"m1nd10_g6_blind_runner imported from {RUNNER}",
    )
except Exception as error:  # noqa: BLE001 - preflight reports, never raises
    runner_module = None
    record("toolchain.runner_import", FAIL, f"{type(error).__name__}: {error}")

try:
    import m1nd10_g6_retrieval as scorer_module

    record(
        "toolchain.scorer_import",
        PASS,
        f"m1nd10_g6_retrieval imported from {SCORER}",
    )
except Exception as error:  # noqa: BLE001
    scorer_module = None
    record("toolchain.scorer_import", FAIL, f"{type(error).__name__}: {error}")

for label, path in (("runner", RUNNER), ("scorer", SCORER)):
    if path.is_file():
        record(
            f"toolchain.{label}_digest",
            PASS,
            f"sha256:{sha256_file(path)} (bound into every receipt)",
        )
    else:
        record(f"toolchain.{label}_digest", FAIL, f"{path} is absent")


# 2. Public artifacts against the pinned digest manifests.
def verify_digest_manifest(corpus_dir: pathlib.Path, label: str) -> dict | None:
    digests_path = corpus_dir / "manifest" / "digests.json"
    if not digests_path.is_file():
        record(f"{label}.digests_manifest", FAIL, f"{digests_path} is absent")
        return None
    document = load(digests_path)
    if "self_digest" in document:
        recomputed = "sha256:" + hashlib.sha256(
            canonical({k: v for k, v in document.items() if k != "self_digest"})
        ).hexdigest()
        state = PASS if recomputed == document["self_digest"] else FAIL
        record(
            f"{label}.digests_self_digest",
            state,
            f"{document['self_digest']} recomputed {'exactly' if state == PASS else 'to ' + recomputed}",
        )
    public_ok = True
    owner_held: list[str] = []
    for artifact in document.get("artifacts", []):
        relative = artifact["path"]
        path = corpus_dir / relative
        if relative.startswith("operator-only/"):
            owner_held.append(relative)
            continue
        if not path.is_file():
            record(f"{label}.artifact:{relative}", FAIL, "absent from the checkout")
            public_ok = False
            continue
        observed = "sha256:" + sha256_file(path)
        size_ok = "bytes" not in artifact or path.stat().st_size == artifact["bytes"]
        if observed == artifact["sha256"] and size_ok:
            record(f"{label}.artifact:{relative}", PASS, observed)
        else:
            record(
                f"{label}.artifact:{relative}",
                FAIL,
                f"pinned {artifact['sha256']} != observed {observed}",
            )
            public_ok = False
    record(
        f"{label}.public_artifact_set",
        PASS if public_ok else FAIL,
        "every public artifact matches its pinned digest"
        if public_ok
        else "at least one public artifact drifted from its pin",
    )
    if owner_held:
        record(
            f"{label}.operator_only_artifacts",
            MISSING if not all((corpus_dir / p).is_file() for p in owner_held) else PASS,
            "owner-held, never read by this preflight: " + ", ".join(sorted(owner_held)),
        )
    return document


held_out_digests = verify_digest_manifest(HELD_OUT, "held_out_v2")
verify_digest_manifest(GENERALIZATION, "generalization_v2")

# 3. The corpus must satisfy the runner's own law, not a restatement of it.
queries_path = HELD_OUT / "public" / "queries.json"
queries = load(queries_path) if queries_path.is_file() else None
if runner_module is not None and queries is not None:
    try:
        tasks = runner_module.validate_public_queries(queries)
        record(
            "held_out_v2.runner_validation",
            PASS,
            f"validate_public_queries accepted {len(tasks)} tasks; "
            f"corpus_id={queries['corpus_id']}",
        )
    except Exception as error:  # noqa: BLE001
        record("held_out_v2.runner_validation", FAIL, str(error))
else:
    record("held_out_v2.runner_validation", FAIL, "runner or corpus unavailable")

# 4. Frozen contracts: pinned bytes, and law-not-world content.
for relative, pin in FROZEN_PINS.items():
    path = ROOT / relative
    if not path.is_file():
        record(f"frozen.{relative}", FAIL, "absent")
        continue
    observed = sha256_file(path)
    record(
        f"frozen.{relative}",
        PASS if observed == pin else FAIL,
        observed if observed == pin else f"pinned {pin} != observed {observed}",
    )

CLOCK_KEYS = ("generated_at", "measured_at", "observed_at", "run_at", "now_ms", "now")
WORLD_KEYS = ("instance_id", "fixture_id", "fixture", "task_count", "measurements")


def frozen_lint(path: pathlib.Path, label: str) -> None:
    """A frozen contract declares LAW; world state belongs outside it."""
    if not path.is_file():
        record(f"frozen_lint.{label}", MISSING, f"{path} is absent")
        return
    document = load(path)
    findings: list[str] = []

    def walk(node: object, trail: str) -> None:
        if isinstance(node, dict):
            for key, value in node.items():
                where = f"{trail}.{key}" if trail else key
                if key in CLOCK_KEYS:
                    findings.append(f"wall clock at {where}")
                if key in WORLD_KEYS:
                    findings.append(f"world state at {where}")
                if key.endswith("_at") and isinstance(value, str) and "T" in value:
                    findings.append(f"instant (not a date) at {where}")
                walk(value, where)
        elif isinstance(node, list):
            for index, value in enumerate(node):
                walk(value, f"{trail}[{index}]")
        elif isinstance(node, str) and node.startswith("/"):
            findings.append(f"absolute filesystem path at {trail}")

    walk(document, "")
    record(
        f"frozen_lint.{label}",
        PASS if not findings else FAIL,
        "declares law only (date-only ratification provenance and repo-relative "
        "references are the declared carve-outs)"
        if not findings
        else "; ".join(sorted(set(findings))),
    )


frozen_lint(METRIC_SPEC_V1, "metric_spec_v1")

# 5. The isolated source snapshot must be materialisable from immutable objects.
if queries is not None:
    manifest = queries["source_manifest"]
    commit = manifest.get("source_commit")
    if commit is None:
        record("snapshot.source_commit", FAIL, "public manifest has no source_commit")
    else:
        probe = subprocess.run(
            ["git", "-C", str(ROOT), "cat-file", "-t", commit],
            capture_output=True,
            text=True,
            check=False,
        )
        if probe.returncode != 0 or probe.stdout.strip() != "commit":
            record(
                "snapshot.source_commit",
                FAIL,
                f"corpus commit {commit} is not present in this object store",
            )
        else:
            expected = sum(len(repo["files"]) for repo in manifest["repos"])
            listing = subprocess.run(
                ["git", "-C", str(ROOT), "ls-tree", "-r", "--name-only", commit],
                capture_output=True,
                text=True,
                check=False,
            )
            available = set(listing.stdout.splitlines())
            wanted = {
                f"{repo['source_root']}/{entry['path']}"
                for repo in manifest["repos"]
                for entry in repo["files"]
            }
            absent = sorted(wanted - available)
            record(
                "snapshot.source_commit",
                PASS if not absent else FAIL,
                f"commit {commit} present; {expected} manifest files resolvable"
                if not absent
                else f"{len(absent)} manifest files absent at {commit}: "
                + ", ".join(absent[:5]),
            )

# 6. Owner-held inputs.
def owner_input(env_key: str, label: str, executable: bool = False) -> pathlib.Path | None:
    raw = os.environ.get(env_key, "")
    if not raw:
        record(f"owner.{label}", MISSING, "not supplied")
        return None
    path = pathlib.Path(raw).expanduser().resolve()
    if not path.is_file():
        record(f"owner.{label}", FAIL, f"{path} is not a regular file")
        return None
    if executable and not os.access(path, os.X_OK):
        record(f"owner.{label}", FAIL, f"{path} is not executable")
        return None
    record(f"owner.{label}", PASS, f"{path} sha256:{sha256_file(path)}")
    return path


owner_metric_spec = owner_input("G6_METRIC_SPEC", "metric_spec_v2")
if owner_metric_spec is not None and runner_module is not None:
    try:
        calibration = runner_module.validate_metric_spec_for_runner(
            load(owner_metric_spec)
        )
        record(
            "owner.metric_spec_v2_contract",
            PASS,
            "accepted by the runner; calibration gate "
            + json.dumps(calibration, sort_keys=True),
        )
        frozen_lint(owner_metric_spec, "metric_spec_v2")
    except Exception as error:  # noqa: BLE001
        record("owner.metric_spec_v2_contract", FAIL, str(error))
else:
    record(
        "owner.metric_spec_v2_contract",
        MISSING,
        "the checked-in metric spec is v1; the runner and the scorer both require "
        "schema m1nd10-g6-metric-spec-v2 with a calibration gate, an outcome-blind "
        "ratification, and an authority receipt digest",
    )

sealed = owner_input("G6_SEALED_CORPUS", "sealed_corpus")
if sealed is not None and held_out_digests is not None:
    pinned = next(
        (
            entry["sha256"]
            for entry in held_out_digests["artifacts"]
            if entry["path"] == "operator-only/corpus.json"
        ),
        None,
    )
    observed = "sha256:" + sha256_file(sealed)
    record(
        "owner.sealed_corpus_pin",
        PASS if observed == pinned else FAIL,
        "matches the pinned sealed-corpus digest (hashed, never parsed)"
        if observed == pinned
        else f"pinned {pinned} != observed {observed}",
    )
elif sealed is None:
    record("owner.sealed_corpus_pin", MISSING, "sealed corpus not supplied")

assembly = owner_input("G6_AUTHORITY_ASSEMBLY", "authority_assembly")
owner_input("G6_AUTHORITY_PROVIDER", "authority_provider", executable=True)
candidate = owner_input("G6_CANDIDATE_BINARY", "candidate_binary", executable=True)
owner_input("G6_BASELINE_BINARY", "baseline_binary", executable=True)
owner_input("G6_BASELINE", "baseline_result")
owner_input("G6_BASELINE_RECEIPT", "baseline_ratification_receipt")
owner_input("G6_RUN_LEDGER", "sealed_run_ledger")

if assembly is not None:
    document = load(assembly)
    production = document.get("production_authority_assembly")
    kind = document.get("provider_kind")
    record(
        "owner.production_authority",
        PASS if production is True and kind == "production" else FAIL,
        f"provider_kind={kind} production_authority_assembly={production}"
        + (
            ""
            if production is True and kind == "production"
            else "; the formal run refuses a non-production assembly "
            "(m1nd10_g6_blind_runner.py: 'formal run requires a pinned production "
            "authority assembly')"
        ),
    )
else:
    record(
        "owner.production_authority",
        MISSING,
        "no pinned production authority assembly; it is minted under the G9 custody "
        "floor (docs/M1ND-10-G9-CUSTODY-DECISION-20260721.md, amendment G9-A1)",
    )

if candidate is not None:
    probe = subprocess.run(
        [str(candidate), "--verify-authorization-receipt"],
        input="{}",
        capture_output=True,
        text=True,
        check=False,
    )
    record(
        "owner.candidate_verifier_mode",
        PASS if probe.returncode in (1, 2) else FAIL,
        f"--verify-authorization-receipt answered exit {probe.returncode} on an "
        "invalid request (a refusal proves the exclusive offline mode exists)",
    )
else:
    record(
        "owner.candidate_verifier_mode",
        MISSING,
        "no pinned candidate binary; freeze one immutable candidate first",
    )

# 7. Verdict.
failures = [c for c in checks if c["state"] == FAIL]
absent = [c for c in checks if c["state"] == MISSING]
if failures:
    status, exit_code = "FAIL", 1
elif absent:
    status, exit_code = "READY_PUBLIC_ONLY", 3
else:
    status, exit_code = "READY", 0

width = max(len(str(c["check"])) for c in checks)
print("M1ND-10 G6 formal blind run — integrity preflight")
print(f"repository: {ROOT}")
print("-" * (width + 24))
for check in checks:
    print(f"{check['state']:<19} {str(check['check']):<{width}}  {check['detail']}")
print("-" * (width + 24))
print(f"STATUS: {status}")
if failures:
    print("\nrepository-side failures — fix before any run:")
    for check in failures:
        print(f"  - {check['check']}: {check['detail']}")
if absent:
    print("\nowner-held inputs still missing — a formal run cannot start:")
    for check in absent:
        print(f"  - {check['check']}: {check['detail']}")

report = {
    "schema": "m1nd10-g6-formal-preflight-v1",
    "status": status,
    "repository": str(ROOT),
    "checks": checks,
}
if os.environ.get("G6_JSON_OUT"):
    out = pathlib.Path(os.environ["G6_JSON_OUT"])
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print(f"\npreflight report: {out}")

sys.exit(exit_code)
PREFLIGHT
