#!/usr/bin/env bash
# Portable contributor commands. No installer, network fetch, global state, or
# port fallback is hidden here: every run uses checkout-private state.
set -euo pipefail

script_dir="$(CDPATH='' cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
# shellcheck source=activate.sh
source "$script_dir/activate.sh"

fail() { printf 'm1nd-dev: %s\n' "$*" >&2; exit 1; }
case "$(uname -s)" in MINGW*|MSYS*|CYGWIN*) fail 'native Windows is unsupported; use a Linux filesystem under WSL';; esac
need() { command -v "$1" >/dev/null 2>&1 || fail "required command is unavailable: $1"; }
check_node() { [[ "$("$M1ND_NODE" -p 'process.versions.node.split(".")[0]')" == "$M1ND_NODE_MAJOR" ]] || fail "Node major must be $M1ND_NODE_MAJOR (override M1ND_NODE_MAJOR explicitly)"; }
check_python() { [[ "$("$M1ND_PYTHON" -c 'import sys; print(f"{sys.version_info.major}.{sys.version_info.minor}")')" == "$M1ND_PYTHON_MINOR" ]] || fail "Python must be $M1ND_PYTHON_MINOR (override M1ND_PYTHON_MINOR explicitly)"; }
check_rust() { [[ "$(rustc --version)" == *" $RUSTUP_TOOLCHAIN "* ]] || fail "Rust must be $RUSTUP_TOOLCHAIN (override M1ND_RUST_TOOLCHAIN explicitly)"; }

usage() {
  cat <<'EOF'
Usage: m1nd-dev <command>

  help                         show this help
  state                        create and check checkout-private state
  doctor                       report selected toolchain and private paths
  model-verify                 verify the required embedding model files
  build [fast|debug|release]   build once with Cargo.lock, then verify identity
  version [fast|debug|release] verify the existing binary matches this tree
  cli <args...>                run the checked-out npm CLI in the fast profile
  stdio [fast|debug|release]   attach to a discovered owner or start private stdio
  serve [fast|debug|release]   serve only on 127.0.0.1:$M1ND_DEV_HTTP_PORT
  ui dev                       run guarded Vite on $M1ND_DEV_UI_PORT
  demo dev                     run the demo on $M1ND_DEV_DEMO_PORT
  gates lightning              run the canonical fast Rust selector
  shell                        open bash or zsh with the selected toolchain reapplied
EOF
}

private_dir() {
  local directory="$1"
  if [[ -L "$directory" ]]; then
    fail "private path cannot be a symlink: $directory"
  fi
  if [[ ! -e "$directory" ]]; then
    (umask 077; mkdir -p "$directory")
  fi
  "$M1ND_PYTHON" - "$directory" <<'PY'
import os, stat, sys
p = sys.argv[1]
s = os.lstat(p)
if stat.S_ISLNK(s.st_mode) or not stat.S_ISDIR(s.st_mode):
    raise SystemExit('private path must be a real directory')
if s.st_uid != os.getuid() or s.st_mode & 0o077:
    raise SystemExit('private directory is not uid-owned mode 0700')
PY
}

prepare_state() {
  private_dir "$M1ND_DEV_STATE"
  private_dir "$M1ND_DEVKIT_RUNTIME_DIR"
  private_dir "$M1ND_DEVKIT_REGISTRY_DIR"
  private_dir "$M1ND_DEVKIT_AGENT_CACHE_DIR"
  private_dir "$M1ND_DEVKIT_WORK_DIR"
  private_dir "$M1ND_DEVKIT_BINARY_STATE_DIR"
}

