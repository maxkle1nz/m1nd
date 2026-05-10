#!/usr/bin/env bash
set -euo pipefail

crate="${1:?crate name required}"
version="${2:?crate version required}"
shift 2

crate_version_exists() {
  if command -v curl >/dev/null 2>&1; then
    curl \
      --connect-timeout 5 \
      --max-time 15 \
      --retry 2 \
      --retry-delay 2 \
      -fsSL "https://crates.io/api/v1/crates/${crate}/${version}" >/dev/null 2>&1
    return $?
  fi

  cargo search "$crate" --limit 1 | grep -F "$crate = \"$version\"" >/dev/null
}

if crate_version_exists; then
  echo "$crate $version is already published; skipping."
  exit 0
fi

cargo publish -p "$crate" "$@"
