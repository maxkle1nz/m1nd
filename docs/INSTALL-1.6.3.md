# Install the signed 1.6.3 native runtime

This recipe pins the **native binary**, independently of npm's `latest` tag. The npm wrapper's `update apply` follows the registry channel even when the wrapper itself is pinned, so it is not a version-pinned installation command.

Requirements: a POSIX shell, `curl`, and [cosign](https://docs.sigstore.dev/cosign/system_config/installation/) on `PATH`. The release has native assets for Linux x86_64 and macOS x86_64/ARM64. No human-only birth or repository mutation is performed by this installation step.

## Download and verify before installing

Run the whole block. It selects the exact platform asset from [release v1.6.3](https://github.com/maxkle1nz/m1nd/releases/tag/v1.6.3), verifies its portable signature bundle against the exact repository workflow and tag identity, and only then installs it in a version-specific directory. A failure stops the subshell; there is no unsigned fallback. The temporary download directory is removed on exit.

```sh
(
  set -eu
  case "$(uname -s):$(uname -m)" in
    Linux:x86_64) asset=m1nd-mcp-linux-x86_64 ;;
    Darwin:arm64|Darwin:aarch64) asset=m1nd-mcp-macos-aarch64 ;;
    Darwin:x86_64) asset=m1nd-mcp-macos-x86_64 ;;
    *) echo "No v1.6.3 prebuilt asset for this platform" >&2; exit 1 ;;
  esac
  base=https://github.com/maxkle1nz/m1nd/releases/download/v1.6.3
  staging=$(mktemp -d)
  trap 'rm -rf "$staging"' EXIT
  curl -fsSL --proto '=https' --proto-redir '=https' \
    "$base/$asset" -o "$staging/$asset"
  curl -fsSL --proto '=https' --proto-redir '=https' \
    "$base/$asset.sigstore.json" -o "$staging/$asset.sigstore.json"
  cosign verify-blob \
    --bundle "$staging/$asset.sigstore.json" \
    --certificate-identity https://github.com/maxkle1nz/m1nd/.github/workflows/release.yml@refs/tags/v1.6.3 \
    --certificate-oidc-issuer https://token.actions.githubusercontent.com \
    "$staging/$asset"
  destination="$HOME/.local/share/m1nd/versions/1.6.3"
  mkdir -p "$destination"
  install -m 755 "$staging/$asset" "$destination/m1nd-mcp"
  "$destination/m1nd-mcp" --version
)
```

The output must identify `m1nd-mcp 1.6.3`. This verifies the release asset's signature; it does not reproduce the updater's whole-candidate compatibility checks or automatic rollback. It leaves any differently located runtime unchanged. Running it again replaces only the binary at this version-specific destination with verified bytes.

## Use that exact binary in setup

In the shell where you will continue setup:

```sh
export M1ND_BINARY="$HOME/.local/share/m1nd/versions/1.6.3/m1nd-mcp"
"$M1ND_BINARY" --version
```

Pass `--binary "$M1ND_BINARY"` to the wrapper's birth and host-plan/apply commands. Do not rely on whichever `m1nd-mcp` happens to be on `PATH`; `doctor` is a general visibility diagnostic and does not pin these commands to a version.

Continue with [the published quick start](wiki/src/tutorials/quickstart.md) or [README setup](../README.md#get-started). A person performs birth separately, with an explicit runtime directory; its receipt and the MCP host must name the same runtime. Keep the newer [source-preview path](AGENT-AUTONOMY.md) separate.
