#!/usr/bin/env bash
set -euo pipefail

# Compatibility entrypoint retained for release automation.  Despite the
# historical filename, an already-existing version is now a hard immutable
# conflict: the helper never skips and never repackages a checkout.
crate_file="${1:?candidate .crate path required}"
crate_name="${2:?crate name required}"
crate_version="${3:?crate version required}"
crate_sha256="${4:?candidate SHA-256 required}"

script_dir="$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd -P)"

exec python3 "${script_dir}/m1nd10_crates_io_upload.py" upload \
  --crate "${crate_file}" \
  --expected-name "${crate_name}" \
  --expected-version "${crate_version}" \
  --expected-sha256 "${crate_sha256}"
