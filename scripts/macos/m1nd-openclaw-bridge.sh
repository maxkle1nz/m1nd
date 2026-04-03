#!/bin/bash
set -euo pipefail

ROOT="/Users/cosmophonix/SISTEMA/m1nd"
SOCK="${M1ND_OPENCLAW_SOCKET:-/tmp/m1nd-openclaw.sock}"
RUNTIME_DIR="${M1ND_RUNTIME_DIR:-/Users/cosmophonix/SISTEMA/m1nd-data}"
GRAPH="${M1ND_GRAPH_SOURCE:-${RUNTIME_DIR}/graph.json}"
PLASTICITY="${M1ND_PLASTICITY_STATE:-${RUNTIME_DIR}/plasticity.json}"

BIN_RELEASE="${ROOT}/target/release/m1nd-openclaw"
BIN_DEBUG="${ROOT}/target/debug/m1nd-openclaw"

if [ -x "${BIN_RELEASE}" ] && [ -x "${BIN_DEBUG}" ]; then
  if [ "${BIN_DEBUG}" -nt "${BIN_RELEASE}" ]; then
    BIN="${BIN_DEBUG}"
  else
    BIN="${BIN_RELEASE}"
  fi
elif [ -x "${BIN_RELEASE}" ]; then
  BIN="${BIN_RELEASE}"
elif [ -x "${BIN_DEBUG}" ]; then
  BIN="${BIN_DEBUG}"
else
  echo "m1nd-openclaw binary not found in release or debug" >&2
  exit 1
fi

export M1ND_RUNTIME_DIR="${RUNTIME_DIR}"
export M1ND_GRAPH_SOURCE="${GRAPH}"
export M1ND_PLASTICITY_STATE="${PLASTICITY}"

exec "${BIN}" --socket "${SOCK}"
