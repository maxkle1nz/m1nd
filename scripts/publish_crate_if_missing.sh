#!/usr/bin/env bash
set -euo pipefail

crate="${1:?crate name required}"
version="${2:?crate version required}"
shift 2

crate_search_version_exists() {
  if ! command -v cargo >/dev/null 2>&1; then
    return 1
  fi

  if command -v timeout >/dev/null 2>&1; then
    if CARGO_HTTP_TIMEOUT=15 CARGO_NET_RETRY=1 timeout 30s \
      cargo search "$crate" --limit 1 | grep -F "$crate = \"$version\"" >/dev/null; then
      return 0
    fi
  elif CARGO_HTTP_TIMEOUT=15 CARGO_NET_RETRY=1 \
    cargo search "$crate" --limit 1 | grep -F "$crate = \"$version\"" >/dev/null; then
    return 0
  fi

  return 1
}

crate_version_exists() {
  if crate_search_version_exists; then
    return 0
  fi

  if command -v curl >/dev/null 2>&1; then
    curl \
      --connect-timeout 5 \
      --max-time 15 \
      --retry 2 \
      --retry-delay 2 \
      -fsSL "https://crates.io/api/v1/crates/${crate}/${version}" >/dev/null 2>&1
    return $?
  fi

  return 1
}

if crate_version_exists; then
  echo "$crate $version is already published; skipping."
  exit 0
fi

publish_log="$(mktemp)"
set +e
cargo publish -p "$crate" "$@" 2>&1 | tee "$publish_log"
publish_status=${PIPESTATUS[0]}
set -e

if [ "$publish_status" -eq 0 ]; then
  rm -f "$publish_log"
  exit 0
fi

if grep -F "crate ${crate}@${version} already exists on crates.io index" "$publish_log" >/dev/null; then
  echo "$crate $version is already published according to cargo publish; treating as success."
  rm -f "$publish_log"
  exit 0
fi

rm -f "$publish_log"
exit "$publish_status"
