"""Keep live feedback guidance separate from historical evidence primitives.

Historical changelogs/translations and frozen contracts are not rewritten by
this guard. Canonical design docs carry an explicit current-scope clarification.
"""
import hashlib
from pathlib import Path
import re
import unittest

ROOT = Path(__file__).resolve().parent.parent
PRDS = (
    "docs/ORGANISM-PRD.md", "docs/NEXTGEN-AGENT-PRD.md", "docs/MEDULLA-PRD.md",
    "docs/CASE-INTELLIGENCE-PRD.md", "docs/HUMAN-LAYER-PRD.md",
)
SURFACES = PRDS + (
    "docs/AGENT-PACKS.md", "docs/PATHOS.md", "docs/UML-ORGANISM.md",
    "m1nd-core/src/trust.rs", "m1nd-core/src/antibody.rs",
)
FORBIDDEN = (
    r"learn\(correct\\?\|wrong\\?\|partial\)",
    r"(?:from|via)\s+`?learn\(\"(?:correct|wrong|partial)\"\)",
    r"plants exactly the co-change signal",
    r"trust \+ tremor \+ co-change ledger updated",
    r"kills its trust and marks it",
    r"debrief's `learn` calls feed the same co-change/trust",
)


def violations(text):
    return [pattern for pattern in FORBIDDEN if re.search(pattern, text)]


class LearningEventGuidanceTests(unittest.TestCase):
    def test_live_guidance_has_no_obsolete_feedback_instructions(self):
        for path in SURFACES:
            with self.subTest(path=path):
                self.assertEqual(violations((ROOT / path).read_text()), [])

    def test_known_legacy_teachings_are_rejected(self):
        for text in (
            "learn(correct|wrong|partial)",
            r"learn(correct\|wrong\|partial)",
            'Number of confirmed defects (from `learn("correct")`).',
            'Antibodies created via `learn("correct")`',
            "plants exactly the co-change signal",
            "trust + tremor + co-change ledger updated",
            "kills its trust and marks it for consolidation",
            "debrief's `learn` calls feed the same co-change/trust calibration",
        ):
            with self.subTest(text=text):
                self.assertTrue(violations(text))

    def test_canonical_designs_expose_the_current_scope(self):
        for path in PRDS:
            text = (ROOT / path).read_text()
            with self.subTest(path=path):
                self.assertIn("Current feedback scope", text)
                self.assertIn("Public learning/debrief/calibration floors remain blocked", text)
                self.assertIn("no new verified-defect ingress", text)
                self.assertIn("LEARNING-EVENT-SEMANTICS.md", text)

    def test_frozen_contract_bytes_are_preserved(self):
        for path, digest in (
            ("docs/M1ND-10-PRD.md", "2745560daf6e5cf6237b84663f895e81e2c4979de4190dfef649b032b680f87b"),
            ("docs/M1ND-10-UML.md", "d5bc29776f516c300cb1a0668f0a53844286f75395fd0ad1e875b52ea3a067a5"),
        ):
            self.assertEqual(hashlib.sha256((ROOT / path).read_bytes()).hexdigest(), digest)


if __name__ == "__main__":
    unittest.main()