require_port() {
  local value="$1" name="$2"
  [[ "$value" =~ ^[0-9]+$ ]] && (( value >= 1 && value <= 65535 )) || fail "$name must be an integer from 1 through 65535"
  printf '%s\n' "$value"
}
http_port() { require_port "${M1ND_DEV_HTTP_PORT:-14438}" M1ND_DEV_HTTP_PORT; }
ui_port() { require_port "${M1ND_DEV_UI_PORT:-5173}" M1ND_DEV_UI_PORT; }
demo_port() { require_port "${M1ND_DEV_DEMO_PORT:-5175}" M1ND_DEV_DEMO_PORT; }
profile() { case "${1:-fast}" in fast|debug|release) printf '%s\n' "${1:-fast}";; *) fail 'profile must be fast, debug, or release';; esac; }
profile_target_dir() { case "$(profile "$1")" in fast) printf 'dev-fast\n';; debug|release) profile "$1";; esac; }
binary_for() { printf '%s/%s/m1nd-mcp\n' "$CARGO_TARGET_DIR" "$(profile_target_dir "${1:-fast}")"; }
tree_is_dirty() { [[ -n "$(git -C "$M1ND_CHECKOUT" status --porcelain --untracked-files=all)" ]]; }

tree_fingerprint() {
  "$M1ND_PYTHON" - "$M1ND_CHECKOUT" <<'PY'
import hashlib, os, subprocess, sys
root = sys.argv[1]
paths = subprocess.check_output(['git', '-C', root, 'ls-files', '-co', '--exclude-standard', '-z'])
h = hashlib.sha256()
for raw in sorted(filter(None, paths.split(b'\0'))):
    rel = raw.decode('utf-8', 'surrogateescape')
    path = os.path.join(root, rel)
    if not os.path.lexists(path):
        h.update(raw + b'\0<absent>\0')
        continue
    st = os.lstat(path)
    h.update(raw + b'\0' + oct(st.st_mode & 0o7777).encode() + b'\0')
    if os.path.islink(path):
        h.update(os.readlink(path).encode('utf-8', 'surrogateescape'))
    elif os.path.isfile(path):
        with open(path, 'rb') as f:
            for chunk in iter(lambda: f.read(1024 * 1024), b''):
                h.update(chunk)
    else:
        raise SystemExit(f'unsupported worktree entry: {rel}')
print(h.hexdigest())
PY
}

identity_file() { printf '%s/%s.identity\n' "$M1ND_DEVKIT_BINARY_STATE_DIR" "$(profile "$1")"; }
tree_snapshot() {
  local revision fingerprint
  revision="$(git -C "$M1ND_CHECKOUT" rev-parse HEAD)" || return 1
  fingerprint="$(tree_fingerprint)" || return 1
  printf '%s %s\n' "$revision" "$fingerprint"
}
assert_snapshot_unchanged() {
  local expected="$1" current
  current="$(tree_snapshot)" || fail 'cannot verify the checkout snapshot after build'
  [[ "$current" == "$expected" ]] || fail 'checkout changed during build; no current binary identity was recorded'
}
record_dirty_identity() {
  local selected="$1" snapshot="$2" was_dirty="$3" record temporary
  [[ "$was_dirty" == 1 ]] || return 0
  record="$(identity_file "$selected")"; temporary="$record.tmp.$$"
  umask 077
  printf '%s\n' "$snapshot" > "$temporary"
  chmod 600 "$temporary"; mv -f "$temporary" "$record"
}

has_native_stamp() {
  local selected="$1" binary expected actual
  selected="$(profile "$selected")"; binary="$(binary_for "$selected")"
  [[ -x "$binary" ]] || return 1
  expected="$(git -C "$M1ND_CHECKOUT" rev-parse HEAD)"
  tree_is_dirty && expected="${expected}-dirty"
  actual="$("$binary" --version 2>/dev/null)" || return 1
  [[ "$actual" == *"($expected)"* ]]
}
has_current_identity() {
  local selected="$1" record current
  has_native_stamp "$selected" || return 1
  if tree_is_dirty; then
    record="$(identity_file "$selected")"
    [[ -f "$record" && ! -L "$record" ]] || return 1
    current="$(tree_snapshot)" || return 1
    [[ "$(cat "$record")" == "$current" ]] || return 1
  fi
}
verify_binary() {
  local selected="$(profile "${1:-fast}")"
  has_current_identity "$selected" || fail "${selected} binary does not match this exact checkout; run: m1nd-dev build $selected"
  "$(binary_for "$selected")" --version
}

