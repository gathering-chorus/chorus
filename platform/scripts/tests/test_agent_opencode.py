"""Hermetic operator deployment tests; no OpenCode install or provider calls."""
import importlib.util
import json
import os
from pathlib import Path
import socket
import sys
import tempfile
import subprocess
import time
import unittest
from unittest import mock

SOURCE = Path(__file__).resolve().parents[1] / "agent_opencode.py"
SPEC = importlib.util.spec_from_file_location("agent_opencode", SOURCE)
OPENCODE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(OPENCODE)

FAKE_SERVER = r'''
import base64,json,os,re,subprocess,sys
from http.server import HTTPServer,BaseHTTPRequestHandler
from pathlib import Path
VERSION='0.0.0-beta.123'
if '--version' in sys.argv:
    print(VERSION);sys.exit(0)
if '--session' in sys.argv:
    Path(os.environ['ATTACH_RECORD']).write_text(json.dumps(sys.argv));sys.exit(0)
assert sys.argv[1:4]==['serve','--hostname','127.0.0.1']
if os.environ.get('CHILD_RECORD'):
    child=subprocess.Popen([sys.executable,'-c','import time; time.sleep(60)'])
    Path(os.environ['CHILD_RECORD']).write_text(str(child.pid))
class Server(BaseHTTPRequestHandler):
    def log_message(self,*args):pass
    def do_GET(self):
        expected='Basic '+base64.b64encode(('opencode:'+os.environ['OPENCODE_PASSWORD']).encode()).decode()
        if self.headers.get('Authorization')!=expected:
            self.send_response(401);self.end_headers();return
        if self.path=='/api/info':
            data={'version':VERSION,'pid':os.getpid()+int(os.environ.get('WRONG_PID','0')),'urls':['http://127.0.0.1'],'paths':{'tmp':'/tmp'}}
        elif self.path.startswith('/api/config?'):
            path=Path(os.environ['OPENCODE_CONFIG_DIR'])/'opencode.json'
            raw=re.sub(r'\{env:([A-Za-z_][A-Za-z0-9_]*)\}',lambda match:json.dumps(os.environ.get(match[1],''))[1:-1],path.read_text())
            data=[{'type':'document','path':str(path),'info':json.loads(raw)}]
            if os.environ.get('OVERRIDE_MODEL'):data.append({'type':'document','path':'/other/opencode.json','info':{'model':'wrong/model'}})
        elif self.path.startswith('/api/plugin?'):
            data={'data':[] if os.environ.get('MISSING_PLUGIN') else [{'id':'chorus','state':{'status':'active'},'source':{'type':'local','path':str(Path(os.environ['OPENCODE_CONFIG_DIR'])/'plugins/chorus.js')}}]}
        else:self.send_response(404);self.end_headers();return
        self.send_response(200);self.send_header('Content-Type','application/json');self.end_headers();self.wfile.write(json.dumps(data).encode())
HTTPServer(('127.0.0.1',int(sys.argv[-1])),Server).serve_forever()
'''


class OpenCodeSetupTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.base = Path(self.temp.name).resolve()
        self.root = self.base / 'repository'
        canonical = self.root / 'designing/claudemd'
        canonical.mkdir(parents=True)
        (canonical / 'PROTOCOL_VERSION').write_text('1.6.0\n')
        (canonical / 'role.md').write_text('# {{ROLE_NAME}}\nKeep Chorus worktree rules.\n')
        (canonical / 'manifest.json').write_text(json.dumps({'variables':{'wren':{'ROLE_NAME':'Wren'}},'roles':{'wren':{'sections':['role.md']}}}))
        skill = self.root / 'skills/pull'
        skill.mkdir(parents=True)
        (skill / 'SKILL.md').write_text('Pull a Chorus card.\n')
        self.profiles = self.base / 'profiles.json'
        self.profiles.write_text(json.dumps({'version':1,'profiles':{'wren-open':{'runtime':'opencode','model':'openai/model'}}}))
        self.output = self.base / 'bundle'
        self.executable = self.base / 'fake-opencode'
        self.executable.write_text('#!' + sys.executable + '\n' + FAKE_SERVER)
        self.executable.chmod(0o700)

    def tearDown(self):
        self.temp.cleanup()

    def prepare(self):
        return OPENCODE.prepare_bundle(self.root,self.profiles,'wren-open','wren',self.output,Path('/private/bin/shim'),Path('/private/bin/node'))

    def endpoint(self):
        with socket.socket() as endpoint:
            endpoint.bind(('127.0.0.1',0))
            return 'http://127.0.0.1:' + str(endpoint.getsockname()[1])

    def start(self, **kwargs):
        return OPENCODE.start_server(self.executable,'0.0.0-beta.123',self.endpoint(),self.output,
                                     {'PATH':os.environ['PATH'],**kwargs},self.root,timeout=2)

    def test_bundle_activates_global_instructions_plugin_skills_and_keeps_secrets_out_of_config(self):
        result = self.prepare()
        config = Path(result['config_dir'])
        self.assertIn('# Wren',(config / 'AGENTS.md').read_text())
        self.assertTrue((config / 'plugins/chorus.js').is_file())
        self.assertTrue((config / 'skills/pull/SKILL.md').is_file())
        credential = Path(result['password_file'])
        self.assertEqual(credential.stat().st_mode & 0o777,0o600)
        self.assertNotIn(credential.read_text().strip(),(config / 'opencode.json').read_text())
        self.assertNotIn(credential.read_text().strip(),(self.output / 'deployment.json').read_text())
        self.assertEqual(result['adapter_config']['username'],'opencode')
        env = OPENCODE.server_env(self.output,{'HOME':'/operator','OPENCODE_CONFIG_CONTENT':'evil','OPENCODE_TEST_HOME':'/fake','CHORUS_SESSION_ID':'old','MODEL_KEY':'credential','OPENCODE_API_KEY':'zen-credential'})
        self.assertNotIn('OPENCODE_CONFIG_CONTENT',env)
        self.assertNotIn('OPENCODE_TEST_HOME',env)
        self.assertNotIn('CHORUS_SESSION_ID',env)
        self.assertEqual(env['HOME'],'/operator')
        self.assertEqual(env['MODEL_KEY'],'credential')
        self.assertEqual(env['OPENCODE_API_KEY'],'zen-credential')
        self.assertEqual(env['XDG_CONFIG_HOME'],str(self.output / 'config'))
        self.assertFalse((self.root / 'AGENTS.md').exists())

    def test_existing_bundle_and_custom_config_remain_untouched(self):
        self.output.mkdir()
        (self.output / 'AGENTS.md').write_text('custom')
        with self.assertRaisesRegex(OPENCODE.OpenCodeError,'already exists'):
            self.prepare()
        self.assertEqual((self.output / 'AGENTS.md').read_text(),'custom')

    def test_modified_generated_config_and_nonprivate_credentials_refuse_start(self):
        result = self.prepare()
        password = Path(result['password_file'])
        password.chmod(0o644)
        with self.assertRaisesRegex(OPENCODE.OpenCodeError,'private regular'):
            OPENCODE.server_env(self.output,{})
        password.chmod(0o600)
        (Path(result['config_dir']) / 'AGENTS.md').write_text('tampered')
        with self.assertRaisesRegex(OPENCODE.OpenCodeError,'configuration changed'):
            OPENCODE.server_env(self.output,{})

    def test_owned_server_starts_with_basic_auth_exact_pin_and_closes_only_its_child(self):
        self.prepare()
        with self.start() as server:
            self.assertEqual(server.status()['state'],'running')
            self.assertEqual(server.info['pid'],server.child.pid)
            self.assertTrue(server.verify_configuration())
        self.assertEqual(server.status()['state'],'stopped')
        server.close()  # cleanup is idempotent

    def test_cleanup_signals_the_owned_server_process_group(self):
        self.prepare()
        record = self.base / 'child.pid'
        with self.start(CHILD_RECORD=str(record)):
            child_pid = int(record.read_text())
        for _ in range(50):
            result = subprocess.run(['ps','-o','stat=','-p',str(child_pid)], capture_output=True, text=True)
            if result.returncode != 0 or result.stdout.strip().startswith('Z'):
                break
            time.sleep(0.02)
        else:
            self.fail('Owned OpenCode tool child survived normal server cleanup')

    def test_busy_port_is_never_adopted_or_terminated(self):
        self.prepare()
        with socket.socket() as other:
            other.bind(('127.0.0.1',0));other.listen()
            endpoint='http://127.0.0.1:'+str(other.getsockname()[1])
            with self.assertRaisesRegex(OPENCODE.OpenCodeError,'already occupied'):
                OPENCODE.start_server(self.executable,'0.0.0-beta.123',endpoint,self.output,{},self.root)
            self.assertGreater(other.fileno(),-1)

    def test_wrong_version_pid_or_project_override_refuses_readiness_and_reaps_child(self):
        self.prepare()
        with self.assertRaisesRegex(OPENCODE.OpenCodeError,'version pin'):
            OPENCODE.probe_version(self.executable,'2.0.0',{})
        for overrides, message in [({'WRONG_PID':'1'},'identity'),({'OVERRIDE_MODEL':'1'},'overrides'),({'MISSING_PLUGIN':'1'},'policy plugin')]:
            with mock.patch.object(OPENCODE.OwnedServer,'close',autospec=True,side_effect=OPENCODE.OwnedServer.close) as close:
                with self.assertRaisesRegex(OPENCODE.OpenCodeError,message):
                    self.start(**overrides)
                self.assertEqual(close.call_count,1)
                self.assertIsNotNone(close.call_args.args[0].child.poll())

    def test_attach_uses_explicit_native_session_without_latest_flag(self):
        self.prepare()
        record = self.base / 'attach.json'
        with self.start(ATTACH_RECORD=str(record)) as server:
            self.assertEqual(server.attach('ses_fixture'),0)
            with self.assertRaises(OPENCODE.OpenCodeError):
                server.attach('')
        args = json.loads(record.read_text())
        self.assertIn('--session',args)
        self.assertIn('ses_fixture',args)
        self.assertNotIn('--continue',args)

    def test_remote_endpoints_and_secret_symlinks_are_rejected(self):
        result = self.prepare()
        for endpoint in ['http://0.0.0.0:4096','http://evil.example:4096','http://127.0.0.1:4096/path','http://user:secret@127.0.0.1:4096']:
            with self.assertRaises(OPENCODE.OpenCodeError):
                OPENCODE._endpoint(endpoint)
        credential = Path(result['password_file'])
        replacement = self.base / 'password'
        credential.rename(replacement)
        credential.symlink_to(replacement)
        with self.assertRaises(OPENCODE.OpenCodeError):
            OPENCODE.server_env(self.output,{})


if __name__ == '__main__':
    unittest.main()
