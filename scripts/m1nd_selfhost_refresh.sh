#!/usr/bin/env bash
# Rebuild the m1nd binary this machine's agents actually talk to, from the
# commit this repo is actually on, and prove afterwards that the two match.
#
# WHY THIS EXISTS
# ---------------
# An agent that develops m1nd while running an OLD m1nd cannot test its own
# work. On 2026-07-27 a full night of Windows fixes shipped from a session whose
# runtime was `m1nd-mcp 1.4.0 (b41883c9…-dirty)` — a binary built from a working
# tree that never became a commit, carrying an EMPTY graph (0 nodes, 0 edges).
# Every "use m1nd to orient" step in the doctrine was silently a no-op, and the
# only reason it surfaced was the owner asking whether m1nd was even working.
#
# The rule the owner set from that: you do not work on what you cannot test, and
# keeping the binary current is not a thing to remember — it is a thing that runs.
#
# WHAT IT DOES NOT DO
# -------------------
# It does not ingest. A fresh binary with an empty graph is still blind, but that
# is a separate, heavier decision (~29k nodes) that belongs to the owner, not to
# a refresh script. It reports the emptiness instead of silently fixing it.
set -uo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TARGET_DIR="${CARGO_TARGET_DIR:-$HOME/.m1nd-build-cache/target}"
DEST="${M1ND_INSTALL_PATH:-$HOME/.local/bin/m1nd-mcp}"

fail() { printf '\033[31m✗ %s\033[0m\n' "$1" >&2; exit 1; }
ok()   { printf '\033[32m✓ %s\033[0m\n' "$1"; }
note() { printf '  %s\n' "$1"; }

cd "$REPO" || fail "repo not found: $REPO"

head_sha="$(git rev-parse HEAD)" || fail "not a git repo"
dirty=""
[ -n "$(git status --porcelain)" ] && dirty=" (WORKING TREE DIRTY — the binary will not match any commit)"

echo "═══ building m1nd-mcp from ${head_sha:0:8}${dirty} ═══"
CARGO_TARGET_DIR="$TARGET_DIR" cargo build --release -p m1nd-mcp || fail "build failed"

built="$TARGET_DIR/release/m1nd-mcp"
[ -x "$built" ] || fail "expected a binary at $built"

mkdir -p "$(dirname "$DEST")"
# Replace by rename so a running process keeps its open inode instead of being
# corrupted mid-read; the next spawn picks up the new file.
cp "$built" "$DEST.new" || fail "could not stage $DEST.new"
chmod +x "$DEST.new"
mv -f "$DEST.new" "$DEST" || fail "could not install $DEST"
ok "installed → $DEST"

echo "═══ proving the install matches the source ═══"
version_line="$("$DEST" --version 2>/dev/null | head -1)"
[ -n "$version_line" ] || fail "the installed binary does not answer --version"
note "$version_line"

# `m1nd-mcp 1.5.0 (<sha>[-dirty])` — compare the sha it reports to this HEAD.
binary_sha="$(printf '%s' "$version_line" | sed -nE 's/.*\(([0-9a-f]{7,40})(-dirty)?\).*/\1/p')"
[ -n "$binary_sha" ] || fail "could not read a git sha out of: $version_line"

if [ "$binary_sha" != "$head_sha" ]; then
  fail "installed binary reports $binary_sha but this repo is at $head_sha"
fi
case "$version_line" in
  *-dirty*) printf '\033[33m! built from a dirty tree — reproducible only on this machine\033[0m\n' ;;
esac
ok "binary sha == repo HEAD (${head_sha:0:8})"

echo "═══ what the fresh binary actually knows ═══"
# A current binary over an empty graph is still a blind agent. Say so.
runtime_root="${M1ND_RUNTIME_DIR:-$REPO/.m1nd}"
snapshot="$runtime_root/graph_snapshot.json"
if [ -s "$snapshot" ]; then
  nodes="$(python3 -c "import json,sys;print(len(json.load(open(sys.argv[1])).get('nodes',[])))" "$snapshot" 2>/dev/null || echo '?')"
  if [ "$nodes" = "0" ]; then
    printf '\033[33m! graph is EMPTY (0 nodes) at %s\033[0m\n' "$snapshot"
    note "a current binary over an empty graph still answers nothing — run an ingest"
  else
    ok "graph: $nodes nodes at $snapshot"
  fi
else
  printf '\033[33m! no graph snapshot at %s — this runtime has never been ingested\033[0m\n' "$snapshot"
fi

echo
ok "done — restart the MCP host so it spawns the new binary"