verify_model() {
  local expected file actual
  [[ -d "$M1ND_DEV_MODEL_DIR" ]] || fail "embedding model is missing: set M1ND_DEV_MODEL_DIR (or M1ND_EMBED_MODEL) to the CI-pinned revision"
  while read -r expected file; do
    [[ -f "$M1ND_DEV_MODEL_DIR/$file" ]] || fail "embedding model file is missing: $file"
    actual="$("$M1ND_PYTHON" - "$M1ND_DEV_MODEL_DIR/$file" <<'PY'
import hashlib, sys
h = hashlib.sha256()
with open(sys.argv[1], 'rb') as f:
    for chunk in iter(lambda: f.read(1024 * 1024), b''):
        h.update(chunk)
print(h.hexdigest())
PY
)"
    [[ "$actual" == "$expected" ]] || fail "embedding model hash mismatch: $file"
  done <<'EOF'
2a6ac0e9aaa356a68a5688070db78fc3a464fefe85d2f06a1905ce3718687553 config.json
f65d0f325faadc1e121c319e2faa41170d3fa07d8c89abd48ca5358d9a223de2 model.safetensors
e67e803f624fb4d67dea1c730d06e1067e1b14d830e2c2202569e3ef0f70bb50 tokenizer.json
EOF
}

build_stamp_inputs() {
  local revision dirty=0
  revision="$(git -C "$M1ND_CHECKOUT" rev-parse HEAD)" || fail 'cannot determine checkout revision for the build stamp'
  tree_is_dirty && dirty=1
  printf '%s %s\n' "$revision" "$dirty"
}
cargo_build_selected() {
  local selected="$1" inputs revision dirty
  inputs="$(build_stamp_inputs)" || fail 'cannot prepare build stamp invalidation inputs'
  revision="${inputs%% *}"; dirty="${inputs##* }"
  case "$selected" in
    fast) env M1ND_DEVKIT_BUILD_STAMP_REVISION="$revision" M1ND_DEVKIT_BUILD_STAMP_DIRTY="$dirty" cargo build --locked --profile dev-fast -p m1nd-mcp;;
    debug) env M1ND_DEVKIT_BUILD_STAMP_REVISION="$revision" M1ND_DEVKIT_BUILD_STAMP_DIRTY="$dirty" cargo build --locked -p m1nd-mcp;;
    release) env M1ND_DEVKIT_BUILD_STAMP_REVISION="$revision" M1ND_DEVKIT_BUILD_STAMP_DIRTY="$dirty" cargo build --locked --release -p m1nd-mcp;;
  esac
}
build_binary() {
  local selected="$(profile "${1:-fast}")" snapshot was_dirty=0
  check_rust
  prepare_state
  cd "$M1ND_CHECKOUT"
  snapshot="$(tree_snapshot)" || fail 'cannot snapshot the checkout before build'
  tree_is_dirty && was_dirty=1
  cargo_build_selected "$selected"
  assert_snapshot_unchanged "$snapshot"
  # A clean/rebuild is reserved for a real stale native stamp after Cargo has
  # had the revision/dirty invalidation inputs; it is never a HEAD/index repair.
  if ! has_native_stamp "$selected"; then
    cargo clean -p m1nd-mcp
    cargo_build_selected "$selected"
    assert_snapshot_unchanged "$snapshot"
  fi
  record_dirty_identity "$selected" "$snapshot" "$was_dirty"
  verify_binary "$selected" >/dev/null
}
require_binary() { local selected="$(profile "${1:-fast}")"; has_current_identity "$selected" || build_binary "$selected"; binary_for "$selected"; }

