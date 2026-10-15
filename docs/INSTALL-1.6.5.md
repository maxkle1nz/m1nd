# Install the signed 1.6.5 native runtime

This recipe pins the **native binary** from [release v1.6.5](https://github.com/maxkle1nz/m1nd/releases/tag/v1.6.5), at immutable commit `363bc97f2336c73e90802916804dccd2109c5746`. GitHub/native, crates.io, and npm 1.6.5 are published; npm `latest` is 1.6.5 as of 2026-10-12. The npm wrapper's `update apply` follows the registry channel even when the wrapper itself is pinned, so it is not a version-pinned native installation command.

Requirements: a POSIX shell, `curl`, and [cosign](https://docs.sigstore.dev/cosign/system_config/installation/) on `PATH`. This release has native assets for Linux x86_64 and macOS x86_64/ARM64. Check release assets for other platforms. Installation does not prepare a repository or perform a human-only birth ceremony.

## Download and verify before installing

Run the whole block. It verifies the exact platform asset's portable signature bundle against the repository workflow and tag identity before installation. A failure stops the subshell; there is no unsigned fallback. The temporary download directory is removed on exit.

```sh
(
  set -eu
  case "$(uname -s):$(uname -m)" in
    Linux:x86_64) asset=m1nd-mcp-linux-x86_64 ;;
    Darwin:arm64|Darwin:aarch64) asset=m1nd-mcp-macos-aarch64 ;;
    Darwin:x86_64) asset=m1nd-mcp-macos-x86_64 ;;
    *) echo "No v1.6.5 prebuilt asset for this platform" >&2; exit 1 ;;
  esac
  base=https://github.com/maxkle1nz/m1nd/releases/download/v1.6.5
  staging=$(mktemp -d)
  trap 'rm -rf "$staging"' EXIT
  curl -fsSL --proto '=https' --proto-redir '=https' \
    "$base/$asset" -o "$staging/$asset"
  curl -fsSL --proto '=https' --proto-redir '=https' \
    "$base/$asset.sigstore.json" -o "$staging/$asset.sigstore.json"
  cosign verify-blob \
    --bundle "$staging/$asset.sigstore.json" \
    --certificate-identity https://github.com/maxkle1nz/m1nd/.github/workflows/release.yml@refs/tags/v1.6.5 \
    --certificate-oidc-issuer https://token.actions.githubusercontent.com \
    "$staging/$asset"
  chmod u+x "$staging/$asset"
  reported_version=$("$staging/$asset" --version)
  case "$reported_version" in
    'm1nd-mcp 1.6.5 (363bc97f2336c73e90802916804dccd2109c5746)'|\
    'm1nd-mcp 1.6.5 (363bc97f2336c73e90802916804dccd2109c5746-dirty)') ;;
    *) echo "Unexpected 1.6.5 native identity: $reported_version" >&2; exit 1 ;;
  esac
  destination="$HOME/.local/share/m1nd/versions/1.6.5"
  mkdir -p "$destination"
  install -m 755 "$staging/$asset" "$destination/m1nd-mcp"
  "$destination/m1nd-mcp" --version
)
```

The signed macOS ARM64 release asset reports the commit with a `-dirty` build marker. The check accepts only the exact release revision with or without that marker; the version text does not certify a clean build tree. The tag-bound signature verifies the published asset. This does not reproduce the updater's whole-candidate compatibility checks or automatic rollback. Running it again replaces only this version-specific binary with verified bytes.

## Use that exact binary

```sh
export M1ND_BINARY="$HOME/.local/share/m1nd/versions/1.6.5/m1nd-mcp"
"$M1ND_BINARY" --version
```

Pass `--binary "$M1ND_BINARY"` to the pinned `@maxkle1nz/m1nd@1.6.5` wrapper. `doctor` is a general visibility diagnostic, not a version selector. Continue with [the quick start](wiki/src/tutorials/quickstart.md) or [README setup](../README.md#get-started).

In 1.6.5, `agent first-minute --repo <repo>` can prepare the exact launcher-granted repository in an external private cache when no covering owner is found. Source files remain inputs, and later calls reuse the graph. Direct host stdio needs the same explicit grant and a safe external runtime; shared-brain preparation retains its authority requirements. See [agent autonomy](AGENT-AUTONOMY.md).

The learning-event separation merged in PR580 is **Unreleased**, absent from this exact binary. V1 calibration containment already shipped in 1.6.5: running diagnostic calibration cannot enable `act`. The [1.6.3 guide](INSTALL-1.6.3.md) remains a separate historical recipe.
