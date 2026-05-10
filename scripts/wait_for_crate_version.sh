#!/usr/bin/env bash
set -euo pipefail

crate="${1:?crate name required}"
version="${2:?crate version required}"

crate_version_exists() {
  if command -v curl >/dev/null 2>&1; then
    curl -fsSL "https://crates.io/api/v1/crates/${crate}/${version}" >/dev/null 2>&1
    return $?
  fi

  cargo search "$crate" --limit 1 | grep -F "$crate = \"$version\"" >/dev/null
}

for attempt in $(seq 1 40); do
  if crate_version_exists; then
    echo "$crate $version is visible on crates.io."
    exit 0
  fi

  echo "Waiting for $crate $version on crates.io index ($attempt/40)..."
  sleep 15
done

echo "Timed out waiting for $crate $version on crates.io index." >&2
exit 1