clean_native_env() {
  env -u M1ND_ATTACH_URL -u M1ND_HTTP_BEARER_TOKEN -u M1ND_HTTP_BEARER_TOKEN_FILE \
    -u M1ND_WORKSPACE_ROOT -u M1ND_RUNTIME_DIR -u M1ND_REGISTRY_DIR -u M1ND_RUNTIME_BASE \
    -u M1ND_GRAPH_SOURCE -u M1ND_PLASTICITY_STATE -u M1ND_MCP_BIN -u M1ND_MCP_ARGS "$@"
}
native_exec() {
  local binary="$1"; shift
  prepare_state; verify_model; cd "$M1ND_CHECKOUT"
  exec env -u M1ND_ATTACH_URL -u M1ND_HTTP_BEARER_TOKEN -u M1ND_HTTP_BEARER_TOKEN_FILE \
    -u M1ND_WORKSPACE_ROOT -u M1ND_RUNTIME_DIR -u M1ND_REGISTRY_DIR -u M1ND_RUNTIME_BASE \
    -u M1ND_GRAPH_SOURCE -u M1ND_PLASTICITY_STATE -u M1ND_MCP_BIN -u M1ND_MCP_ARGS \
    M1ND_WORKSPACE_ROOT="$M1ND_CHECKOUT" M1ND_RUNTIME_DIR="$M1ND_DEVKIT_RUNTIME_DIR" \
    M1ND_REGISTRY_DIR="$M1ND_DEVKIT_REGISTRY_DIR" M1ND_EMBED_MODEL="$M1ND_DEV_MODEL_DIR" \
    M1ND_NO_GUI=1 "$binary" "$@"
}

discover_stdio_owner() {
  local binary="$1" output status decision
  set +e
  output="$(cd "$M1ND_CHECKOUT" && clean_native_env M1ND_WORKSPACE_ROOT="$M1ND_CHECKOUT" M1ND_RUNTIME_DIR="$M1ND_DEVKIT_RUNTIME_DIR" M1ND_REGISTRY_DIR="$M1ND_DEVKIT_REGISTRY_DIR" "$binary" --discover-owner --runtime-dir "$M1ND_DEVKIT_RUNTIME_DIR" --registry-dir "$M1ND_DEVKIT_REGISTRY_DIR")"
  status=$?
  set -e
  if ! decision="$("$M1ND_PYTHON" -c '
import json, sys
try: status = int(sys.argv[1]); value = json.load(sys.stdin)
except Exception: raise SystemExit(2)
if not isinstance(value, dict) or value.get("schema") != "m1nd-owner-discovery-v0" or type(value.get("found")) is not bool: raise SystemExit(2)
if value["found"]:
    if status == 0 and value.get("discovery") in {"runtime_root", "ingest_coverage"} and isinstance(value.get("base_url"), str) and value["base_url"] and isinstance(value.get("owner_runtime_root"), str) and value["owner_runtime_root"]: print("attach")
    else: raise SystemExit(2)
elif status == 1 and value.get("discovery") is None and value.get("base_url") is None and value.get("owner_runtime_root") is None and isinstance(value.get("reason"), str) and value["reason"].startswith("no live serve owner for this client, on either discovery question:"): print("direct")
else: raise SystemExit(2)' "$status" <<<"$output")"; then fail 'owner discovery was malformed, ambiguous, or unavailable; refusing stdio'; fi
  printf '%s\n' "$decision"
}

