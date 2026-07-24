"""No agent-facing surface may teach the unreachable cross-root bootstrap.

`m1nd-mcp/src/server.rs` already asserts that the served `M1ND_INSTRUCTIONS` never
names the withdrawn bootstrap call, and that the published `ingest` schema carries
neither `project_root` nor `allow_overlap`. The prose surfaces had no such guard,
so they drifted behind the runtime and v1.5.0 shipped five documents instructing
agents to call a verb that fails closed with
`brain_bootstrap_consumer_not_installed`.

This test extends the runtime's guard to the prose, so the two can no longer
disagree silently.
"""

import pathlib
import unittest

REPO_ROOT = pathlib.Path(__file__).resolve().parent.parent

# The teaching forms — the imperative/advertising phrasings that send an agent to
# the refused call. Mentioning the parameters in the NEGATIVE ("`ingest` does not
# accept `project_root`") is exactly what these surfaces should say, so the list
# targets the instruction, not the token.
FORBIDDEN_TEACHINGS = (
    "ONE call sets you up",
    "ingest project_root=",
    "`ingest` with `project_root=",
    "pass `allow_overlap:true`",
)

# Everything an agent or a new user reads as instruction. AGENTS.md is included
# because the repo's own doctrine file drifted too — it taught the withdrawn call
# in both write laws while the runtime had already withdrawn it.
AGENT_SURFACE_GLOBS = (
    "skills/**/*.md",
    "docs/wiki/src/**/*.md",
    "AGENTS.md",
    "README.md",
    "CLAUDE.md",
)

# The changelog records what past releases shipped; rewriting it would falsify
# history. It is exempt ONLY while it carries the superseded banner that stops a
# reader from acting on the withdrawn entry — see the assertion below, which
# keeps this exemption from rotting into a loophole.
HISTORICAL_RECORD = pathlib.Path("docs/wiki/src/changelog.md")
SUPERSEDED_BANNER = "**Superseded — do not follow this entry today:**"


class AgentSurfaceBootstrapHonesty(unittest.TestCase):
    def _agent_surfaces(self):
        seen = set()
        for pattern in AGENT_SURFACE_GLOBS:
            for path in sorted(REPO_ROOT.glob(pattern)):
                relative = path.relative_to(REPO_ROOT)
                if relative == HISTORICAL_RECORD or relative in seen:
                    continue
                seen.add(relative)
                yield relative, path

    def test_no_agent_surface_teaches_the_withdrawn_bootstrap(self):
        offences = []
        for relative, path in self._agent_surfaces():
            text = path.read_text(encoding="utf-8")
            for teaching in FORBIDDEN_TEACHINGS:
                if teaching in text:
                    line = next(
                        (
                            index
                            for index, content in enumerate(text.splitlines(), 1)
                            if teaching in content
                        ),
                        0,
                    )
                    offences.append(f"{relative}:{line} teaches {teaching!r}")

        self.assertEqual(
            offences,
            [],
            "agent-facing prose must not teach the cross-root bootstrap the runtime "
            "refuses (brain_bootstrap_consumer_not_installed); the published `ingest` "
            "schema carries neither `project_root` nor `allow_overlap`:\n  "
            + "\n  ".join(offences),
        )

    def test_the_historical_record_keeps_its_superseded_banner(self):
        changelog = REPO_ROOT / HISTORICAL_RECORD
        text = changelog.read_text(encoding="utf-8")
        teaches = [t for t in FORBIDDEN_TEACHINGS if t in text]
        if not teaches:
            return
        self.assertIn(
            SUPERSEDED_BANNER,
            text,
            f"{HISTORICAL_RECORD} still records the withdrawn bootstrap "
            f"({teaches}) but has lost the banner telling readers not to follow it; "
            "either restore the banner or remove the stale instruction",
        )

    def test_the_surfaces_state_the_honest_refusal_code(self):
        # The fix is not merely deleting the lie: a reader who hits a mismatch must
        # be handed the honest state. The two first-contact surfaces must name it.
        for relative in (
            pathlib.Path("skills/m1nd-universal-agent-pack.md"),
            pathlib.Path("skills/m1nd-operator/SKILL.md"),
        ):
            text = (REPO_ROOT / relative).read_text(encoding="utf-8")
            self.assertIn(
                "brain_bootstrap_consumer_not_installed",
                text,
                f"{relative} must name the honest bootstrap state, not just omit the lie",
            )


if __name__ == "__main__":
    unittest.main()
