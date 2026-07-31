# G9 — the platform wall, and the three ways through it

> Written 2026-07-31, after the v1.6.0 release measured the wall with a kernel
> refusal. This document does **not** decide; it lays out what each road costs and
> what it does to the custody guarantee, so the owner can choose. Nothing here is
> implemented.

## The wall, in three sentences

The custody floor persists the owner's authority key in the **data-protection
keychain** — by Apple's design the *only* keychain a Secure Enclave key can be made
permanent in (`m1nd-mcp/src/enclave_authority.rs`, the HARD PREREQUISITE block).
Writing and resolving there requires the `keychain-access-groups` entitlement, which
AMFI honours **only** when an embedded provisioning profile authorizes it (Apple,
TN3137 § Implementation differences). A command-line binary has nowhere to embed a
profile, so a signed CLI claiming that entitlement is SIGKILLed at launch — measured
live on the v1.6.0 artifact: *"Code has restricted entitlements, but the validation
of its code signature failed"*, amfid `-413` *"No matching profile found"*.

This is a property of the platform. No change to m1nd's code removes it.

## Why the key's permanence is the crux

The owner authority is not a session token: every receipt the ladder produces is
signed by it, and the whole point of G9→G10 is that later receipts chain to earlier
ones under **one** authority. A key that cannot be made permanent means a new
authority per boot, and a chain that cannot reach back. So "just drop permanence" is
not a small dial — it is a change to what the program proves.

## Road A — an app-like bundle carrying an embedded profile

Apple's own documented answer for exactly this case (*Signing a daemon with a
restricted entitlement*): wrap the executable in an app-like structure and put the
profile at `<name>.app/Contents/embedded.provisionprofile`.

- **Custody guarantee:** unchanged — Secure Enclave, permanent key, data-protection
  keychain. This is the design as ratified.
- **What the owner must obtain:** a macOS provisioning profile. **Measured
  2026-07-31, and smaller than it looked:** `keychain-access-groups` is not a
  capability anyone toggles — a profile already on this machine grants
  `["<TEAM>.*", "com.apple.token"]`, a team-wide wildcard that already covers the
  group the custody entitlement names, and the App ID carrying it declares its
  platform as *iOS, iPadOS, macOS, tvOS, watchOS, visionOS*. So the road needs a
  profile generated and downloaded, not a capability requested or a membership
  changed.
- **What the machine must build:** the release packages the ceremony binary as a
  bundle, embeds the profile, signs the bundle with the entitlement, and the artifact
  smoke learns to launch a bundled executable. The profile **expires** (typically a
  year), so the release gains a renewal dependency and an expiry check that must fail
  loudly rather than silently ship an unlaunchable artifact.
- **Cost of being wrong:** low. If the bundle is malformed the kernel refuses at
  launch, exactly as it did in v1.6.0, and the smoke gate catches it before publish.
- **What is still unmeasured:** that a Developer-ID-signed *bundle* carrying such a
  profile actually satisfies AMFI for this entitlement — the negative was proven
  (a bundle without a profile is killed exactly like a raw binary), the positive
  needs a real profile to test.

## Road B — the file-based keychain

Apple's guidance for programs running outside a user context (a launchd daemon, for
instance) is the file-based keychain, which needs no restricted entitlement.

- **Custody guarantee:** **changed, and this is the decision.** A Secure Enclave key
  cannot be made permanent there. The owner authority would either stop being
  enclave-backed (a software key in a file-based keychain — protected by the keychain,
  not by the Secure Enclave) or stay enclave-backed but non-permanent, which Road C
  describes.
- **What it buys:** the ceremony runs on the plain signed CLI the release already
  publishes, today, with no portal work and no bundle.
- **What it costs:** the G9 amendment ratified Path B *because* the enclave was the
  floor. Choosing this road means re-opening that ratification honestly — not
  quietly widening it — and re-stating what the custody floor now claims.
- **Cost of being wrong:** high and silent. A software key that reads like an enclave
  key in the receipts is exactly the class of lie this program exists to refuse; any
  move here must make the weaker guarantee *visible in the receipt*.

## Road C — keep the enclave, drop permanence

Mint the enclave key fresh, hold it for the ceremony, never persist it.

- **Custody guarantee:** the key is enclave-backed and biometry-gated while it lives,
  but the authority cannot outlive the process, so the receipt chain restarts every
  time. G10's autonomy epoch, which is defined on a continuing authority, would need
  redefinition.
- **What it buys:** no portal work, no bundle, no weakening of what the key *is*.
- **What it costs:** the most conceptual work of the three — it changes what "the
  owner's authority" means across time, which is PRD material, not a config flag.
- **Cost of being wrong:** medium. Nothing breaks loudly; the program simply proves
  less than it says. Would need the manifest and the ceremony receipt to state the
  epoch boundary explicitly.

## Decision — Road A, ratified by the owner 2026-07-31

Taken after the measurement above turned the road's only unknown into a downloaded
file. It keeps the guarantee the program already ratified, it re-argues no security
floor, and its failure mode is the loud kernel refusal an existing gate already
catches. Roads B and C stay recorded because they remain the honest fallbacks if the
bundle path fails AMFI for a reason not yet visible — each would change what the
ladder proves, and would need its own ratification.

**What this makes buildable now:** the release learns to package the ceremony as a
bundle with an embedded profile, sign it with the entitlement, and prove it launches.
**What stays the owner's:** generating the macOS provisioning profile once, and the
ceremony itself.

## What the machine cannot do here

`preflight` reports P4 as owner-side; `provision-seats` and the rest refuse with the
keychain's own error on an unentitled binary. That refusal is correct and must not be
"fixed" — it is the floor telling the truth about the platform it is standing on.