validate_cli_binary_binding() {
  local expected="$1" argument
  shift
  while (($#)); do
    argument="$1"
    case "$argument" in
      --binary)
        (($# >= 2)) || fail 'm1nd-dev cli requires a value after --binary'
        [[ "$2" == "$expected" ]] || fail 'm1nd-dev cli binds --binary to this checkout only'
        shift 2
        ;;
      --binary=*) fail 'm1nd-dev cli requires its exact checkout binary path';;
      *) shift;;
    esac
  done
}
validate_cli_command() {
  case "$1" in
    agent|demo|doctor|help|host|hosts|init|install-skills|kickstart|mcp-config|pack-check|pack-routing-check|smoke|version) ;;
    restart|update|/restart) fail 'm1nd-dev cli refuses restart and update; run npm directly for those external operations';;
    *) fail 'm1nd-dev cli requires a supported command as its first argument';;
  esac
}
run_cli() {
  (($#)) || fail 'usage: m1nd-dev cli <m1nd arguments>'
  validate_cli_command "$1"
  local binary; binary="$(require_binary "${M1ND_DEV_PROFILE:-fast}")"
  validate_cli_binary_binding "$binary" "$@"
  check_node; check_python; prepare_state; verify_model
  exec env -u M1ND_ATTACH_URL -u M1ND_HTTP_BEARER_TOKEN -u M1ND_HTTP_BEARER_TOKEN_FILE \
    -u M1ND_WORKSPACE_ROOT -u M1ND_RUNTIME_DIR -u M1ND_REGISTRY_DIR -u M1ND_RUNTIME_BASE \
    -u M1ND_GRAPH_SOURCE -u M1ND_PLASTICITY_STATE -u M1ND_MCP_BIN -u M1ND_MCP_ARGS \
    M1ND_MCP_BINARY="$binary" M1ND_EMBED_MODEL="$M1ND_DEV_MODEL_DIR" \
    M1ND_RUNTIME_DIR="$M1ND_DEVKIT_RUNTIME_DIR" M1ND_REGISTRY_DIR="$M1ND_DEVKIT_REGISTRY_DIR" \
    M1ND_AGENT_CACHE_DIR="$M1ND_DEVKIT_AGENT_CACHE_DIR" "$M1ND_NODE" "$M1ND_CHECKOUT/npm/bin/m1nd.js" "$@" --binary "$binary"
}
run_stdio() {
  local binary="$1" mode; check_python; prepare_state; verify_model; mode="$(discover_stdio_owner "$binary")"
  case "$mode" in
    attach) native_exec "$binary" --stdio --no-gui --attach auto --runtime-dir "$M1ND_DEVKIT_RUNTIME_DIR" --registry-dir "$M1ND_DEVKIT_REGISTRY_DIR";;
    direct) native_exec "$binary" --stdio --no-gui --runtime-dir "$M1ND_DEVKIT_RUNTIME_DIR" --registry-dir "$M1ND_DEVKIT_REGISTRY_DIR";;
  esac
}
run_shell() {
  local boot user_zdotdir
  check_node; check_python; check_rust; prepare_state
  boot="$(mktemp -d "$M1ND_DEVKIT_WORK_DIR/shell.XXXXXX")"
  chmod 700 "$boot"
  if command -v zsh >/dev/null 2>&1; then
    user_zdotdir="${ZDOTDIR:-$HOME}"
    printf '%s\n' \
      'export ZDOTDIR="$M1ND_DEVKIT_USER_ZDOTDIR"' \
      '[[ -r "$ZDOTDIR/.zshenv" ]] && source "$ZDOTDIR/.zshenv"' \
      'export ZDOTDIR="$M1ND_DEVKIT_BOOT_DIR"' > "$boot/.zshenv"
    printf '%s\n' \
      'export ZDOTDIR="$M1ND_DEVKIT_USER_ZDOTDIR"' \
      '[[ -r "$ZDOTDIR/.zshrc" ]] && source "$ZDOTDIR/.zshrc"' \
      'source "$M1ND_DEVKIT_ACTIVATE"' \
      'cd "$M1ND_CHECKOUT"' > "$boot/.zshrc"
    M1ND_DEVKIT_BOOT_DIR="$boot" M1ND_DEVKIT_USER_ZDOTDIR="$user_zdotdir" M1ND_DEVKIT_ACTIVATE="$script_dir/activate.sh" ZDOTDIR="$boot" exec zsh -i
  fi
  printf '%s\n' \
    '[[ -r "$HOME/.bashrc" ]] && source "$HOME/.bashrc"' \
    'source "$M1ND_DEVKIT_ACTIVATE"' \
    'cd "$M1ND_CHECKOUT"' > "$boot/.bashrc"
  M1ND_DEVKIT_ACTIVATE="$script_dir/activate.sh" exec bash --noprofile --rcfile "$boot/.bashrc" -i
}
run_doctor() {
  prepare_state; need "$M1ND_NODE"; need "$M1ND_PYTHON"; need cargo; need rustc
  check_node; check_python; check_rust
  printf 'checkout: %s\ntarget: %s\nstate: %s\nruntime: %s\nnode: %s\npython: %s\nrust: %s\n' "$M1ND_CHECKOUT" "$CARGO_TARGET_DIR" "$M1ND_DEV_STATE" "$M1ND_DEVKIT_RUNTIME_DIR" "$("$M1ND_NODE" --version)" "$("$M1ND_PYTHON" --version)" "$(rustc --version)"
}

