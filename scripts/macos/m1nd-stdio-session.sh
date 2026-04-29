#!/usr/bin/env bash
set -euo pipefail

binary="${M1ND_MCP_BINARY:-m1nd-mcp}"
workspace="${M1ND_WORKSPACE:-${PWD}}"
runtime_base="${M1ND_RUNTIME_BASE:-${HOME}/.m1nd/runtimes}"
workspace_id="${M1ND_WORKSPACE_ID:-$(printf '%s' "${workspace}" | shasum -a 256 | awk '{print substr($1, 1, 12)}')}"
session_id="${M1ND_INSTANCE_ID:-ppid-${PPID:-0}-pid-$$}"
runtime_dir="${M1ND_RUNTIME_DIR:-${runtime_base}/${workspace_id}/sessions/${session_id}}"
retention_days="${M1ND_RUNTIME_RETENTION_DAYS:-1}"

mkdir -p "${runtime_dir}"

expired_dir="${runtime_base}/_expired/${workspace_id}-$(date +%Y%m%d-%H%M%S)"
sessions_dir="${runtime_base}/${workspace_id}/sessions"
if [ -d "${sessions_dir}" ]; then
  while IFS= read -r -d '' expired_path; do
    mkdir -p "${expired_dir}"
    mv "${expired_path}" "${expired_dir}/"
  done < <(find "${sessions_dir}" -mindepth 1 -maxdepth 1 -mtime +"${retention_days}" -print0 2>/dev/null)
fi

cd "${runtime_dir}"
exec "${binary}" \
  --stdio \
  --no-gui \
  --graph "${runtime_dir}/graph_snapshot.json" \
  --plasticity "${runtime_dir}/plasticity_state.json" \
  "$@"
