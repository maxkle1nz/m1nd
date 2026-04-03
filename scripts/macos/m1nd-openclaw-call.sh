#!/bin/bash
set -euo pipefail

ROOT="/Users/cosmophonix/SISTEMA/m1nd"
SOCK="${M1ND_OPENCLAW_SOCKET:-/tmp/m1nd-openclaw.sock}"
BIN_RELEASE="${ROOT}/target/release/m1nd-openclaw-client"
BIN_DEBUG="${ROOT}/target/debug/m1nd-openclaw-client"

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
  echo "m1nd-openclaw-client binary not found in release or debug" >&2
  exit 1
fi

if [ "$#" -lt 2 ]; then
  echo "usage: m1nd-openclaw-call.sh <tool> '<json-args>'" >&2
  exit 1
fi

TOOL="$1"
ARGS="$2"

exec "${BIN}" --socket "${SOCK}" "${TOOL}" "${ARGS}"
