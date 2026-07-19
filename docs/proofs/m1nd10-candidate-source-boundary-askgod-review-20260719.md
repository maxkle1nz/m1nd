# M1ND-10 candidate source boundary — askGOD review — 2026-07-19

## Binding and outcome

| Field | Value |
|---|---|
| Review mode | Isolated Fugu Ultra, read-only, bounded candidate-source review |
| Repository base | `b59a1c2a1454a83164dfb4d5640c6b005154d1ee` |
| Verdict | `CHANGE` |
| Confidence | `high` |
| Owner/port contact | none; installed owner and port 1338 were outside scope |
| Private benchmark access | none; no `operator-only` or `runner-results` content was opened |
| Repository mutation by reviewer | none |
| Pre/post status-shape SHA-256 | `72b8a06bef56d51a5ded0445df6782cb75282052b2f263c9bf2015b4e4a710f1` |
| Pre/post public tracked-diff SHA-256 | `7533f410504bc3cd1e1b25d6853305e6d820a7d8e04dbb14f026fba757780e30` |
| Pre/post untracked-public-content SHA-256 | `17f3f19b0a9b2b18ddf6baaf9cac44feff78502ca2f3e6844fa751e6c533b53c` |

The first broad Fugu process entered a repeated compaction/re-read loop and produced no verdict. It
was stopped without touching the repository. One isolated retry then reviewed only the allowed
candidate-boundary files and returned the contract below. The three pre/post fingerprints match,
so the verdict binds the exact reviewed source state.

The original verdict necessarily named the machine-local absolute path that demonstrated the
public-no-leak defect. This public copy replaces that value with `<repo-root>` and otherwise
preserves the verdict contract. The redaction does not alter the finding or required change.

## Verdict contract