command_name="${1:-help}"
case "$command_name" in
  help|-h|--help) usage;;
  state) shift; (($# == 0)) || fail 'state accepts no arguments'; prepare_state; printf 'private state ready: %s\n' "$M1ND_DEV_STATE";;
  doctor) shift; (($# == 0)) || fail 'doctor accepts no arguments'; run_doctor;;
  shell) shift; (($# == 0)) || fail 'shell accepts no arguments'; run_shell;;
  model-verify) shift; (($# == 0)) || fail 'model-verify accepts no arguments'; verify_model; printf 'embedding model verified\n';;
  build) shift; selected="$(profile "${1:-${M1ND_DEV_PROFILE:-fast}}")"; (($# <= 1)) || fail 'build accepts fast, debug, or release'; build_binary "$selected";;
  version) shift; selected="$(profile "${1:-${M1ND_DEV_PROFILE:-fast}}")"; (($# <= 1)) || fail 'version accepts fast, debug, or release'; verify_binary "$selected";;
  cli) shift; run_cli "$@";;
  stdio) shift; selected="$(profile "${1:-${M1ND_DEV_PROFILE:-fast}}")"; (($# <= 1)) || fail 'stdio accepts fast, debug, or release'; run_stdio "$(require_binary "$selected")";;
  serve) shift; selected="$(profile "${1:-${M1ND_DEV_PROFILE:-fast}}")"; (($# <= 1)) || fail 'serve accepts fast, debug, or release'; selected_port="$(http_port)"; native_exec "$(require_binary "$selected")" --serve --bind 127.0.0.1 --port "$selected_port" --runtime-dir "$M1ND_DEVKIT_RUNTIME_DIR" --registry-dir "$M1ND_DEVKIT_REGISTRY_DIR";;
  ui) shift; [[ "${1:-}" == dev && $# == 1 ]] || fail 'usage: m1nd-dev ui dev'; check_node; prepare_state; exec "$M1ND_NODE" "$script_dir/ui-dev.mjs";;
  demo) shift; [[ "${1:-}" == dev && $# == 1 ]] || fail 'usage: m1nd-dev demo dev'; cd "$M1ND_CHECKOUT/m1nd-demo"; exec "$M1ND_NODE" "$M1ND_CHECKOUT/m1nd-demo/node_modules/vite/bin/vite.js" --host 127.0.0.1 --port "$(demo_port)" --strictPort;;
  gates) shift; [[ "${1:-}" == lightning && $# == 1 ]] || fail 'usage: m1nd-dev gates lightning'; check_rust; cd "$M1ND_CHECKOUT"; exec bash scripts/lightning_check.sh;;
  *) fail "unknown command: $command_name";;
esac
