"""Keep first-use documentation within the shipped release and evidence gates.

These are prose-interface regressions, not runtime or statistical proofs. The
historical 1.6.3 recipe and versioned changelog records have separate boundaries.
"""

import os
import pathlib
import re
import subprocess
import unittest


REPO_ROOT = pathlib.Path(__file__).resolve().parent.parent
RELEASE_SHA = "363bc97f2336c73e90802916804dccd2109c5746"
ENTRY_POINTS = (
    "README.md",
    "llms-install.md",
    "docs/wiki/src/introduction.md",
    "docs/wiki/src/faq.md",
    "docs/wiki/src/tutorials/quickstart.md",
)


def calibration_teachings(text):
    # Match affirmative teaching forms, while permitting descriptions such as
    # "running calibration does not enable act" and historical supersession.
    normalized = re.sub(r"\s+", " ", text.replace("`", ""))
    patterns = (
        r"\bcalibrate_(?:predict|envelope)\s+once\b",
        r"\bcalibrate_(?:predict|envelope)\s+(?:arms?|enables?|unlocks?)\b",
        r"\bgate\s+is\s+armed\b.{0,80}\b(?:running|calling)\b",
    )
    return [match.group() for pattern in patterns for match in re.finditer(pattern, normalized, re.I)]


def current_readme(text):
    return text.split("### Historical pinned 1.6.3:", 1)[0]


