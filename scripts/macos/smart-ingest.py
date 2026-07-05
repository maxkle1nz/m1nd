#!/usr/bin/env python3
"""
smart-ingest.py — m1nd intelligent namespace ingest
Ingests only relevant project namespaces, skipping vendored pods and noise.

Usage:
    python3 smart-ingest.py              # Full ingest all namespaces
    python3 smart-ingest.py project-a    # Single namespace
    python3 smart-ingest.py --incremental  # All namespaces, incremental
"""

import sys
import json
import time
import urllib.request
import urllib.error
import os

M1ND_PORT = int(os.environ.get("M1ND_PORT", "1337"))
# Base directory holding the repos you want ingested. Point this at your own
# workspace and list one entry per repo below.
WORKSPACE = os.path.expanduser(os.environ.get("M1ND_WORKSPACE", "~/workspace"))

# Ordered by priority — highest first
NAMESPACES = [
    {"name": "project-a",  "path": f"{WORKSPACE}/project-a",  "mode": "replace"},
    {"name": "openclaw",   "path": f"{WORKSPACE}/openclaw",   "mode": "merge"},
    {"name": "project-b",  "path": f"{WORKSPACE}/project-b",  "mode": "merge"},
]


def ingest_namespace(ns: dict, incremental: bool = False) -> dict:
    payload = json.dumps({
        "path": ns["path"],
        "agent_id": "antigravity",
        "adapter": "code",
        "mode": ns["mode"],
        "namespace": ns["name"],
        "incremental": incremental,
    }).encode()

    req = urllib.request.Request(
        f"http://127.0.0.1:{M1ND_PORT}/api/tools/ingest",
        data=payload,
        headers={"Content-Type": "application/json"},
        method="POST"
    )
    try:
        with urllib.request.urlopen(req, timeout=120) as resp:
            raw = json.loads(resp.read())
            return raw.get("result", raw)
    except Exception as e:
        return {"error": str(e)}


def main():
    args = sys.argv[1:]
    incremental = "--incremental" in args
    target = next((a for a in args if not a.startswith("--")), None)

    namespaces = [ns for ns in NAMESPACES if target is None or ns["name"] == target]

    if not namespaces:
        print(f"❌ namespace '{target}' não encontrado")
        sys.exit(1)

    print(f"🧠 m1nd smart-ingest — {'incremental' if incremental else 'full'}")
    print(f"   {len(namespaces)} namespace(s) a ingestar\n")

    total_nodes = 0
    total_files = 0
    t0 = time.time()

    for i, ns in enumerate(namespaces):
        mode_label = "replace" if ns["mode"] == "replace" else "merge"
        print(f"[{i+1}/{len(namespaces)}] {ns['name']} ({mode_label})...", end=" ", flush=True)
        result = ingest_namespace(ns, incremental)

        if "error" in result:
            print(f"❌ {result['error']}")
            continue

        nodes = result.get("node_count", 0)
        files = result.get("files_parsed", 0)
        ms = result.get("elapsed_ms", 0)
        total_nodes = nodes
        total_files += files
        print(f"✅ {files} arquivos → {nodes} nós cumulativos ({ms:.0f}ms)")

    elapsed = time.time() - t0
    print(f"\n✨ Concluído em {elapsed:.1f}s")
    print(f"   Total: {total_nodes:,} nós | {total_files:,} arquivos parseados")
    print(f"   Grafo salvo em ~/.m1nd/graph.json")


if __name__ == "__main__":
    main()
