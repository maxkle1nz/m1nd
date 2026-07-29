# `build/` — signing surface

## `m1nd-mcp.entitlements.plist`

**A HARD PREREQUISITE for the G9 Secure Enclave custody ceremony, not a nicety.**

The custody floor persists the owner's authority key in the **data-protection
keychain** (`Location::DataProtectionKeychain`) — the only keychain a Secure
Enclave key can be made permanent in — and scopes both the provisioning write and
`resolve_persisted_key` to it via `kSecUseDataProtectionKeychain`. See the
`HARD PREREQUISITE` block above the real Security.framework key store in
`m1nd-mcp/src/enclave_authority.rs`.

An unsigned or **unentitled** binary cannot persist or resolve that key at all:
`open`/`sign` fail closed. Before this file existed, `release.yml` signed with
`--options runtime` and **no `--entitlements`**, so the shipped, notarized binary
was structurally incapable of running the owner's ceremony — a gap invisible until
the ceremony would have failed at its first step. Measured 2026-07-29 while staging
G9; the release step now passes the entitlement **and proves it landed on the
shipped bytes** (a signature that silently dropped it would leave the ceremony
broken and the release green).

`4KLJ4N9D5K` is the Developer ID team the release already signs with
(`Developer ID Application: … (4KLJ4N9D5K)`).

### This file is a custody surface

It is deliberately **minimal** — one entitlement, one group. Every entry widens
what a signed binary may do, so adding to it is a **security decision**, never a
build tweak.

### Why the plist carries no comments

`codesign` parses entitlements with `AMFIUnserializeXML`, which **rejects XML
comments anywhere in the file** — including inside `<dict>`. `plutil -lint`
accepts them, so a commented plist lints clean and then fails at signing time with
`Failed to parse entitlements: AMFIUnserializeXML: syntax error`. Worse, that
failure does **not** set a non-zero exit on `codesign`: the binary signs *without*
the entitlement and the pipeline looks green. Both facts were proven by signing a
probe binary rather than assumed — which is why the rationale lives in this README
and the release step verifies the entitlement is present on the output.
