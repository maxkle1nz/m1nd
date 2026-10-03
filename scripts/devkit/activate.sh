#!/usr/bin/env bash
# Source this file to select the portable m1nd contributor environment.
# It supports bash and zsh; it never modifies an rc file or HOME.

if [[ -n "${ZSH_VERSION-}" ]]; then
  m1nd_devkit_source="${(%):-%N}"
  case "${ZSH_EVAL_CONTEXT-}" in
    *:file) ;;
    *) printf 'Use: source %s\n' "$m1nd_devkit_source" >&2; return 2 2>/dev/null || exit 2 ;;
  esac
else
  m1nd_devkit_source="${BASH_SOURCE[0]}"
  if [[ "$m1nd_devkit_source" == "$0" ]]; then
    printf 'Use: source %s\n' "$m1nd_devkit_source" >&2
    exit 2
  fi
fi

m1nd_devkit_activate() {
  local script_dir checked_root checkout toplevel state_base state_id state state_real home_real python_bin node_bin target_dir
  script_dir="$(CDPATH='' cd -- "$(dirname -- "$m1nd_devkit_source")" && pwd -P)"
  checked_root="$(CDPATH='' cd -- "$script_dir/../.." && pwd -P)"
  checkout="${M1ND_CHECKOUT:-$checked_root}"
  if ! checkout="$(CDPATH='' cd -- "$checkout" && pwd -P)"; then
    printf 'm1nd-dev: checkout is not accessible: %s\n' "$checkout" >&2
    return 1
  fi
  if ! toplevel="$(git -C "$checkout" rev-parse --show-toplevel 2>/dev/null)"; then
    printf 'm1nd-dev: checkout is not a Git worktree: %s\n' "$checkout" >&2
    return 1
  fi
  toplevel="$(CDPATH='' cd -- "$toplevel" && pwd -P)"
  if [[ "$toplevel" != "$checkout" ]]; then
    printf 'm1nd-dev: M1ND_CHECKOUT must name the Git toplevel, not a nested directory: %s\n' "$checkout" >&2
    return 1
  fi
  state_id="$(printf '%s' "$checkout" | git hash-object --stdin)"
  state_base="${M1ND_DEV_STATE_BASE:-${XDG_STATE_HOME:-$HOME/.local/state}/m1nd-devkit}"
  if [[ "$state_base" != /* ]]; then
    printf 'm1nd-dev: M1ND_DEV_STATE_BASE must be an absolute path\n' >&2
    return 1
  fi
  state="${M1ND_DEV_STATE:-$state_base/${state_id:0:16}}"
  if [[ "$state" != /* ]]; then
    printf 'm1nd-dev: M1ND_DEV_STATE must be an absolute path\n' >&2
    return 1
  fi
  python_bin="${M1ND_PYTHON:-python3}"
  command -v "$python_bin" >/dev/null 2>&1 || { printf 'm1nd-dev: Python is required to canonicalize private state\n' >&2; return 1; }
  python_bin="$(command -v "$python_bin")"
  node_bin="${M1ND_NODE:-node}"
  command -v "$node_bin" >/dev/null 2>&1 || { printf 'm1nd-dev: Node is required for the contributor kit\n' >&2; return 1; }
  node_bin="$(command -v "$node_bin")"
  state_real="$("$python_bin" -c 'import os, sys; print(os.path.realpath(sys.argv[1]))' "$state")" || return 1
  home_real="$("$python_bin" -c 'import os, sys; print(os.path.realpath(sys.argv[1]))' "$HOME")" || return 1
  case "$state_real" in
    "$checkout"|"$checkout"/*)
      printf 'm1nd-dev: M1ND_DEV_STATE must be outside the checkout\n' >&2
      return 1
      ;;
    "$home_real/.m1nd"|"$home_real/.m1nd"/*)
      printf 'm1nd-dev: M1ND_DEV_STATE must not use the live ~/.m1nd runtime\n' >&2
      return 1
      ;;
  esac
  export M1ND_CHECKOUT="$checkout"
  export M1ND_DEVKIT_ROOT="$checked_root"
  export M1ND_DEV_STATE="$state_real"
  export M1ND_DEVKIT_RUNTIME_DIR="$M1ND_DEV_STATE/runtime"
  # The native launcher canonicalizes its owner registry below the private runtime.
  # Keep every kit path on that canonical registry so serve, discovery, and CLI agree.
  export M1ND_DEVKIT_REGISTRY_DIR="$M1ND_DEVKIT_RUNTIME_DIR/registry"
  export M1ND_DEVKIT_AGENT_CACHE_DIR="$M1ND_DEV_STATE/agent-cache"
  export M1ND_DEVKIT_WORK_DIR="$M1ND_DEV_STATE/work"
  export M1ND_DEVKIT_BINARY_STATE_DIR="$M1ND_DEV_STATE/binary-state"
  export M1ND_DEV_MODEL_DIR="${M1ND_DEV_MODEL_DIR:-${M1ND_EMBED_MODEL:-$M1ND_DEV_STATE/models/potion-base-8m}}"
  export M1ND_NODE="$node_bin"
  export M1ND_PYTHON="$python_bin"
  export M1ND_NODE_MAJOR="${M1ND_NODE_MAJOR:-22}"
  export M1ND_PYTHON_MINOR="${M1ND_PYTHON_MINOR:-3.12}"
  export RUSTUP_TOOLCHAIN="${M1ND_RUST_TOOLCHAIN:-1.98.1}"
  export M1ND_DEV_PROFILE="${M1ND_DEV_PROFILE:-fast}"
  if ! target_dir="$(cd "$checkout" && bash scripts/cargo_target_dir.sh)"; then
    printf 'm1nd-dev: canonical cargo target helper failed\n' >&2
    return 1
  fi
  export CARGO_TARGET_DIR="$target_dir"
  export PATH="$(dirname "$node_bin"):$(dirname "$python_bin"):$checkout/scripts:$PATH"
}

m1nd_devkit_activate
m1nd_devkit_status=$?
unset -f m1nd_devkit_activate
unset m1nd_devkit_source
return "$m1nd_devkit_status" 2>/dev/null || exit "$m1nd_devkit_status"
