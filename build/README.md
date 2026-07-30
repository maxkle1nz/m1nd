# `build/` — signing surface

## `m1nd-mcp.entitlements.plist`

**The release does NOT sign with this file, and must not. It is kept for the
owner-side, bundled signing the G9 custody ceremony needs (prerequisite P4).**

An earlier version of this document asserted the opposite — that threading
`--entitlements` through `release.yml` was what made the shipped binary capable of
the ceremony. That was wrong, and the correction below is measured, not reasoned.

### What was measured, 2026-07-30

Release run `30556058443` (tag `v1.6.0`) signed both macOS binaries with
`--options runtime --entitlements build/m1nd-mcp.entitlements.plist`. Everything a
signature check can ask, passed: `codesign --verify --strict` reported *valid on
disk* and *satisfies its Designated Requirement*, notarization returned
`status: Accepted`, `spctl -a` answered *accepted, source=Notarized Developer ID*,
and `codesign -d --entitlements -` showed the entitlement on the shipped bytes.

The binaries could not run. The installed-artifact smoke failed on both macOS legs
with

```
release artifact smoke refused: Command '[…/runtime/m1nd-mcp', '--version']' died with <Signals.SIGKILL: 9>.
```

Running that exact downloaded artifact on an Apple Silicon Mac (macOS 15.6)
reproduces it — exit 137 — and the kernel says why:

```
taskgated-helper  Disallowing m1nd-mcp because no eligible provisioning profiles found
amfid             m1nd-mcp not valid: Error Domain=AppleMobileFileIntegrityError Code=-413
                  "No matching profile found"
kernel (AppleMobileFileIntegrity)  AMFI: When validating m1nd-mcp:
                  Code has restricted entitlements, but the validation of its code
                  signature failed.
                  Unsatisfied Entitlements:
kernel            proc …: load code signature error 4 for file "m1nd-mcp"
```

The entitlement is the whole cause, isolated by an A/B on the same bytes:
re-signed ad-hoc **without** entitlements the binary launches and prints its
version; re-signed ad-hoc **with** this plist it is SIGKILLed again. Wrapping it in
a minimal `.app` bundle does not help either — without an
`embedded.provisionprofile` inside that bundle the kill is identical.

### What Apple actually requires

`keychain-access-groups` is a *restricted* entitlement: AMFI honours it only when a
provisioning profile authorizes it. TN3137, *On Mac keychain APIs and
implementations*, § Implementation differences, states the rule and its consequence
for tools like this one:

> macOS builds the list of data protection keychain access groups available to your
> program from its code signing entitlements. […] These entitlements must be
> authorized by a provisioning profile. Your program needs an app-like bundle
> structure in which to embed that profile. This is standard for app and app
> extensions but not for command-line tools.

A raw Mach-O executable has nowhere to put a profile — it lives at
`Contents/embedded.provisionprofile` inside a bundle — so a shipped, non-bundled
`m1nd-mcp` cannot carry this entitlement and remain runnable. Apple's own workaround
for exactly this case is *Signing a daemon with a restricted entitlement*: wrap the
standalone executable in an app-like structure whose profile authorizes the
entitlement. That is a packaging and Apple-Developer-portal decision belonging to
the owner, not a byte the release pipeline can add.

The same technote also fixes the other half of P4: the data-protection keychain is
"only available to programs running in a user context", so the ceremony is run by
hand from a login session, never from a `launchd` daemon.

### What the release does instead

`release.yml` signs with `--options runtime --timestamp` and **no entitlements**,
then refuses, mechanically:

1. any provisioning-profile-restricted entitlement on the signed output
   (`keychain-access-groups`, `application-identifier`, `com.apple.developer.*`) —
   the check now points the other way, because the direction that kills the product
   is *claiming* one; and
2. a binary that does not launch — `"$BIN_PATH" --version` runs on the native
   runner right after signing.

The installed-artifact smoke is unchanged and stays exactly as strict. It is what
caught this, and the two checks above only move the same failure earlier.

### What this leaves open for G9

Prerequisite P4 is now **owner-side**. The shipped binary fails closed at the
keychain, by design and with its own name
(`custody_ceremony_keychain_entitlement_missing`) — that is the honest answer, not
a regression. The ceremony still needs a binary that *is* authorized, which means
the owner signs locally against a profile, or the ceremony surface ships as a
bundle. Recorded, undecided, in `docs/benchmarks/G9-CUSTODY-CEREMONY.md` §1 P4 and
§5 R1; this PR does not choose for the owner.

### This file is a custody surface

It is deliberately **minimal** — one entitlement, one group, unchanged by this
correction. Every entry widens what a signed binary may do, so adding to it is a
**security decision**, never a build tweak. `4KLJ4N9D5K` is the Developer ID team
the release signs with (`Developer ID Application: … (4KLJ4N9D5K)`).

### Why the plist carries no comments

`codesign` parses entitlements with `AMFIUnserializeXML`, which **rejects XML
comments anywhere in the file** — including inside `<dict>`. `plutil -lint` accepts
them, so a commented plist lints clean and then fails at signing time with
`Failed to parse entitlements: AMFIUnserializeXML: syntax error`. Worse, that
failure does **not** set a non-zero exit on `codesign`: the binary signs *without*
the entitlement and the pipeline looks green. Both facts were proven by signing a
probe binary rather than assumed (2026-07-29), and they still hold for whoever
signs this plist onto a bundled build — the trap is in `codesign`, not in the
release.