class PublicReleaseDocumentation(unittest.TestCase):
    def read(self, relative):
        return (REPO_ROOT / relative).read_text(encoding="utf-8")

    def test_published_entry_points_identify_exact_release_and_later_main(self):
        for relative in ENTRY_POINTS:
            text = self.read(relative)
            with self.subTest(path=relative):
                self.assertIn(RELEASE_SHA, text)
                self.assertIn("1.6.5", text)
                self.assertIn("PR580", text)
                self.assertIn("Unreleased", text)
                self.assertNotRegex(text, r"npm 1\.6\.5.{0,50}(?:pending|remains unpublished)")
                self.assertNotRegex(text, r"latest published release.{0,50}1\.6\.3")

    def test_current_commands_use_matching_released_wrapper_and_binary(self):
        for relative in ("README.md", "llms-install.md", "docs/wiki/src/tutorials/quickstart.md"):
            text = self.read(relative)
            if relative == "README.md":
                text = current_readme(text)
            blocks = "\n".join(re.findall(r"```(?:bash|sh)\n(.*?)```", text, re.S))
            with self.subTest(path=relative):
                self.assertIn("@maxkle1nz/m1nd@1.6.5 agent first-minute", blocks)
                self.assertIn("versions/1.6.5/m1nd-mcp", blocks)
                self.assertNotIn("@maxkle1nz/m1nd@1.6.3", blocks)
                first_minute = next(block for block in blocks.split("\n\n") if "agent first-minute" in block)
                self.assertIn('--binary "$M1ND_BINARY"', first_minute)

    def test_current_onboarding_preserves_external_runtime_and_authority_boundary(self):
        for relative in ("llms-install.md", "docs/wiki/src/tutorials/quickstart.md"):
            text = self.read(relative)
            with self.subTest(path=relative):
                self.assertIn('"M1ND_WORKSPACE_ROOT":', text)
                self.assertRegex(text, r'"M1ND_RUNTIME_DIR"\s*:\s*"/absolute/private/runtime/repo-alpha"')
                self.assertIn("install -d -m 700", text)
                self.assertIn("generic", text)
                self.assertIn("shared", text.lower())
                self.assertRegex(text, r"(?:[Ss]ource files remain inputs|preserves source files)")
                self.assertNotIn('"M1ND_RUNTIME_DIR": "/absolute/path/to/project/.m1nd"', text)

    def test_refresh_guarantee_is_scoped_to_reused_automatic_isolated_cache(self):
        for relative in ("README.md", "llms-install.md", "docs/wiki/src/tutorials/quickstart.md",
                         "skills/m1nd-first/SKILL.md", "skills/m1nd-operator/SKILL.md"):
            paragraphs = re.split(r"\n\s*\n", self.read(relative))
            startup = [p for p in paragraphs if "first-minute" in p and "before retrieval" in p]
            with self.subTest(path=relative):
                self.assertTrue(startup, "first-use freshness boundary must be explicit")
                for paragraph in startup:
                    normalized = re.sub(r"\s+", " ", paragraph)
                    self.assertIn("Only when reusing the automatic isolated cache", normalized)
                    self.assertIn("Attached-owner freshness follows the owner's own semantics", normalized)

    def test_pinned_contributor_recipe_cannot_discover_an_older_owner(self):
        contributor = self.read("README.md").split("### Unreleased main:", 1)[1].split("## What to try next", 1)[0]
        recipe = re.findall(r"```bash\n(.*?)```", contributor, re.S)[0]
        self.assertIn("git checkout --detach 74d0348c9a5a1e0d5d5e5b5a784ee4f3771d0808", recipe)
        self.assertIn("--binary ./target/release/m1nd-mcp --no-attach", recipe)
        self.assertIn("unset M1ND_RUNTIME_DIR M1ND_MCP_ARGS M1ND_ATTACH_URL", recipe)
        self.assertIn("older covering owner", contributor)

    def test_instructional_calibration_does_not_advertise_act_repair(self):
        surfaces = (*ENTRY_POINTS, "docs/AGENT-PACKS.md", "docs/CALIBRATION-QUALIFICATION.md",
                    "docs/wiki/src/api-reference/overview.md", "docs/wiki/src/tool-matrix.md",
                    "skills/m1nd-first/SKILL.md", "skills/m1nd-operator/SKILL.md")
        for relative in surfaces:
            with self.subTest(path=relative):
                self.assertEqual(calibration_teachings(self.read(relative)), [])
        installer = self.read("llms-install.md")
        for term in ("legacy_evidence_unqualified", "abstains", "reverify", "unprovable", "policy-disabled"):
            self.assertIn(term, installer)
        self.assertRegex(installer, r"running\s+them does not enable `act`")

    def test_calibration_guard_rejects_old_and_equivalent_false_teachings(self):
        old = "the conformal gate is armed per-repo by running\n `calibrate_predict` once"
        for text in (old, "calibrate_predict enables act", "calibrate_envelope arms the gate"):
            self.assertTrue(calibration_teachings(text), text)
        self.assertEqual(calibration_teachings("Running calibrate_predict does not enable act."), [])

    def test_tracking_guide_identifies_shipped_scope_without_durable_lock_claim(self):
        text = self.read("docs/GRAPH-CHANGE-TRACKING.md")
        self.assertIn(RELEASE_SHA, text)
        self.assertIn("Published in signed 1.6.5", text)
        self.assertNotIn("Unreleased source-build recovery", text)
        self.assertNotIn("1.6.5 source candidate", text)
        for limit in ("session-local", "not filesystem locks", "periodic` refuses", "authority"):
            self.assertIn(limit, text)

    def test_signed_install_recipe_is_parseable_and_verifies_before_install(self):
        text = self.read("docs/INSTALL-1.6.5.md")
        recipe = re.findall(r"```sh\n(.*?)```", text, re.S)[0]
        parsed = subprocess.run(["sh", "-n"], input=recipe, text=True, capture_output=True)
        self.assertEqual(parsed.returncode, 0, parsed.stderr)
        for required in (
            "set -eu", "releases/download/v1.6.5", "$asset.sigstore.json",
            "--certificate-identity https://github.com/maxkle1nz/m1nd/.github/workflows/release.yml@refs/tags/v1.6.5",
            "--certificate-oidc-issuer https://token.actions.githubusercontent.com",
            f"'m1nd-mcp 1.6.5 ({RELEASE_SHA})'",
            f"'m1nd-mcp 1.6.5 ({RELEASE_SHA}-dirty)'",
            '*) echo "Unexpected 1.6.5 native identity: $reported_version" >&2; exit 1',
        ):
            self.assertIn(required, recipe)
        self.assertLess(recipe.index("cosign verify-blob"), recipe.index('install -m 755'))
        self.assertLess(recipe.index('case "$reported_version"'), recipe.index('install -m 755'))
        self.assertNotRegex(recipe, r"cosign[^\n]*(?:\|\|\s*true|--insecure)")

    def test_documented_identity_check_accepts_published_marker_and_refuses_other_revisions(self):
        recipe = re.findall(r"```sh\n(.*?)```", self.read("docs/INSTALL-1.6.5.md"), re.S)[0]
        start = recipe.index('case "$reported_version"')
        # Execute only the documented identity check, with no download or install.
        check = recipe[start:recipe.index('destination="$HOME', start)]
        cases = (
            (f"m1nd-mcp 1.6.5 ({RELEASE_SHA})", True),
            (f"m1nd-mcp 1.6.5 ({RELEASE_SHA}-dirty)", True),
            (f"m1nd-mcp 1.6.3 ({RELEASE_SHA})", False),
            ("m1nd-mcp 1.6.5 (0000000000000000000000000000000000000000)", False),
            (f"m1nd-mcp 1.6.5 ({RELEASE_SHA}-unexpected)", False),
        )
        for version, accepted in cases:
            with self.subTest(version=version):
                env = {**os.environ, "reported_version": version}
                result = subprocess.run(["sh", "-c", check], env=env, capture_output=True, text=True)
                self.assertEqual(result.returncode == 0, accepted, result.stderr)

    def test_agent_pack_scopes_legacy_birth_guidance_to_its_authority_contract(self):
        for relative in ("skills/m1nd-first/SKILL.md", "skills/m1nd-operator/SKILL.md",
                         "skills/m1nd-universal-agent-pack.md"):
            text = self.read(relative)
            with self.subTest(path=relative):
                self.assertIn("Published 1.6.5", text)
                self.assertIn("first-minute --repo", text)
                self.assertIn("external private", text)
                self.assertIn("legacy", text)
                self.assertIn("shared", text)
                self.assertIn("human", text)

    def test_historical_163_installation_remains_separate(self):
        historical = self.read("docs/INSTALL-1.6.3.md")
        self.assertIn("releases/download/v1.6.3", historical)
        self.assertIn("release.yml@refs/tags/v1.6.3", historical)
        self.assertIn("versions/1.6.3", historical)
        self.assertNotIn("releases/download/v1.6.5", historical)
        changelog = self.read("docs/wiki/src/changelog.md")
        section = changelog.split("## [1.2.0]", 1)[1].split("## [", 1)[0]
        self.assertIn("Superseded — do not follow this entry today:", section)
        self.assertIn("unqualified", section)

    def test_landing_keeps_setup_and_evidence_claims_within_current_contract(self):
        install = self.read("m1nd-demo/src/components/InstallSection.tsx")
        faq = self.read("m1nd-demo/src/components/FAQSection.tsx")
        self.assertIn("@maxkle1nz/m1nd@1.6.5 agent first-minute", install)
        self.assertIn("INSTALL-1.6.5.md", install)
        self.assertNotIn("init --birth", install)
        self.assertNotIn("1.6.5 source preview", faq)
        self.assertIn("shared brains retain separate authority requirements", faq)
        memory = self.read("m1nd-demo/src/components/MemoryFeature.tsx")
        reasoning = self.read("m1nd-demo/src/components/ReasoningFeature.tsx")
        impact = self.read("m1nd-demo/src/components/ImpactFeature.tsx")
        for phrase in ("Exactly Where", "full trail", "No re-reading files", "Continued in milliseconds"):
            self.assertNotIn(phrase, memory)
        self.assertIn("can refuse", memory)
        self.assertIn("downgrade hypotheses", memory)
        self.assertNotIn("under a millisecond", reasoning)
        self.assertNotIn("Verify a Claim in One Query", reasoning)
        self.assertIn("evidence and gaps", reasoning)
        self.assertNotIn("most likely to break", impact)
        self.assertIn("to inspect in the graph", impact)


if __name__ == "__main__":
    unittest.main()
