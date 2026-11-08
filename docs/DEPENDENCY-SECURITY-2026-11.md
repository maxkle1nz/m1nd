# November dependency security update

This unreleased source candidate updates seven locked packages. It does not
replace published 1.6.5 artifacts or change source APIs, features, persistence
formats, authority rules, or audit policy.

| Package | Previous | Candidate |
| --- | --- | --- |
| crypto-bigint | 0.7.5 | 0.7.9 |
| ctutils | 0.4.2 | 0.4.5 |
| quinn-proto | 0.11.16 | 0.11.18 |
| rustls | 0.23.45 | 0.23.46 |
| rustls-webpki | 0.103.15 | 0.103.16 |
| webpki-root-certs | 1.0.6 | 1.0.10 |
| webpki-roots | 1.0.8 | 1.0.10 |

The existing webpki-roots 0.26.11 compatibility package forwards to 1.0.10.
The package count remains 472; unrelated dependency edges are preserved.
crypto-bigint 0.7.9 is the first non-yanked corrected patch; it requires
ctutils >=0.4.5. Both now declare Rust 1.87. The root packages now declare
Rust 1.71. These are dependency-level requirement increases, although the
existing workspace lock already includes sysinfo with a Rust 1.95 requirement.
Use a current stable toolchain; this update does not establish a lower workspace
MSRV.

## Audit boundary

Against RustSec database revision
`26fd2d43f7280a1fb925a3743ec463017afe7119` (2026-11-05), the main lockfile at
`a300897072208b4a500f670adbab6d144345022e` reports 14 vulnerabilities, and this
candidate lockfile reports zero, with no ignores or target/severity filters.
Six informational warnings remain: four unmaintained-package and two unsoundness
warnings. Zero vulnerability findings does not mean all dependency debt is paid.
Run `cargo audit --file Cargo.lock` against the current advisory database when
assessing a later revision.

The changes cover QUIC resource-exhaustion/panic findings, TLS secret disclosure
and handshake handling, certificate distrust/root updates, and constant-time
arithmetic findings. P-256 persisted-signature continuity and normal
certificate-validated model download are compatibility checks, not exploit
reproductions or a general cryptographic security assessment.

## Follow-up scope and pending branches

### UI audit follow-up

The source candidate also updates Vite 8.1.5 to 8.1.6 and the transitive Zod
resolution from 4.4.3 to 4.6.7. The npm audit comparison drops from twelve
findings (two high, ten moderate) to ten moderate findings, with zero high or
critical findings. Existing platform selectors and the dependency inventory are
preserved. The high-severity audit gate and lint rules are unchanged.

The remaining ten moderate entries share the minimatch advisory through the
ESLint dependency chain; they are not ten independent defects. Its current
consumers still require minimatch 10, which has no compatible patched release
for this advisory. This candidate neither forces minimatch 11 outside those
ranges nor downgrades ESLint. That migration remains explicit dependency debt;
passing `npm audit --audit-level=high` does not mean a zero-finding npm audit.

### Runtime overlay

The candidate imports only the existing runtime-overlay optimization and its
regressions from PR #588. Matching rules and heat arithmetic are preserved;
substring filters and label signatures are reused within a batch. See the
[matching contract and worst-case limits](RUNTIME-OVERLAY-MATCHING.md). The
five-second local stress assertion is unchanged. Performance proof belongs to
the exact tested candidate, not the donor branch's historical measurements.

PR #586 and the composed PR #588 already carry the same quinn-proto and rustls
patches. This candidate extends that narrow remediation with five package
updates. PR #587 is a broader dependency batch and does not replace this fix.
Reconcile Cargo.lock when integrating either branch rather than reverting these
security versions.

The formerly public Rust coverage map is encapsulated only in PR #588. That
source API migration is outside this lockfile update and still needs its own
compatibility review. This candidate preserves main's Rust and MCP APIs.
Local test results and hosted CI must be assessed at the exact candidate commit;
another PR's green run does not qualify it. Integration and release remain
separate from this source candidate.
