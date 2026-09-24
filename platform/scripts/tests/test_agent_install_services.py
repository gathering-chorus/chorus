"""Hermetic activation planning and wrapper runtime tests; no launchctl calls."""
import copy
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agent_install_services import prepare_services, ServicePlanError


class ServicePlanTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.home = Path(self.temp.name)
        self.root = self.home / "selected checkout"
        self.node = str(self.home / "node22/bin/node")
        self.home_patch = patch.dict(os.environ, {"HOME": str(self.home)})
        self.home_patch.start()
        self.addCleanup(self.home_patch.stop)

    def service(self, label, args, **extra):
        value = {"Label": label, "ProgramArguments": args, "KeepAlive": True,
                 "StandardOutPath": "/existing/log", "EnvironmentVariables": {"CUSTOM": "preserved", "PATH": "/custom/bin:/usr/bin", "CHORUS_MCP_IDENTITY_MODE": "compatibility"}}
        value.update(extra)
        return {label: (self.home / (label + ".plist"), value)}

    def test_known_wrappers_relocate_missing_or_stale_paths_without_losing_fields(self):
        for label, wrapper, directory in [
            ("com.chorus.api", "chorus-api-wrapper.sh", "platform/api"),
            ("com.chorus.mcp", "chorus-mcp-wrapper.sh", "platform/mcp-server"),
        ]:
            with self.subTest(label=label):
                source = self.service(label, ["/bin/bash", "/missing/old/" + wrapper, "--existing-argument"])
                before = copy.deepcopy(source)
                planned = prepare_services(source, self.root, self.node)[label][1]
                self.assertEqual(source, before)
                self.assertEqual(planned["ProgramArguments"], ["/bin/bash", str(self.root / "platform/scripts" / wrapper), "--existing-argument"])
                self.assertEqual(planned["WorkingDirectory"], str(self.root / directory))
                self.assertTrue(planned["KeepAlive"])
                self.assertEqual(planned["StandardOutPath"], "/existing/log")
                environment = planned["EnvironmentVariables"]
                self.assertEqual(environment["CUSTOM"], "preserved")
                self.assertEqual(environment["CHORUS_MCP_IDENTITY_MODE"], "compatibility")
                self.assertEqual(environment["CHORUS_NODE_BIN"], self.node)
                self.assertEqual(environment["CHORUS_ROOT"], str(self.root))
                self.assertEqual(environment["CHORUS_HOME"], str(self.root))
                self.assertEqual(environment["CHORUS_MCP_DIR"], str(self.root / "platform/mcp-server"))
                self.assertTrue(environment["PATH"].startswith(str(Path(self.node).parent) + ":"))
                self.assertIn("/custom/bin", environment["PATH"].split(":"))

    def test_direct_node_services_relocate_entry_point_and_preserve_runtime_and_service_args(self):
        for label, directory, entry in [
            ("com.chorus.api", "platform/api", "server.js"),
            ("com.gathering.messaging", "platform/pulse", "service.js"),
            ("com.chorus.mcp", "platform/mcp-server", "main.js"),
        ]:
            with self.subTest(label=label):
                source = self.service(label, ["/old/node20/bin/node", "--max-old-space-size=4096", f"/old/{directory}/dist/{entry}", "--port", "1234"])
                planned = prepare_services(source, self.root, self.node)[label][1]
                self.assertEqual(planned["ProgramArguments"], [self.node, "--max-old-space-size=4096", str(self.root / directory / "dist" / entry), "--port", "1234"])
                self.assertEqual(planned["WorkingDirectory"], str(self.root / directory))

    def test_relative_node_entry_and_missing_environment_are_updated(self):
        label = "com.gathering.messaging"
        source = self.service(label, ["node", "dist/service.js"])
        del source[label][1]["EnvironmentVariables"]
        planned = prepare_services(source, self.root, self.node)[label][1]
        self.assertEqual(planned["ProgramArguments"], [self.node, str(self.root / "platform/pulse/dist/service.js")])
        self.assertIn("/usr/bin", planned["EnvironmentVariables"]["PATH"].split(":"))

    def test_hooks_use_installed_owner_binary_and_keep_arguments(self):
        label = "com.chorus.hooks"
        source = self.service(label, ["/old/build/chorus-hooks", "--port", "3342"])
        planned = prepare_services(source, self.root, self.node)[label][1]
        self.assertEqual(planned["ProgramArguments"], [str(self.home / ".chorus/bin/chorus-hooks"), "--port", "3342"])
        self.assertEqual(planned["WorkingDirectory"], str(self.root))

    def test_matching_program_field_is_updated_and_mismatched_override_refused(self):
        label = "com.chorus.api"
        wrapper = "/stale/chorus-api-wrapper.sh"
        source = self.service(label, [wrapper], Program=wrapper)
        planned = prepare_services(source, self.root, self.node)[label][1]
        self.assertEqual(planned["Program"], str(self.root / "platform/scripts/chorus-api-wrapper.sh"))
        source[label][1]["Program"] = "/custom/executable"
        with self.assertRaisesRegex(ServicePlanError, label + ".*overrides"):
            prepare_services(source, self.root, self.node)

    def test_unknown_wrappers_shell_expressions_and_wrong_scripts_refuse(self):
        label = "com.chorus.api"
        for args in [["/custom/company-api-wrapper.sh"],
                     ["/bin/bash", "-c", "exec /old/node dist/server.js"],
                     ["/bin/bash", "/custom/chorus-mcp-wrapper.sh"],
                     ["/node", "/another/project/server.js"],
                     ["/node", "--eval", "doSomething()", "dist/server.js"],
                     ["/node", "--require=unreviewed", "dist/server.js"]]:
            with self.subTest(args=args), self.assertRaisesRegex(ServicePlanError, label):
                prepare_services(self.service(label, args), self.root, self.node)

    def test_invalid_service_data_and_unknown_labels_refuse_without_mutation(self):
        for source in [self.service("com.unknown.service", ["node", "dist/main.js"]),
                       self.service("com.chorus.api", []),
                       self.service("com.chorus.api", ["/bin/bash", "/old/chorus-api-wrapper.sh"], EnvironmentVariables=[]),
                       self.service("com.chorus.api", ["/bin/bash", "/old/chorus-api-wrapper.sh"], EnvironmentVariables={"PATH": []})]:
            before = copy.deepcopy(source)
            with self.assertRaises(ServicePlanError):
                prepare_services(source, self.root, self.node)
            self.assertEqual(source, before)

    def test_planning_is_idempotent_and_requires_absolute_runtime(self):
        label = "com.chorus.api"
        source = self.service(label, ["/bin/bash", "/old/chorus-api-wrapper.sh"])
        once = prepare_services(source, self.root, self.node)
        self.assertEqual(prepare_services(once, self.root, self.node), once)
        with self.assertRaises(ServicePlanError):
            prepare_services(source, self.root, "node")

    def test_wrappers_execute_selected_node_without_loading_nvm(self):
        scripts = self.root / "platform/scripts"
        scripts.mkdir(parents=True)
        (scripts / "chorus-env-setup.sh").write_text('export CHORUS_ROOT="$TEST_CHECKOUT"\n')
        fake_node = self.home / "custom runtime/node"
        fake_node.parent.mkdir()
        fake_node.write_text('#!/bin/sh\nprintf "%s\\n" "$PWD" "$@" > "$TEST_NODE_LOG"\n')
        fake_node.chmod(0o700)
        nvm = self.home / ".nvm"
        nvm.mkdir()
        (nvm / "nvm.sh").write_text('echo unexpected-nvm >&2; exit 99\n')
        for wrapper, directory, entry in [("chorus-api-wrapper.sh", "platform/api", "server.js"),
                                          ("chorus-mcp-wrapper.sh", "platform/mcp-server", "main.js")]:
            with self.subTest(wrapper=wrapper):
                (self.root / directory).mkdir(parents=True, exist_ok=True)
                target = scripts / wrapper
                target.write_text((Path(__file__).resolve().parents[1] / wrapper).read_text())
                log = self.home / "node.log"
                env = {"HOME": str(self.home), "PATH": "/usr/bin:/bin", "CHORUS_NODE_BIN": str(fake_node),
                       "CHORUS_MCP_DIR": str(self.root / "platform/mcp-server"),
                       "TEST_CHECKOUT": str(self.root), "TEST_NODE_LOG": str(log)}
                result = subprocess.run(["/bin/bash", str(target)], env=env, text=True, capture_output=True)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(log.read_text().splitlines(), [str(self.root / directory), "dist/" + entry])


if __name__ == "__main__":
    unittest.main()
