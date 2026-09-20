"""Hermetic renderer acceptance tests: no native runtime or personal config."""
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

SOURCE = Path(__file__).resolve().parents[1] / "agent-config.py"
SPEC = importlib.util.spec_from_file_location("agent_config", SOURCE)
CONFIG = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CONFIG)


class AgentConfigTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name) / "repo"
        directory = self.root / "designing/claudemd"
        directory.mkdir(parents=True)
        (directory / "PROTOCOL_VERSION").write_text("1.6.0\n")
        (directory / "role.md").write_text("# {{ROLE_NAME}}\nProtocol {{CHORUS_PROMPT_VERSION}}\n")
        (directory / "manifest.json").write_text(json.dumps({"variables": {"wren": {"ROLE_NAME": "Wren"}},
            "roles": {"wren": {"sections": ["role.md"]}}}))

    def tearDown(self):
        self.temp.cleanup()

    def render(self, runtime):
        out = Path(self.temp.name) / runtime
        return CONFIG.render(self.root, runtime, "wren", out, Path("/installed path/shim"), "/runtime/node")

    def test_all_runtimes_render_from_fragments_without_secret_values(self):
        for runtime in CONFIG.RUNTIMES:
            out = self.render(runtime)
            bundle = json.loads((out / "bundle.json").read_text())
            self.assertFalse(bundle["installed"])
            self.assertIn("# Wren\nProtocol 1.6.0", (out / bundle["instruction_file"]).read_text())
            self.assertNotIn("verified", bundle["capabilities"].values())
            self.assertIn("role.md", bundle["source_hashes"])

    def test_custom_files_are_never_overwritten(self):
        out = self.render("codex")
        (out / "AGENTS.md").write_text("my custom instruction")
        with self.assertRaises(ValueError):
            self.render("codex")
        self.assertEqual((out / "AGENTS.md").read_text(), "my custom instruction")

    def test_gemini_timeout_units_and_fail_closed_plugin(self):
        out = self.render("gemini")
        config = json.loads((out / ".gemini/settings.json").read_text())
        hook = config["hooks"]["BeforeTool"][0]["hooks"][0]
        self.assertEqual(hook["timeout"], 30000)
        self.assertIn("'/installed path/shim'", hook["command"])
        self.assertEqual(config["mcpServers"]["chorus-api"]["env"]["CHORUS_SESSION_TOKEN_FILE"], "${CHORUS_SESSION_TOKEN_FILE}")
        plugin = (self.render("opencode") / ".opencode/plugins/chorus/index.js").read_text()
        self.assertIn('answer.decision !== "allow"', plugin)
        self.assertIn('throw new Error("Chorus hook did not return a decision")', plugin)

    def test_unresolved_fragments_fail_before_writing(self):
        (self.root / "designing/claudemd/role.md").write_text("{{UNKNOWN_VARIABLE}}")
        with self.assertRaises(ValueError):
            self.render("gemini")
        self.assertFalse((Path(self.temp.name) / "gemini").exists())

    def test_skills_project_logical_tools_and_report_nonportable_features(self):
        skill = self.root / "skills/example"
        skill.mkdir(parents=True)
        original = "---\nname: example\ndescription: Example\n---\nUse mcp__chorus-api__chorus_werk({}). Then TaskStop.\n"
        (skill / "SKILL.md").write_text(original)
        (skill / "helper.sh").write_text("#!/bin/sh\nexit 0\n")
        (skill / "helper.sh").chmod(0o755)
        out = self.render("codex")
        projected = out / ".agents/skills/example/SKILL.md"
        body = projected.read_text()
        self.assertTrue(body.startswith("---\nname: example"))
        self.assertIn("Use chorus_werk({})", body)
        self.assertIn("native-delegation", body)
        self.assertIn("Then TaskStop.", body)
        manifest = json.loads((out / "bundle.json").read_text())
        self.assertIn("skills/example/SKILL.md", manifest["source_hashes"])
        self.assertIn(".agents/skills/example/SKILL.md", manifest["generated_files"])
        self.assertEqual(manifest["skills"][0]["logical_mcp_tools"], ["chorus_werk"])
        self.assertEqual((skill / "SKILL.md").read_text(), original)
        self.assertTrue((out / ".agents/skills/example/helper.sh").stat().st_mode & 0o100)

    def test_oversized_instruction_fails_before_output_creation(self):
        (self.root / "designing/claudemd/role.md").write_text("x" * CONFIG.MAX_FILE_BYTES)
        with self.assertRaisesRegex(ValueError, "byte budget"):
            self.render("codex")
        self.assertFalse((Path(self.temp.name) / "codex").exists())

    def test_openai_compatible_endpoint_uses_runtime_alias_and_environment_reference(self):
        profiles = Path(self.temp.name) / "profiles.json"
        document = {"version":1,"profiles":{"endpoint":{"runtime":"opencode","model":"chorus-endpoint/coder",
                    "provider":{"protocol":"openai-chat","base_url":"https://models.example/v1",
                                "api_key_env":"MODELS_API_KEY","model_id":"vendor/actual-model"}}}}
        profiles.write_text(json.dumps(document))
        provider = CONFIG.endpoint_provider(profiles,"endpoint","opencode")
        out = CONFIG.render(self.root,"opencode","wren",Path(self.temp.name)/"endpoint",Path("/shim"),"node",provider)
        config = json.loads((out/"opencode.json").read_text())
        self.assertEqual(config["model"],"chorus-endpoint/coder")
        mcp_env = config["mcp"]["servers"]["chorus-api"]["environment"]
        self.assertEqual(mcp_env["CHORUS_AGENT_BINDING_PROFILE"],"endpoint")
        self.assertNotIn("CHORUS_SESSION_ID", mcp_env)
        binding = config["providers"]["chorus-endpoint"]
        self.assertEqual(binding["env"],["MODELS_API_KEY"])
        self.assertEqual(binding["models"]["coder"]["modelID"],"vendor/actual-model")
        self.assertEqual(binding["package"],"@opencode/ai/providers/openai-compatible")
        document["profiles"]["endpoint"]["provider"]["protocol"]="openai-responses"
        profiles.write_text(json.dumps(document))
        self.assertEqual(CONFIG.endpoint_provider(profiles,"endpoint","opencode")["config"]["package"],"@opencode/ai/providers/responses")
        document["profiles"]["endpoint"]["provider"]["api_key"]="must-not-embed"
        profiles.write_text(json.dumps(document))
        with self.assertRaises(ValueError):
            CONFIG.endpoint_provider(profiles,"endpoint","opencode")

    def test_codex_hooks_and_opencode_v2_schema(self):
        codex = self.render("codex")
        hooks = json.loads((codex / ".codex/hooks.json").read_text())["hooks"]
        self.assertIn("PreToolUse", hooks)
        self.assertEqual(hooks["PreToolUse"][0]["hooks"][0]["timeout"], 30)
        opencode = self.render("opencode")
        config = json.loads((opencode / "opencode.json").read_text())
        server = config["mcp"]["servers"]["chorus-api"]
        self.assertFalse(server["disabled"])
        self.assertNotIn("enabled", server)
        plugin = (opencode / ".opencode/plugins/chorus/index.js").read_text()
        self.assertIn('Plugin.define', plugin)
        self.assertIn('ctx.tool.hook("execute.before"', plugin)
        self.assertIn('"/v1/native-binding"', plugin)
        self.assertNotIn('ctx.location.directory', plugin)

    def test_plugin_resolves_identity_and_fails_closed(self):
        node = os.environ.get("CHORUS_TEST_NODE") or shutil.which("node")
        if not node:
            self.skipTest("Node required for generated OpenCode plugin conformance fixture")
        out = self.render("opencode")
        package = out / "node_modules/@opencode/plugin"
        package.mkdir(parents=True)
        (package / "package.json").write_text('{"type":"module","exports":"./index.js"}')
        (package / "index.js").write_text('export const Plugin = { define: value => value };')
        (out / "package.json").write_text('{"type":"module"}')
        # The fixture controls every external boundary: SDK registration, UDS,
        # and policy process. No installed runtime or live daemon is contacted.
        shim = out / "policy.py"
        shim.write_text('''#!/usr/bin/env python3
import json, os, sys
payload = json.load(sys.stdin)
assert os.environ['CHORUS_ROLE'] == 'wren'
assert os.environ['CHORUS_SESSION_ID'] == 'enrolled-id'
assert payload['cwd'] == os.environ['EXPECTED_CWD']
if payload.get('tool_name') == 'malformed':
    print('{}')
else:
    print(json.dumps({'decision':'deny' if payload.get('tool_name') == 'deny' else 'allow', 'reason':'policy refused', 'hookSpecificOutput':{'additionalContext':'delivered context'}}))
''')
        shim.chmod(0o700)
        plugin = out / ".opencode/plugins/chorus/index.js"
        plugin.write_text(CONFIG.opencode_plugin(shim))
        harness = out / "harness.mjs"
        harness.write_text('''import assert from "node:assert/strict";
import { createServer } from "node:http";
import plugin from "./.opencode/plugins/chorus/index.js";
const hooks = new Map();
const requests = [];
const server = createServer((request,response) => {
  let body = "";
  request.on("data", chunk => body += chunk);
  request.on("end", () => {
    const input = JSON.parse(body); requests.push(input);
    assert.equal(request.url, "/v1/native-binding");
    response.writeHead(input.native_session_id === "unregistered" ? 404 : 200);
    response.end(JSON.stringify({session_id:"enrolled-id",role:"wren",cwd:process.env.EXPECTED_CWD}));
  });
});
await new Promise(resolve => server.listen(process.env.CHORUS_AGENT_SOCKET,resolve));
try {
 await plugin.setup({tool:{hook:async(name,fn)=>hooks.set(name,fn)},session:{hook:async(name,fn)=>hooks.set(name,fn)}});
 const event = {sessionID:"native-id",id:"call",tool:"read",input:{path:"example"}};
 await hooks.get("execute.before")(event);
 await assert.rejects(hooks.get("execute.before")({...event,tool:"deny"}), /policy refused/);
 await assert.rejects(hooks.get("execute.before")({...event,tool:"malformed"}), /did not return a decision/);
 await assert.rejects(hooks.get("execute.before")({...event,sessionID:"unregistered"}), /not registered/);
 await assert.rejects(hooks.get("execute.before")({...event,sessionID:undefined}), /native session identity/);
 const prompt = {sessionID:"native-id",prompt:{text:"hello"}};
 await hooks.get("prompt")(prompt);
 assert.match(prompt.prompt.text,/delivered context/);
 assert.ok(requests.every(request=>request.runtime==="opencode"));
} finally { await new Promise(resolve=>server.close(resolve)); }
''')
        env = {**os.environ, "CHORUS_AGENT_SOCKET": str(out / "fixture.sock"),
               "EXPECTED_CWD": str(out), "CHORUS_ROLE": "wrong-process-role",
               "CHORUS_SESSION_ID": "wrong-process-session"}
        result = subprocess.run([node, str(harness)], env=env, capture_output=True, text=True, timeout=30)
        self.assertEqual(result.returncode, 0, result.stderr)


if __name__ == "__main__":
    unittest.main()
