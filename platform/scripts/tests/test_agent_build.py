"""The paired shortcut signs and installs both artifacts in an isolated world."""
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

SOURCE = Path(__file__).resolve().parents[1] / "build-signed.sh"


class AgentBuildTests(unittest.TestCase):
    def run_build(self, platform="Darwin", missing_daemon=False):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            scripts = root / "platform/scripts"
            scripts.mkdir(parents=True)
            (root / "platform/services/chorus-agent").mkdir(parents=True)
            shutil.copyfile(SOURCE, scripts / "build-signed.sh")
            fakebin = root / "fakebin"
            fakebin.mkdir()
            def executable(path, text):
                path.write_text("#!/bin/sh\n" + text)
                path.chmod(0o700)
            executable(fakebin / "cargo", 'mkdir -p target/release\nprintf cli > target/release/chorus-agent\n' +
                       ('' if missing_daemon else 'printf daemon > target/release/chorus-agentd\n'))
            executable(fakebin / "uname", f'printf "{platform}\\n"\n')
            executable(fakebin / "codesign", '''printf '%s\\n' "$*" >> "$SIGN_LOG"
if [ "$1" = "-dvvv" ]; then
  printf 'Identifier=com.chorus.fixture\\nAuthority=Fixture\\nCDHash=fixturehash\\n'
fi
''')
            executable(scripts / "chorus-bin-install", 'printf "%s\\n" "$2" >> "$INSTALL_LOG"\n')
            executable(fakebin / "chorus-log", "exit 0\n")
            env = {**os.environ, "CHORUS_ROOT": str(root), "CHORUS_HOME": str(root),
                   "HOME": str(root), "PATH": str(fakebin) + os.pathsep + os.environ["PATH"],
                   "CHORUS_TRACE_ID": "fixture", "CHORUS_CARD_ID": "fixture",
                   "SIGN_LOG": str(root / "sign.log"), "INSTALL_LOG": str(root / "install.log")}
            env.pop("BUILD_SKIP_INSTALL", None)
            result = subprocess.run(["bash", str(scripts / "build-signed.sh"), "chorus-agent"],
                                    env=env, capture_output=True, text=True, timeout=30)
            installed = (root / "install.log").read_text() if (root / "install.log").exists() else ""
            signed = (root / "sign.log").read_text() if (root / "sign.log").exists() else ""
            return result, installed, signed

    def test_cli_and_daemon_both_sign_and_install(self):
        result, installed, signed = self.run_build()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(installed, "chorus-agent\nchorus-agentd\n")
        self.assertIn("--identifier com.chorus.agent ", signed)
        self.assertIn("--identifier com.chorus.agentd ", signed)

    def test_missing_secondary_fails_before_either_install(self):
        result, installed, _ = self.run_build(missing_daemon=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(installed, "")

    def test_non_macos_records_both_unsigned_artifacts(self):
        result, installed, signed = self.run_build(platform="Linux")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(installed, "chorus-agent\nchorus-agentd\n")
        self.assertEqual(signed, "")
