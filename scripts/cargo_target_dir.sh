#!/usr/bin/env bash
# Print the CARGO_TARGET_DIR that belongs to the checkout this is run from.
#
#   export CARGO_TARGET_DIR="$(scripts/cargo_target_dir.sh)"
#
# WHY THIS EXISTS
# ---------------
# Cargo's metadata hash does not encode the source path, so two checkouts of
# this repo building into ONE target dir produce artifacts with the SAME name.
# Measured twice on 2026-07-27/28 across parallel worktrees: two of them emitted
# the identical test binary path, one worktree's gate linked the other's
# artifact, and one executor killed the other's test process believing it a
# duplicate. The visible half was a red run — a worktree whose
# `m1nd-control/src/action_catalog.rs` held 169 entries linked a sibling's 172
# and failed 47 tests with `CatalogDrift`, cured by `touch` alone, with no code
# change. The dangerous half is the false GREEN: a gate that passes on another
# checkout's binary makes every "proved" claim in a parallel burst unfalsifiable.
#
# A gate is only evidence about the tree it was run in. This makes that
# mechanical instead of remembered.
#
# WHAT IT DOES NOT DO
# -------------------
# It is not wired into a checked-in `.cargo/config.toml`. That would impose a
# machine-specific build layout on every contributor and every CI runner (which
# already build in isolated checkouts and have no such collision). This is a
# workflow mechanism for parallel local worktrees; the wiring is the export
# above.
#
# Everything stays under `~/.m1nd-build-cache/`, so the disk rule that names
# that one directory as the auto-deletable build cache still covers all of it.
set -euo pipefail

cache_root="$HOME/.m1nd-build-cache"

# Outside a git checkout there is nothing to separate: keep the historical path.
if ! toplevel="$(git rev-parse --show-toplevel 2>/dev/null)" || [ -z "$toplevel" ]; then
  printf '%s\n' "$cache_root/target"
  exit 0
fi

# Resolve through symlinks so one checkout can never hash to two directories.
toplevel="$(CDPATH='' cd -- "$toplevel" && pwd -P)"

# The checkout PATH is the discriminator — a linked worktree has its own, while
# branch, HEAD state (detached included) and the shared object store do not
# distinguish builds and must not enter the digest. Hashing via git keeps this
# dependency-free: `shasum` and `sha256sum` are not both present on every OS
# this repo is developed on, and git is already required to have gotten here.
digest="$(printf '%s' "$toplevel" | git hash-object --stdin)"

printf '%s\n' "$cache_root/target-${digest:0:12}"
