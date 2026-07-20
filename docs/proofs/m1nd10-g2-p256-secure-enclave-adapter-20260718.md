# M1nd 10 G2 checkpoint — P-256 and macOS Secure Enclave adapter

Date: 2026-07-18
Gate status: **checkpoint only; G2 remains open**

## Outcome

The authority protocol no longer pretends an Ed25519 private key can live in
Apple Secure Enclave. It now supports two explicit signature algorithms:

- `ED25519`, retained for compatibility;
- `ECDSA_P256_SHA256_X962`, with a 65-byte uncompressed SEC1 public key and a
  strict, low-S ASN.1 DER signature.

h4nd now contains a dormant macOS adapter that opens or explicitly provisions
a P-256 Secure Enclave key, re-attests the token/type/size attributes, obtains
only the public representation and signs through `SecKeyCreateSignature`.
There is no startup-time or implicit `open-or-create` path.

## Mechanical proof

M1nd control-plane commands:

```text
RUSTFLAGS='-D warnings' cargo test --locked -p m1nd-control --test crypto_p256
3 passed; 0 failed

RUSTFLAGS='-D warnings' cargo test --locked -p m1nd-control --test crypto_authority
12 passed; 0 failed

cargo clippy --locked -p m1nd-control --all-targets -- -D warnings
PASS
```

The P-256 tests cover owner challenge, human approval, autonomous capability,
replay, body tampering, malformed DER, compressed public-key refusal and raw
signature refusal. The pre-existing Ed25519 battery remains green.

h4nd native commands:

```text
RUSTFLAGS='-D warnings' cargo test --locked --all-targets
16 passed; 0 failed; 1 ignored live-owner probe

cargo clippy --locked --all-targets -- -D warnings
PASS
```

The h4nd tests cover strict key identity/label construction, exact
release-candidate-bound provisioning permit validation, P-256 public/signature
wire compatibility, tamper refusal and a real read-only Keychain lookup proving
that a missing key does not trigger provisioning.

## Source checkpoint digests

```text
m1nd-control/src/crypto_authority.rs
  9c4f62d980119b74a6fd1d982207837723f41574d21ef11a0a9587f36f0d498e
m1nd-control/tests/crypto_p256.rs
  18f35abdf9f189a3f47b3bcc193968cb669bef93d349976a19318fd17be39f3b
god-hud/h4nd-app/src-tauri/src/secure_enclave.rs
  894686ea6ac0dd4125380927afd9d927590f44f2145a79511a2406a4c2582cae
```

## Exact boundary

### Proven

- Software P-256 fixtures and the M1nd verifier agree on the exact wire contract.
- Ed25519 compatibility remains green.
- The macOS Security.framework adapter compiles and its non-mutating path runs.
- Provisioning requires an explicit candidate/authority-receipt permit object.
- Duplicate, missing, ambiguous and non-Secure-Enclave key states fail closed in
  the adapter logic.
- Human-biometric and unattended-owner roles have separate access-control specs.

### Not proven

- No production private key was created, rotated, activated or used.
- No claim is made that the current installed h4nd binary has the required
  production code-signing/keychain identity.
- A real Secure Enclave signature with biometric UI is `NOT_RUN`.
- An unattended owner signature under a promoted build is `NOT_RUN`.
- Key deletion recovery, code-signing access-group isolation and adversarial
  same-UID access remain G2/G9 proof obligations.
- The adapter is not yet wired to the new MissionService authority flow.

This boundary is deliberate: creating a protected production key is an external
state mutation and an authority ceremony. Compilation and software fixtures are
not allowed to self-ratify that ceremony.
