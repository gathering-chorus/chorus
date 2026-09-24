"""Hermetic operator frontend tests: no subprocess, live service, or real home."""
import argparse
from contextlib import redirect_stdout, redirect_stderr
import io
import json
import os
from pathlib import Path
import plistlib
import sys
import tempfile
import unittest
from unittest import mock

SCRIPTS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(SCRIPTS))
import agent_setup as setup
import agent_opencode


class OperatorSetupTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="chorus-operator-test-")
        self.addCleanup(self.temporary.cleanup)
        self.base = Path(self.temporary.name)
        self.home = self.base / "home"
        self.home.mkdir(mode=0o700)
        self.root = self.base / "checkout"
        (self.root / "roles/wren").mkdir(parents=True)
        self.werks = self.base / "chorus-werk"
        self.werks.mkdir()
        self.environment = mock.patch.dict(os.environ, {"HOME": str(self.home), "PATH": "/usr/bin:/bin",
                                                       "EXAMPLE_PROVIDER_KEY": "not-a-real-provider-key"}, clear=True)
        self.environment.start()
        self.addCleanup(self.environment.stop)
        self.settings = setup.Settings(self.root)
        self.output = io.StringIO()
        self.stdout = redirect_stdout(self.output)
        self.stdout.__enter__()
        self.addCleanup(self.stdout.__exit__, None, None, None)
        self.stderr = redirect_stderr(self.output)
        self.stderr.__enter__()
        self.addCleanup(self.stderr.__exit__, None, None, None)
        # Accidental process/network calls fail the test instead of reaching the Mac.
        self.block_process = mock.patch.object(subprocess_module(), "run", side_effect=AssertionError("unmocked process"))
        self.block_process.start()
        self.addCleanup(self.block_process.stop)

    def config(self, **extra):
        document = {"version": 1, "profiles": {"existing-claude": {"runtime": "claude"}},
                    "roles": {"silas": "existing-claude", "wren": "existing-claude"},
                    "role_workspaces": {"silas": "/deployed/silas"}, "max_concurrent_jobs": 3,
                    "worktree_base": str(self.werks)}
        document.update(extra)
        setup.atomic_json(self.settings.config, document)
        return document

    def args(self, *values):
        return setup.parser().parse_args(values)

    def credential(self, role="wren", account=None):
        path = self.home / ".chorus/identity" / role / "cred.json"
        setup.atomic_json(path, {"hostAccount": account, "webId": "https://identity.example/wren"})
        return path

    def test_parser_exposes_separate_install_setup_switch_and_resume(self):
        install = self.args("install", "--restart-services", "--strict-mcp", "--yes")
        self.assertTrue(install.restart_services and install.strict_mcp and install.yes)
        switch = self.args("switch", "wren", "local-model", "--handoff", "/tmp/notes", "--legacy-stopped")
        self.assertEqual(switch.name, "local-model")
        self.assertTrue(switch.legacy_stopped)
        resumed = self.args("open", "wren", "--human-token-file", "/tmp/human")
        self.assertEqual(resumed.command, "open")
        self.assertFalse(hasattr(resumed, "name"))
        with self.assertRaises(SystemExit):
            self.args("setup", "wren", "--runtime", "invented")
        with self.assertRaises(SystemExit):
            self.args()

    def test_private_files_reject_symlinks_and_public_permissions(self):
        target = self.base / "private"
        target.write_text("fixture")
        target.chmod(0o600)
        alias = self.base / "alias"
        alias.symlink_to(target)
        with self.assertRaises(setup.SetupError):
            setup.private_file(alias)
        target.chmod(0o644)
        with self.assertRaises(setup.SetupError):
            setup.private_file(target)

    def test_host_account_refused_before_issuer_invocation(self):
        self.credential(account="role-owner")
        bridge = setup.CredentialBridge(self.settings, "wren")
        with mock.patch.object(setup.getpass, "getuser", return_value="contributor"), mock.patch.object(setup, "run") as issuer:
            with self.assertRaisesRegex(setup.SetupError, "role-owner"):
                bridge.refresh()
            issuer.assert_not_called()
        self.assertFalse(bridge.path.exists())
        self.assertNotIn("webId", self.output.getvalue())

    def test_refresh_publishes_private_token_without_printing_it(self):
        self.credential()
        bridge = setup.CredentialBridge(self.settings, "wren")
        with mock.patch.object(setup, "run", return_value="fixture.header.signature") as issuer:
            bridge.refresh()
        self.assertEqual(bridge.path.read_text(), "fixture.header.signature")
        self.assertEqual(bridge.path.stat().st_mode & 0o777, 0o600)
        self.assertTrue(issuer.call_args.kwargs["capture"])
        self.assertEqual(issuer.call_args.args[0][-1], "wren")
        self.assertNotIn("fixture.header.signature", self.output.getvalue())

    def test_refresh_failure_preserves_previous_token(self):
        self.credential()
        bridge = setup.CredentialBridge(self.settings, "wren")
        setup.atomic_bytes(bridge.path, b"previous.token")
        with mock.patch.object(setup, "run", side_effect=setup.SetupError("issuer unavailable")):
            with self.assertRaises(setup.SetupError):
                bridge.refresh()
        self.assertEqual(bridge.path.read_text(), "previous.token")

    def test_setup_merges_profiles_uses_unique_revisions_and_stores_no_keys(self):
        before = self.config()
        args = self.args("setup", "wren", "--protocol", "openai-chat", "--model", "coding-model",
                         "--base-url", "https://models.example/v1", "--key-env", "EXAMPLE_PROVIDER_KEY",
                         "--version", "2.0.0", "--trust-runtime")
        calls = []
        def bundle(root, candidate, name, role, directory, shim, node):
            calls.append((name, json.loads(candidate.read_text())))
            directory.mkdir(mode=0o700)
            return {"adapter_config": {"password_file": str(directory / "password")}}
        port = mock.MagicMock()
        port.__enter__.return_value.getsockname.return_value = ("127.0.0.1", 40001)
        with mock.patch.object(setup, "executable", side_effect=lambda x: "/fixture/bin/" + Path(x).name), \
             mock.patch.object(agent_opencode, "probe_version", return_value="2.0.0"), \
             mock.patch.object(setup.socket, "socket", return_value=port), \
             mock.patch.object(agent_opencode, "prepare_bundle", side_effect=bundle):
            setup.setup_profile(self.settings, args)
            setup.setup_profile(self.settings, args)
        after = self.settings.profiles()
        self.assertEqual(after["roles"], before["roles"])
        self.assertEqual(after["profiles"]["existing-claude"], before["profiles"]["existing-claude"])
        self.assertEqual(after["role_workspaces"]["silas"], "/deployed/silas")
        self.assertEqual(len(after["profiles"]), 3)
        self.assertNotEqual(calls[0][0], calls[1][0])
        latest = self.settings.metadata()["deployments"]["wren"]["opencode"]
        self.assertEqual(latest["profile"], calls[1][0])
        self.assertEqual(after["profiles"][latest["profile"]]["provider"]["api_key_env"], "EXAMPLE_PROVIDER_KEY")
        self.assertNotIn("permissions", after["profiles"][latest["profile"]]["adapter_config"])
        for path in self.settings.state.rglob("*"):
            if path.is_file():
                self.assertNotIn(b"not-a-real-provider-key", path.read_bytes())
        self.assertEqual(list(self.settings.state.glob(".profiles-*.json")), [])

    def test_setup_requires_explicit_trust_before_writes(self):
        args = self.args("setup", "wren", "--model", "provider/model", "--key-env", "EXAMPLE_PROVIDER_KEY", "--version", "2.0.0")
        with mock.patch.object(setup, "executable", return_value="/fixture/opencode"), \
             mock.patch.object(agent_opencode, "probe_version", return_value="2.0.0"), \
             mock.patch.object(setup.sys.stdin, "isatty", return_value=False):
            with self.assertRaisesRegex(setup.SetupError, "confirmation"):
                setup.setup_profile(self.settings, args)
        self.assertFalse(self.settings.state.exists())

    def test_failed_admission_keeps_default_and_is_never_retried(self):
        self.config()
        before = self.settings.config.read_bytes()
        item = {"profile": "new-opencode", "workspace": str(self.root / "roles/wren")}
        with mock.patch.object(setup, "uds", side_effect=setup.SetupError("connection outcome unknown")) as api:
            with self.assertRaises(setup.SetupError):
                setup.switch_session(self.settings, "wren", item, self.base / "token")
        self.assertEqual(api.call_count, 1)
        self.assertEqual(self.settings.config.read_bytes(), before)

    def test_switch_submits_exact_old_session_and_handoff_once(self):
        self.config()
        expected = {"session_id": "new-session", "profile": "new-opencode"}
        item = {"profile": "new-opencode", "workspace": str(self.root / "roles/wren")}
        with mock.patch.object(setup, "uds", return_value=expected) as api:
            result = setup.switch_session(self.settings, "wren", item, self.base / "token",
                                          {"session_id": "explicit-old"}, "Task: card 42; worktree: /fixture; obligations: review")
        self.assertEqual(result, expected)
        self.assertEqual(api.call_count, 1)
        self.assertEqual(api.call_args.args[1], "/v1/sessions/explicit-old/switch")
        self.assertIn("obligations", api.call_args.args[2]["context"])
        self.assertEqual(self.settings.profiles()["roles"]["wren"], "new-opencode")

    def test_save_failure_after_admission_is_named_and_not_retried(self):
        self.config()
        before = self.settings.config.read_bytes()
        with mock.patch.object(setup, "uds", return_value={"session_id": "admitted-123", "profile": "new"}) as api, \
             mock.patch.object(setup, "save_config", side_effect=setup.SetupError("reload refused")):
            with self.assertRaisesRegex(setup.SetupError, "admitted-123.*Do not retry"):
                setup.switch_session(self.settings, "wren", {"profile": "new", "workspace": "/fixture"}, self.base / "token")
        self.assertEqual(api.call_count, 1)
        self.assertEqual(self.settings.config.read_bytes(), before)

    def test_config_reload_failure_restores_file(self):
        document = self.config()
        before = self.settings.config.read_bytes()
        self.settings.socket.parent.mkdir(mode=0o700, exist_ok=True)
        self.settings.socket.touch()
        document["roles"]["wren"] = "new"
        with mock.patch.object(setup, "uds", side_effect=setup.SetupError("busy")) as api:
            with self.assertRaises(setup.SetupError):
                setup.save_config(self.settings, document)
        self.assertEqual(api.call_count, 1)
        self.assertEqual(self.settings.config.read_bytes(), before)

    def test_resume_uses_explicit_saved_session(self):
        self.config()
        with mock.patch.object(setup, "uds", return_value={"session_id": "explicit-old", "profile": "existing-claude"}) as api:
            setup.switch_session(self.settings, "wren", {}, self.base / "token", {"session_id": "explicit-old"}, resume=True)
        self.assertEqual(api.call_args.args[1], "/v1/sessions/explicit-old/resume")
        self.assertEqual(api.call_args.args[2], {})

    def test_read_only_check_never_creates_state_or_refreshes_identity(self):
        for package in setup.PACKAGES:
            (self.root / package / "dist").mkdir(parents=True)
        self.credential()
        def snapshot():
            return {str(p.relative_to(self.base)): (p.stat().st_mode, p.read_bytes())
                    for p in self.base.rglob("*") if p.is_file()}
        before = snapshot()
        with mock.patch.object(setup, "executable", return_value="/fixture/tool"), \
             mock.patch.object(setup, "uds", side_effect=setup.SetupError("not running")), \
             mock.patch.object(setup.CredentialBridge, "refresh", side_effect=AssertionError("must not mint")), \
             mock.patch.object(setup, "run", side_effect=AssertionError("must not run process")), \
             mock.patch.object(setup, "http_json", side_effect=AssertionError("no saved session token")):
            self.assertEqual(setup.check(self.settings, "wren"), 1)
        self.assertEqual(snapshot(), before)
        self.assertFalse(self.settings.operator.exists())
        self.assertFalse(self.settings.config.exists())

    def test_install_builds_but_never_implicitly_restarts(self):
        calls = []
        def process(argv, **kwargs):
            calls.append((argv, kwargs))
            return "v22.0.0" if len(argv) == 2 and argv[1] == "--version" else None
        with mock.patch.object(setup.sys, "platform", "darwin"), \
             mock.patch.object(setup, "executable", return_value="/fixture/tool"), \
             mock.patch.object(setup, "run", side_effect=process), \
             mock.patch.object(setup, "service_plists", return_value={}):
            setup.install(self.settings, self.args("install", "--yes"))
        self.assertFalse(any(argv[0] == "launchctl" for argv, _ in calls))
        builds = [(argv, kwargs) for argv, kwargs in calls if argv[0] in ("bash", "npm")]
        self.assertTrue(builds)
        self.assertTrue(all(kwargs["env"]["CHORUS_BIN_NO_KICKSTART"] == "1" for _, kwargs in builds))
        self.assertTrue(all(kwargs["env"]["CHORUS_ROOT"] == str(self.settings.root) for _, kwargs in builds))
        self.assertTrue((self.settings.bin / "chorus-agent-setup").exists())
        self.assertEqual(self.settings.profiles()["roles"], {})

    def test_strict_install_preserves_custom_plist_fields_and_requires_restart_flag(self):
        directory = self.home / "Library/LaunchAgents"
        directory.mkdir(parents=True)
        path = directory / "com.chorus.mcp.plist"
        original = {"Label": "com.chorus.mcp", "ProgramArguments": ["/deployed/node", "/deployed/mcp.js"],
                    "KeepAlive": {"SuccessfulExit": False}, "EnvironmentVariables": {"CUSTOM": "preserved"}}
        path.write_bytes(plistlib.dumps(original))
        path.chmod(0o644)
        calls = []
        def process(argv, **kwargs):
            calls.append(argv)
            return "v22.0.0" if len(argv) == 2 and argv[1] == "--version" else None
        with mock.patch.object(setup.sys, "platform", "darwin"), \
             mock.patch.object(setup, "executable", return_value="/fixture/tool"), \
             mock.patch.object(setup, "run", side_effect=process):
            setup.install(self.settings, self.args("install", "--strict-mcp", "--yes"))
        updated = plistlib.loads(path.read_bytes())
        self.assertEqual(updated["KeepAlive"], original["KeepAlive"])
        self.assertEqual(updated["ProgramArguments"], original["ProgramArguments"])
        self.assertEqual(updated["EnvironmentVariables"], {"CUSTOM": "preserved", "CHORUS_MCP_IDENTITY_MODE": "strict"})
        self.assertEqual(plistlib.loads(path.with_suffix(".plist.agent-backup").read_bytes()), original)
        self.assertEqual(path.stat().st_mode & 0o777, 0o644)
        self.assertFalse(any(argv[0] == "launchctl" for argv in calls))

    def test_unattended_install_refuses_without_yes(self):
        with mock.patch.object(setup.sys, "platform", "darwin"), \
             mock.patch.object(setup.sys.stdin, "isatty", return_value=False), \
             mock.patch.object(setup, "executable", return_value="/fixture/tool"), \
             mock.patch.object(setup, "run", return_value="v22.0.0") as process, \
             mock.patch.object(setup, "service_plists", return_value={}):
            with self.assertRaises(setup.SetupError):
                setup.install(self.settings, self.args("install"))
        self.assertEqual(process.call_count, 1)  # read-only Node version probe
        self.assertFalse(self.settings.state.exists())


    def test_legacy_handoff_is_persisted_before_default_change(self):
        self.config()
        result = {"session_id": "legacy-transfer", "profile": "opencode-new"}
        calls = []
        def endpoint(socket_path, route, body=None):
            calls.append((route, body))
            if route == "/v1/sessions":
                return result
            self.assertEqual(self.settings.profiles()["roles"]["wren"], "existing-claude")
            return {"ok": True}
        text = "Card 42\nWorktree: /fixture/𓃠\nOpen obligations: review\n"
        with mock.patch.object(setup, "uds", side_effect=endpoint):
            setup.switch_session(self.settings, "wren", {"profile": "opencode-new", "workspace": "/fixture"},
                                 self.base / "token", handoff=text)
        self.assertEqual(calls[1], ("/v1/sessions/legacy-transfer/context", {"text": text}))
        self.assertEqual(len(calls), 2)
        self.assertEqual(self.settings.profiles()["roles"]["wren"], "opencode-new")

    def test_legacy_context_failure_does_not_repeat_admission_or_change_default(self):
        self.config()
        before = self.settings.config.read_bytes()
        with mock.patch.object(setup, "uds", side_effect=[{"session_id": "created-legacy", "profile": "new"},
                                                        setup.SetupError("connection lost")]) as api:
            with self.assertRaisesRegex(setup.SetupError, "created-legacy.*handoff was not stored"):
                setup.switch_session(self.settings, "wren", {"profile": "new", "workspace": "/fixture"},
                                     self.base / "token", handoff="Do not lose this obligation")
        self.assertEqual(api.call_count, 2)
        self.assertEqual(self.settings.config.read_bytes(), before)

    def test_main_reports_opencode_error_without_traceback(self):
        with mock.patch.object(setup, "Settings", return_value=self.settings), \
             mock.patch.object(setup, "setup_profile", side_effect=agent_opencode.OpenCodeError("OpenCode version mismatch")):
            self.assertEqual(setup.main(["setup", "wren"]), 1)
        self.assertIn("chorus-agent-setup: OpenCode version mismatch", self.output.getvalue())
        self.assertNotIn("Traceback", self.output.getvalue())

    def test_install_strict_restart_bootstraps_modified_mcp_and_only_restarts_known_services(self):
        directory = self.home / "Library/LaunchAgents"
        directory.mkdir(parents=True)
        for label, command in [("com.chorus.mcp", "/old/chorus-mcp-wrapper.sh"),
                               ("com.chorus.api", "/old/chorus-api-wrapper.sh"),
                               ("unrelated.service", "/fixture/service")]:
            (directory / (label + ".plist")).write_bytes(plistlib.dumps({"Label": label, "ProgramArguments": ["/bin/bash", command]}))
        calls = []
        def process(argv, **kwargs):
            calls.append([str(x) for x in argv])
            return "v22.0.0" if len(argv) == 2 and argv[1] == "--version" else None
        with mock.patch.object(setup.sys, "platform", "darwin"), \
             mock.patch.object(setup, "executable", return_value="/fixture/node"), \
             mock.patch.object(setup, "run", side_effect=process), \
             mock.patch.object(setup.subprocess, "run", return_value=mock.Mock(returncode=0)) as loaded:
            setup.install(self.settings, self.args("install", "--strict-mcp", "--restart-services", "--yes"))
        domain = f"gui/{os.getuid()}"
        service_calls = [argv for argv in calls if argv[0] == "launchctl"]
        self.assertIn(["launchctl", "bootout", domain + "/com.chorus.mcp"], service_calls)
        self.assertIn(["launchctl", "bootstrap", domain, str(directory / "com.chorus.mcp.plist")], service_calls)
        self.assertIn(["launchctl", "bootout", domain + "/com.chorus.api"], service_calls)
        self.assertIn(["launchctl", "bootstrap", domain, str(directory / "com.chorus.api.plist")], service_calls)
        self.assertEqual(len(service_calls), 4)
        self.assertEqual(loaded.call_count, 2)
        self.assertTrue(all(call.args[0][:2] == ["launchctl", "print"] for call in loaded.call_args_list))
        updated = plistlib.loads((directory / "com.chorus.api.plist").read_bytes())
        self.assertEqual(updated["EnvironmentVariables"]["CHORUS_NODE_BIN"], "/fixture/node")
        self.assertEqual(updated["ProgramArguments"], ["/bin/bash", str(self.settings.root / "platform/scripts/chorus-api-wrapper.sh")])
        self.assertLess(service_calls.index(["launchctl", "bootout", domain + "/com.chorus.mcp"]),
                        service_calls.index(["launchctl", "bootstrap", domain, str(directory / "com.chorus.mcp.plist")]))
        self.assertFalse(any("unrelated.service" in " ".join(argv) for argv in service_calls))

    def test_install_unknown_activation_command_refuses_before_builds(self):
        directory = self.home / "Library/LaunchAgents"
        directory.mkdir(parents=True)
        path = directory / "com.chorus.api.plist"
        original = plistlib.dumps({"Label": "com.chorus.api", "ProgramArguments": ["/custom/company-api-launcher"]})
        path.write_bytes(original)
        with mock.patch.object(setup.sys, "platform", "darwin"), \
             mock.patch.object(setup, "executable", return_value="/fixture/node"), \
             mock.patch.object(setup, "run", return_value="v22.0.0") as process:
            with self.assertRaisesRegex(ValueError, "com.chorus.api.*unrecognized"):
                setup.install(self.settings, self.args("install", "--restart-services", "--yes"))
        self.assertEqual(process.call_count, 1)
        self.assertEqual(path.read_bytes(), original)
        self.assertFalse(self.settings.state.exists())

    def test_launch_finally_disconnects_exact_session_and_closes_owned_server_on_chat_failure(self):
        item = {"profile": "opencode-new", "workspace": str(self.root / "roles/wren"), "executable": "/fixture/opencode",
                "version": "2.0.0", "bundle": str(self.base / "bundle"), "key_env": "EXAMPLE_PROVIDER_KEY"}
        self.config(profiles={"opencode-new": {"endpoint": "http://127.0.0.1:40001"}})
        setup.atomic_json(self.settings.operator, {"version": 1, "deployments": {"wren": {"opencode": item}}})
        token = self.base / "token"
        setup.atomic_bytes(token, b"fixture-role-token")
        bridge = argparse.Namespace(path=token, error=None)
        session = {"session_id": "owned-session", "profile": "opencode-new", "state": "idle"}
        calls = []
        def endpoint(socket_path, route, body=None):
            calls.append((route, body))
            if route == "/v1/sessions":
                return {"sessions": []} if body is None else session
            if route == "/v1/sessions/owned-session":
                return session
            return {"ok": True}
        owner = mock.MagicMock()
        with mock.patch.object(setup, "ensure_supervisor"), \
             mock.patch.object(setup, "uds", side_effect=endpoint), \
             mock.patch.object(setup, "CredentialBridge") as refresh, \
             mock.patch.object(setup, "check_services", return_value={"principal": "wren-id", "role": "wren"}), \
             mock.patch.object(agent_opencode, "start_server", return_value=owner) as start, \
             mock.patch.object(setup, "http_json", return_value=(200, session)), \
             mock.patch.object(setup, "chat", side_effect=setup.SetupError("chat unavailable")):
            refresh.return_value.__enter__.return_value = bridge
            with self.assertRaisesRegex(setup.SetupError, "chat unavailable"):
                setup.launch(self.settings, self.args("switch", "wren", "--legacy-stopped"))
        self.assertEqual(start.call_count, 1)
        owner.__exit__.assert_called_once()
        self.assertEqual(sum(route == "/v1/sessions/owned-session/disconnect" for route, _ in calls), 1)
        self.assertFalse(any(route.endswith("/stop") for route, _ in calls))
        self.assertEqual(self.settings.profiles()["roles"]["wren"], "opencode-new")


    def session(self, state="idle"):
        return {"session_id": "chat-fixture", "role": "wren", "runtime": "opencode", "model": "provider/model",
                "state": state, "enforcement": "trusted", "capabilities": {"gaps": []}, "last_event_sequence": 0}

    def test_chat_receives_idle_pulse_events_without_typed_input(self):
        session = self.session()
        calls = []
        def endpoint(socket_path, route, body=None):
            calls.append((route, body))
            if "/events?" in route:
                return {"events": [{"type": "message.delta", "data": {"text": "Peer work completed ✓"}}] if "after=0" in route else [],
                        "next_cursor": 1, "has_more": False}
            return session
        stdin = io.StringIO("/quit\n")
        with mock.patch.object(setup, "uds", side_effect=endpoint), \
             mock.patch.object(setup.sys, "stdin", stdin), \
             mock.patch.object(setup.select, "select", side_effect=[([], [], []), ([stdin], [], [])]) as waiting:
            setup.chat(self.settings, session, argparse.Namespace(error=None))
        self.assertEqual(waiting.call_count, 2)
        self.assertIn("Peer work completed ✓", self.output.getvalue())
        self.assertFalse(any(route.endswith("/send") for route, _ in calls))
        self.assertTrue(any(route.endswith("after=1") for route, _ in calls))

    def test_chat_paste_preserves_unicode_whitespace_newlines_and_literal_slash(self):
        session = self.session()
        sent = []
        def endpoint(socket_path, route, body=None):
            if "/events?" in route:
                return {"events": [], "next_cursor": 0, "has_more": False}
            if route.endswith("/send"):
                sent.append(body)
                return {"status": "context_delivered", "persisted": True}
            return session
        stdin = io.StringIO("/paste\n/tmp/𓃠 file\n  second line  \n\n/end\n/quit\n")
        with mock.patch.object(setup, "uds", side_effect=endpoint), \
             mock.patch.object(setup.sys, "stdin", stdin), \
             mock.patch.object(setup.select, "select", return_value=([stdin], [], [])):
            setup.chat(self.settings, session, argparse.Namespace(error=None))
        self.assertEqual(len(sent), 1)
        self.assertEqual(sent[0]["input"], "/tmp/𓃠 file\n  second line  \n")
        self.assertEqual(sent[0]["kind"], "human_input")

    def test_chat_accepts_approval_while_turn_is_waiting(self):
        session = self.session("awaiting_approval")
        approvals = []
        human = self.base / "human.token"
        setup.atomic_bytes(human, b"fixture-human-token")
        def endpoint(socket_path, route, body=None):
            if "/events?" in route:
                return {"events": [{"type": "approval.required", "data": {"request_id": "permission-1"}}] if "after=0" in route else [],
                        "next_cursor": 1, "has_more": False}
            if route.endswith("/approve"):
                approvals.append(body)
                session["state"] = "idle"
                return {"accepted": True}
            return session
        stdin = io.StringIO("/approve permission-1 once\n/quit\n")
        with mock.patch.object(setup, "uds", side_effect=endpoint), \
             mock.patch.object(setup.sys, "stdin", stdin), \
             mock.patch.object(setup.select, "select", return_value=([stdin], [], [])):
            setup.chat(self.settings, session, argparse.Namespace(error=None), human)
        self.assertEqual(approvals, [{"credential_file": str(human.absolute()), "request_id": "permission-1", "decision": "once"}])
        self.assertIn("Approval needed", self.output.getvalue())
        self.assertNotIn("fixture-human-token", self.output.getvalue())

    def test_chat_never_retries_uncertain_input(self):
        session = self.session()
        sends = []
        def endpoint(socket_path, route, body=None):
            if "/events?" in route:
                return {"events": [], "next_cursor": 0, "has_more": False}
            if route.endswith("/send"):
                sends.append(body)
                raise setup.SetupError("delivery outcome unknown")
            return session
        stdin = io.StringIO("change files\n")
        with mock.patch.object(setup, "uds", side_effect=endpoint), \
             mock.patch.object(setup.sys, "stdin", stdin), \
             mock.patch.object(setup.select, "select", return_value=([stdin], [], [])):
            with self.assertRaisesRegex(setup.SetupError, "outcome unknown"):
                setup.chat(self.settings, session, argparse.Namespace(error=None))
        self.assertEqual(len(sends), 1)

    def test_chat_busy_input_is_not_queued_and_ctrl_c_can_cancel(self):
        session = self.session("running")
        calls = []
        def endpoint(socket_path, route, body=None):
            calls.append((route, body))
            if "/events?" in route:
                return {"events": [], "next_cursor": 0, "has_more": False}
            return session
        stdin = io.StringIO("do more work\n/quit\n")
        with mock.patch.object(setup, "uds", side_effect=endpoint), \
             mock.patch.object(setup.sys, "stdin", stdin), \
             mock.patch.object(setup.select, "select", side_effect=[([stdin], [], []), KeyboardInterrupt(), ([stdin], [], [])]):
            setup.chat(self.settings, session, argparse.Namespace(error=None))
        self.assertFalse(any(route.endswith("/send") for route, _ in calls))
        self.assertEqual(sum(route.endswith("/cancel") for route, _ in calls), 1)
        self.assertIn("input was not queued", self.output.getvalue())

    def test_service_readiness_uses_authenticated_nonexistent_pulse_recipient(self):
        token = self.base / "session.token"
        setup.atomic_bytes(token, b"fixture-role-token")
        identity = {"principal": "identity:wren", "role": "wren", "scopes": []}
        calls = []
        def endpoint(url, method="GET", body=None, token=None, **kwargs):
            calls.append((url, method, body, token, kwargs))
            if url.endswith("/identity/verify"):
                return 200, identity
            if url.endswith("/agent-sessions"):
                return 200, {"version": 1, "sessions": []}
            if url == self.settings.mcp:
                return 401, {"error": "missing credential"}
            return 409, {"error": "session-not-available"}
        hook = mock.MagicMock()
        hook.getresponse.return_value.status = 200
        with mock.patch.dict(os.environ, {"CHORUS_PULSE_SECRET": "fixture-shared-secret"}), \
             mock.patch.object(setup, "http_json", side_effect=endpoint), \
             mock.patch.object(setup, "UnixConnection", return_value=hook):
            self.assertEqual(setup.check_services(self.settings, token), identity)
        pulse = calls[-1]
        self.assertTrue(pulse[0].endswith("/api/agent-inbox/claim"))
        self.assertEqual(pulse[2]["role"], "wren")
        self.assertTrue(pulse[2]["session_id"].startswith("readiness-"))
        self.assertEqual(pulse[2]["limit"], 1)
        self.assertEqual(pulse[4]["extra_headers"], {"X-Chorus-Pulse-Secret": "fixture-shared-secret"})
        hook.request.assert_called_once_with("GET", "/health")
        hook.close.assert_called_once()
        self.assertNotIn("fixture-shared-secret", self.output.getvalue())

    def test_service_readiness_refuses_nonstrict_mcp_and_names_pulse_failure(self):
        token = self.base / "session.token"
        setup.atomic_bytes(token, b"fixture-role-token")
        good_identity = (200, {"principal": "identity:wren", "role": "wren", "scopes": []})
        inventory = (200, {"version": 1, "sessions": []})
        for status in (200, 403, 405):
            with self.subTest(status=status), \
                 mock.patch.object(setup, "http_json", side_effect=[good_identity, inventory, (status, {})]), \
                 mock.patch.object(setup, "UnixConnection") as hooks:
                with self.assertRaisesRegex(setup.SetupError, "MCP.*strict"):
                    setup.check_services(self.settings, token)
                hooks.assert_not_called()
        hook = mock.MagicMock()
        hook.getresponse.return_value.status = 200
        with mock.patch.dict(os.environ, {"CHORUS_PULSE_SECRET": "fixture-shared-secret"}), \
             mock.patch.object(setup, "http_json", side_effect=[good_identity, inventory, (401, {}), (404, {})]), \
             mock.patch.object(setup, "UnixConnection", return_value=hook):
            with self.assertRaisesRegex(setup.SetupError, "Pulse.*supervisor"):
                setup.check_services(self.settings, token)


    def test_admitted_session_is_disconnected_when_default_persistence_fails(self):
        item = {"role": "wren", "profile": "new", "workspace": str(self.root / "roles/wren"),
                "executable": "/fixture/opencode", "version": "2.0.0", "bundle": "/fixture/bundle",
                "key_env": "EXAMPLE_PROVIDER_KEY"}
        self.config(profiles={"new": {"endpoint": "http://127.0.0.1:40001"}})
        setup.atomic_json(self.settings.operator, {"version": 1, "deployments": {"wren": {"opencode": item}}})
        token = self.base / "token"
        setup.atomic_bytes(token, b"fixture-role-token")
        session = {"session_id": "admitted-but-unsaved", "profile": "new", "state": "idle"}
        def endpoint(socket_path, route, body=None):
            return {"sessions": []} if route == "/v1/sessions" else session
        with mock.patch.object(setup, "ensure_supervisor"), \
             mock.patch.object(setup, "uds", side_effect=endpoint) as api, \
             mock.patch.object(setup, "CredentialBridge") as refresh, \
             mock.patch.object(setup, "check_services", return_value={"role": "wren"}), \
             mock.patch.object(agent_opencode, "start_server") as server, \
             mock.patch.object(setup, "switch_session", side_effect=setup.AdmissionError("Default could not be saved", session)) as admit, \
             mock.patch.object(setup, "chat") as chat:
            refresh.return_value.__enter__.return_value = argparse.Namespace(path=token, error=None)
            with self.assertRaisesRegex(setup.AdmissionError, "Default could not be saved"):
                setup.launch(self.settings, self.args("switch", "wren", "--legacy-stopped"))
            server.return_value.__exit__.assert_called_once()
            chat.assert_not_called()
            admit.assert_called_once()
            self.assertIn(mock.call(self.settings.socket, "/v1/sessions/admitted-but-unsaved/disconnect", {}), api.call_args_list)
        self.assertEqual(self.settings.profiles()["roles"]["wren"], "existing-claude")

    def test_check_uses_configured_opencode_path_and_does_not_start_supervisor(self):
        for package in setup.PACKAGES:
            (self.root / package / "dist").mkdir(parents=True)
        self.credential()
        item = {"role": "wren", "profile": "custom", "executable": "/installed/opencode2", "version": "0.0.0-beta.123",
                "key_env": "EXAMPLE_PROVIDER_KEY"}
        setup.atomic_json(self.settings.operator, {"version": 1, "deployments": {"wren": {"opencode": item}}})
        with mock.patch.object(setup, "executable", return_value="/fixture/tool") as find, \
             mock.patch.object(agent_opencode, "probe_version", return_value="0.0.0-beta.123") as version, \
             mock.patch.object(setup, "uds", side_effect=AssertionError("must not start or contact absent supervisor")):
            self.assertEqual(setup.check(self.settings, "wren"), 0)
        version.assert_called_once_with(Path("/installed/opencode2"), "0.0.0-beta.123")
        self.assertFalse(any(str(call.args[0]) == "opencode" for call in find.call_args_list))
        self.assertIn("Supervisor will be started by switch", self.output.getvalue())


    def test_allow_tool_permissions_requires_operator_confirmation_and_only_changes_new_profile(self):
        before = self.config()
        args = self.args("setup", "wren", "--model", "provider/model", "--key-env", "EXAMPLE_PROVIDER_KEY",
                         "--version", "2.0.0", "--tool-permissions", "allow")
        rendered = []
        def bundle(root, candidate, name, role, directory, shim, node):
            rendered.append(json.loads(candidate.read_text())["profiles"][name])
            directory.mkdir(mode=0o700)
            return {"adapter_config": {"password_file": str(directory / "password")}}
        port = mock.MagicMock()
        port.__enter__.return_value.getsockname.return_value = ("127.0.0.1", 40001)
        with mock.patch.object(setup, "executable", return_value="/fixture/tool"), \
             mock.patch.object(agent_opencode, "probe_version", return_value="2.0.0"), \
             mock.patch.object(setup.socket, "socket", return_value=port), \
             mock.patch.object(agent_opencode, "prepare_bundle", side_effect=bundle), \
             mock.patch.object(setup.sys.stdin, "isatty", return_value=False):
            with self.assertRaisesRegex(setup.SetupError, "confirmation"):
                setup.setup_profile(self.settings, args)
            self.assertEqual(self.settings.profiles(), before)
            self.assertEqual(rendered, [])
            args.trust_runtime = True
            setup.setup_profile(self.settings, args)
        self.assertEqual(len(rendered), 1)
        expected = [{"action": "*", "resource": "*", "effect": "allow"}]
        self.assertEqual(rendered[0]["adapter_config"]["permissions"], expected)
        self.assertEqual(rendered[0]["enforcement"], "trusted")
        self.assertEqual(rendered[0]["approved_gaps"], setup.GAPS)
        after = self.settings.profiles()
        item = self.settings.metadata()["deployments"]["wren"]["opencode"]
        self.assertEqual(after["profiles"][item["profile"]]["adapter_config"]["permissions"], expected)
        self.assertEqual(after["roles"], before["roles"])
        self.assertEqual(after["profiles"]["existing-claude"], before["profiles"]["existing-claude"])
        self.assertNotIn("not-a-real-provider-key", self.settings.config.read_text())

    def test_private_directory_preserves_existing_modes_and_refuses_writable_parents(self):
        for mode in (0o700, 0o755):
            with self.subTest(mode=oct(mode)):
                path = self.base / ("existing-" + oct(mode))
                path.mkdir(mode=mode)
                path.chmod(mode)
                setup.private_dir(path)
                setup.atomic_bytes(path / "operator.json", b"{}")
                self.assertEqual(path.stat().st_mode & 0o777, mode)
                self.assertEqual((path / "operator.json").stat().st_mode & 0o777, 0o600)
        writable = self.base / "group-writable"
        writable.mkdir()
        writable.chmod(0o775)
        with self.assertRaisesRegex(setup.SetupError, "writable by other accounts"):
            setup.private_dir(writable)
        self.assertEqual(writable.stat().st_mode & 0o777, 0o775)
        fresh = self.base / "new-private"
        setup.private_dir(fresh)
        self.assertEqual(fresh.stat().st_mode & 0o777, 0o700)


def subprocess_module():
    return setup.subprocess


if __name__ == "__main__":
    unittest.main()
