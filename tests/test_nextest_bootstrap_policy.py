"""Bootstrap processes must not contend for the embedding owner in local nextest."""

from pathlib import Path
import re
import unittest


CONFIG = Path(__file__).resolve().parents[1] / ".config/nextest.toml"
BINARY_IDS = (
    "m1nd-mcp::agent_autonomy_bootstrap",
    "m1nd-mcp::agent_autonomy_http_bootstrap",
)


class BootstrapConcurrencyPolicyTest(unittest.TestCase):
    def test_default_profile_serializes_both_bootstrap_binaries(self):
        text = CONFIG.read_text()
        self.assertRegex(text, r"(?m)^agent-bootstrap = \{ max-threads = 1 \}$")
        default_overrides = re.findall(
            r"(?ms)^\[\[profile\.default\.overrides\]\]\n(.*?)(?=^\[|\Z)", text
        )
        filters = [
            match.group(1)
            for override in default_overrides
            if "test-group = 'agent-bootstrap'" in override
            for match in [re.search(r"(?m)^filter = '([^']+)'$", override)]
            if match is not None
        ]
        self.assertTrue(
            any(all(f"binary_id({binary_id})" in expression for binary_id in BINARY_IDS)
                for expression in filters),
            "both bootstrap binaries must share the bounded group in the default profile",
        )


if __name__ == "__main__":
    unittest.main()
