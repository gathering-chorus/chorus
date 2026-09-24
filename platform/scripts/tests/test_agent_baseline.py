import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

SPEC = importlib.util.spec_from_file_location("agent_baseline", Path(__file__).resolve().parents[1] / "agent-baseline.py")
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


class BaselineTest(unittest.TestCase):
    def test_inventory_never_outputs_config_values_or_changes_files(self):
        with tempfile.TemporaryDirectory() as root:
            home = Path(root)
            settings = home / ".claude/settings.json"
            settings.parent.mkdir()
            original = json.dumps({"env": {"SECRET_KEY": "private-token-value"},
                                   "hooks": {"PreToolUse": [{"command": "sensitive-command-argument"}]}})
            settings.write_text(original)
            report = MODULE.baseline(home, home / "chorus", probe=False)
            encoded = json.dumps(report)
            self.assertIn("SECRET_KEY", encoded)
            self.assertNotIn("private-token-value", encoded)
            self.assertNotIn("sensitive-command-argument", encoded)
            self.assertEqual(settings.read_text(), original)
            self.assertEqual(report["session_registry"]["v2_count"], 0)


if __name__ == "__main__":
    unittest.main()
