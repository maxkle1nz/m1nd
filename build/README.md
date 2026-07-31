# `build/` — signing surface

## `m1nd-mcp.entitlements.plist`

**The release signs exactly one artifact with this file — the custody-ceremony
`.app` bundle — and must never sign the ordinary `m1nd-mcp` runtime with it.**
The two artifacts carry opposite contracts, and the reason is measured, not
reasoned; the correction below is the measurement.

An earlier version of this document asserted that threading `--entitlements`
through `release.yml` was what made the shipped binary capable of the ceremony.
That was wrong.

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
entitlement. The packaging half of that is the release's, and is below; the
profile half stays the owner's — nothing in this pipeline can mint one.

The same technote also fixes the other half of P4: the data-protection keychain is
"only available to programs running in a user context", so the ceremony is run by
hand from a login session, never from a `launchd` daemon.

### Artifact 1 — the ordinary `m1nd-mcp` runtime, unentitled forever

`release.yml` signs it with `--options runtime --timestamp` and **no
entitlements**, then refuses, mechanically:

1. any provisioning-profile-restricted entitlement on the signed output
   (`keychain-access-groups`, `application-identifier`, `com.apple.developer.*`) —
   the check now points the other way, because the direction that kills the product
   is *claiming* one; and
2. a binary that does not launch — `"$BIN_PATH" --version` runs on the native
   runner right after signing.

The installed-artifact smoke is unchanged and stays exactly as strict. It is what
caught this, and the two checks above only move the same failure earlier. This is
the artifact every user downloads, and nothing about the ceremony may ever weaken
these two refusals.

### Artifact 2 — `m1nd-custody-ceremony.app`, the only entitled artifact

Apple's own workaround for this case — *Signing a daemon with a restricted
entitlement* — is to wrap the executable in an app-like structure whose embedded
profile authorizes the entitlement. The owner ratified that road
(`docs/benchmarks/G9-PLATFORM-DECISION.md`, Road A), so `release.yml` builds a
**second** macOS artifact per target, from the **same bytes** the build step
already produced:

```
m1nd-custody-ceremony.app/Contents/Info.plist
m1nd-custody-ceremony.app/Contents/MacOS/m1nd-mcp
m1nd-custody-ceremony.app/Contents/embedded.provisionprofile
m1nd-custody-ceremony.app/Contents/_CodeSignature/CodeResources
```

signed **with** this plist, notarized, stapled, and published as
`m1nd-custody-ceremony-macos-<arch>.zip`. It is a CI artifact of the release run
(retained 90 days, longer than the runtime artifacts), deliberately **outside** the
signed candidate/release file set — the same posture the verified-updater receipts
already hold — because it exists only when the owner supplies a profile, and the
candidate's bytes must not depend on a secret.

Four properties are load-bearing:

- **The profile is an owner input, never a repo file.** It lives in the repository
  secret `APPLE_CUSTODY_PROFILE_BASE64`, is decoded to `$RUNNER_TEMP`, read, copied
  into the bundle, and deleted, exactly as `APPLE_CERT_P12_BASE64` is handled. With
  the secret absent the step **skips loudly** (`::warning`) and publishes nothing —
  a bundle without a profile is SIGKILLed exactly like the raw binary was, so an
  unentitled artifact wearing the ceremony's name would be worse than no artifact.
- **The bundle identifier is derived, not invented.** It is the access group's own
  suffix, because a program's default data-protection keychain access group *is*
  its application-identifier: identified this way, the bundle asks for exactly the
  group this plist grants. The consequence is a coupling the release enforces —
  the profile must be issued for the App ID `<team>.<that suffix>` (a team wildcard
  covers it), and if the owner ever regenerates the profile for a different App ID
  the bundle identifier must move with it. The release **refuses** the mismatch
  instead of publishing a bundle the kernel will kill.
- **A signature that verifies is not a binary that runs.** After signing, the step
  launches `Contents/MacOS/m1nd-mcp --version` on the native runner, checks the
  version it prints is this release's, and repeats both after packaging and
  unpacking. That is the check v1.6.0 did not have.
- **The profile expires, and the bundle dies with it.** The step reads
  `ExpirationDate` out of the profile (`security cms -D` → plist) and **fails the
  release** when it is under **30 days** away, naming the date it read. Thirty days
  is not a claim about the artifact's shelf life — that is the expiry itself, which
  the run prints as a `::notice`; it is the window in which the owner can notice,
  regenerate and re-tag without the ceremony surface ever being unavailable. Two
  more profile shapes are refused for the same reason (a silent death later beats
  no artifact now): a device-scoped development profile, which authorizes only
  enrolled Macs and not the runner that must launch-prove the bundle, and a profile
  whose platform list is not macOS.

### What is still unproven

That a Developer-ID-signed **bundle** with a real profile satisfies AMFI for this
entitlement. The negative is measured — a bundle *without* a profile is killed
exactly like the raw binary — and the positive needs a profile no agent can
create. It is proven or disproven loudly by the launch check above, on the first
tagged release with the secret set, before anything is published.

### This file is a custody surface

It is deliberately **minimal** — one entitlement, one group, and it is the same
file it was before the bundle existed: the second artifact added **zero** bytes to
it. Every entry widens what a signed binary may do, so adding to it is a
**security decision**, never a build tweak — and it now also decides the bundle's
identity, so an edit here moves the App ID the owner's profile must cover.
`4KLJ4N9D5K` is the Developer ID team the release signs with
(`Developer ID Application: … (4KLJ4N9D5K)`).

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