```text
VERDICT: CHANGE
CONFIDENCE: high
EVIDENCE:
- scripts/m1nd10_candidate_source_guard.py:31 — PRIVATE_COMPONENTS is the lowercase literal set {"operator-only","runner-results"}.
- scripts/m1nd10_candidate_source_guard.py:44 — SECRET_BASENAMES is the exact set {".env","runnerd.secret","runners.toml"}; no `.env.*`, `.npmrc`, `.cargo/credentials`, or SSH keys.
- scripts/m1nd10_candidate_source_guard.py:65-72 — operator/component/basename checks compare raw `path_text`/`path.parts`/`path.name` with no case-folding; only the suffix checks at :69 and :73 normalize with `.lower()`, so `.ENV`, `Operator-Only`, `Node_Modules`, `.DS_STORE` all return None while `release.PEM` is caught.
- scripts/m1nd10_candidate_source_guard.py:48,73 — archives are governed only by MAX_BLOB_BYTES (no `.zip/.tar/.tgz/...` class) and key suffixes stop at `.key/.p12/.pem/.pfx`; `secrets/archive.zip`, `.cargo/credentials.toml`, `.ssh/id_ed25519`, `foo/runners.toml.bak` return None.
- scripts/m1nd10_candidate_source_guard.py:217,235-236 — inspect_candidate/inspect_worktree_projection merge only path-policy + metadata violations; neither reads blob text, so no public-content policy is enforced.
- docs/security/m1nd10-g10-security-assessment-20260718.md:156 — public artifact contains personal absolute path `<repo-root>`; file is untracked and `git check-ignore` returns exit 1 (not ignored), so a `git add -A`/candidate commit includes it and no current gate (guard has no content scan; Gitleaks is not a policy scanner) refuses it.
- tests/test_m1nd10_candidate_source_guard.py — refusal tests use only exact lowercase forms; grep finds no case-variant, `.env.*`, `.npmrc`, SSH, credential, archive, or content coverage.
- tests/test_m1nd10_ci_security_contract.py:103,105 — assert the guard runs against `--revision "${GITHUB_SHA}"` in both CI and the tag guard, but no test asserts a content-leak gate, archive denial, case-folding, or credential-pattern policy.
- SOUND (preserve): scripts/m1nd10_candidate_source_guard.py:99-109 exact 40-hex commit resolution; :157-169 non-mutating `git add -A` projection; :172-185 symlink/gitlink/non-regular/oversized denial; ci.yml:145-147 & release.yml:82-84 guard invoked against exact `${GITHUB_SHA}`; ci.yml:149-152 & release.yml:86-89 Gitleaks pinned to full SHA ff98106e…/8.30.1; docs/proofs/m1nd10-candidate-source-boundary-20260719.md:9-11 honest NOT_PROVEN/NOT_RUN separation.
RATIONALE:
The architecture is sound and worth keeping (exact Git identity, non-mutating projection, metadata denial, SHA-pinned workflow gates, honest NOT_PROVEN/NOT_RUN), but the path policy is fail-open, not fail-closed. Matching is a case-sensitive closed list, so operator-only/runner-results/node_modules/.DS_Store/.env/runners.toml bypass via trivial case variants — reproduced by the probe and confirmed in source — directly defeating the operator-private guarantee this boundary exists to enforce. Whole credential classes (`.env.*`, `.npmrc`, cargo/aws credentials, SSH keys, `.netrc`, `.git-credentials`) and opaque archives pass unseen, and Gitleaks explicitly does not unpack archives, so private/label/secret material can ship inside a container. Critically, in the enforced exact-commit `git ls-tree` mode `.gitignore` performs no filtering (its `*.bak`, `.DS_Store`, component rules only shape the worktree projection), so the case-sensitive guard is the sole path gate against a force-added or arbitrary commit. Separately, a public-no-leak law violation (a personal absolute path) sits in an untracked, non-ignored candidate file that no current gate can catch. These defects are demonstrated, not hypothetical, so the boundary cannot honestly be called fail-closed; they are bounded and correctable, so this is CHANGE, not REJECT.
REQUIRED_CHANGES:
1. Case-fold path matching: compare PRIVATE_COMPONENTS, CACHE_COMPONENTS, SECRET_BASENAMES, and GENERATED_BASENAMES against casefolded `path.parts`/`path.name` (suffix sets already use `.lower()`). This closes the operator-only/runner-results/node_modules/.DS_Store/.env/runners.toml case bypasses.
2. Broaden secret/credential coverage beyond exact `.env`: refuse basename `.env` and `.env.*`; add `.npmrc`, `.pypirc`, `.netrc`, `.git-credentials`, `credentials`/`credentials.toml` under `.cargo`/`.aws`-class components; SSH private keys (anything under `.ssh/`, extensionless `id_rsa`/`id_dsa`/`id_ecdsa`/`id_ed25519`, and `*_rsa`/`*_ed25519`); plus `.p8`/`.der`/`.jks`/`.keystore` key material, keeping existing `.key/.p12/.pem/.pfx`.
3. Deny opaque archive/container formats (`.zip`, `.tar`, `.tar.gz`/`.tgz`, `.tar.bz2`/`.tbz2`, `.tar.xz`/`.txz`, `.gz`, `.bz2`, `.xz`, `.7z`, `.rar`, `.jar`) under a new closed reason (e.g. `opaque_archive`) unless a separate, enforced unpack-and-Gitleaks step is added — today neither the guard nor Gitleaks inspects archive contents.
4. Remove the machine-local absolute repository path from docs/security/…:156 (and any sibling occurrence) in favor of a repo-relative placeholder, and add a mechanical candidate-tree public-content gate (guard content check or CI job) that fails closed on personal absolute paths (`/Users/<name>/…`, `/home/<name>/…`, Windows `C:\Users\<name>\…`), since Gitleaks does not cover this class.
5. Add adversarial unit tests for every variant (case variants of each component/basename, `.env.local`, `.npmrc`, `.cargo/credentials.toml`, `.ssh/id_ed25519`, `id_rsa`, uppercase `.DS_STORE`, each archive extension) asserting the exact new reasons, and add CI/release semantic assertions in tests/test_m1nd10_ci_security_contract.py that both workflows invoke the guard against `${GITHUB_SHA}` AND that the personal-path/content gate is wired, so neither can silently regress.
RISKS_MISSED:
- Even after these fixes both gates remain closed-list/pattern-based; a novel unenumerated private class or a renamed/encrypted container can still pass — fail-closed only holds for enumerated classes, not universally.
- No immutable candidate exists; all evidence is a dirty worktree projection, so exact-commit enforcement and hosted execution remain NOT_RUN/NOT_PROVEN and unverified against the final identity.
- In the enforced `git ls-tree` mode `.gitignore` gives zero protection; if the guard's lists drift from `.gitignore`, force-added or arbitrary-commit paths bypass silently.
- The added content check covers only personal-absolute-path leakage; other public-no-leak classes (internal hostnames, operator identifiers, tokens Gitleaks misses) remain unguarded.
```

## Consequence

Checkpoint 25's local commands remain historical evidence that the original tests passed. They do
not prove the stated fail-closed boundary. The candidate-source boundary is now `CHANGE_REQUIRED`
and blocks candidate freeze until every required change is implemented, tested adversarially,
rerun against both worktree projection and an authorized exact commit, and independently reviewed.

## Post-review discovery

After preserving the verdict, a broader public-path census over candidate-visible files (with all
private benchmark components excluded) found the current machine-local prefix 509 times across 143
files. Historical benchmark documentation accounts for 134 files; the frozen PRD also contains
three occurrences. This evidence was outside Fugu's intentionally narrow file scope and therefore
does not alter its transcript. It expands required change 4 into a governed migration problem.

The frozen PRD must not be edited in place. Noncanonical occurrences must be scrubbed or retired
with evidence bindings preserved or regenerated, and any canonical amendment or exact-digest
exception requires explicit owner ratification. A blanket allowlist would conceal the defect and is
not an acceptable closure.
