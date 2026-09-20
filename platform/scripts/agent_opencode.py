"""Private OpenCode V2 deployment bundles and owned foreground servers.

No work is done at import. All paths are supplied by the operator frontend.
Protocol references: https://opencode.ai/v2/docs/{cli/commands,cli/web,config,instructions}
Source contract: anomalyco/opencode v2 packages/{cli/src/env.ts,util/src/global-roots.ts}.
"""
import base64
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import secrets
import signal
import shutil
import socket
import stat
import subprocess
import time
from urllib.error import HTTPError, URLError
from urllib.parse import urlencode, urlsplit
from urllib.request import build_opener, HTTPRedirectHandler, ProxyHandler, Request


class OpenCodeError(RuntimeError):
    pass


def _private_file(path):
    """Read a credential without following a final symlink or leaking errors/data."""
    try:
        fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
        with os.fdopen(fd, "rb") as stream:
            info = os.fstat(stream.fileno())
            if not stat.S_ISREG(info.st_mode) or info.st_uid != os.getuid() or info.st_mode & 0o077:
                raise ValueError()
            data = stream.read(4097)
        value = data.decode().strip()
        if not value or len(data) > 4096 or any(ord(c) < 32 for c in value):
            raise ValueError()
        return value
    except (OSError, ValueError, UnicodeError):
        raise OpenCodeError("OpenCode password file must be a private regular file owned by this operator") from None


def _write_private(path, data):
    with os.fdopen(os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600), "w") as stream:
        stream.write(data)


def prepare_bundle(root, profiles_path, profile_name, role, output, shim, node):
    """Render fresh instructions and activate them in a private XDG config tree.

    The returned adapter_config can be merged into the selected Chorus profile.
    Existing bundles and personal/project OpenCode configuration are untouched.
    """
    root, output = Path(root).resolve(), Path(output).absolute()
    if output.exists() or output.is_symlink():
        raise OpenCodeError("OpenCode bundle already exists; create a new revision")
    spec = importlib.util.spec_from_file_location("chorus_agent_config", Path(__file__).with_name("agent-config.py"))
    renderer = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(renderer)
    provider = renderer.endpoint_provider(Path(profiles_path), profile_name, "opencode")
    output.mkdir(mode=0o700)
    try:
        rendered = renderer.render(root, "opencode", role, output / "rendered", Path(shim), str(node), provider)
        config_dir = output / "config/opencode"
        config_dir.mkdir(parents=True, mode=0o700)
        config = json.loads((rendered / "opencode.json").read_text())
        # V2 ignores `instructions`; global AGENTS.md is the supported source.
        config["update"] = "disable"
        _write_private(config_dir / "opencode.json", json.dumps(config, indent=2) + "\n")
        shutil.copyfile(rendered / "AGENTS.md", config_dir / "AGENTS.md")
        # Direct plugin files are the documented discovery shape and do not
        # depend on an inferred package.json entrypoint for a directory.
        (config_dir / "plugins").mkdir(mode=0o700)
        shutil.copyfile(rendered / ".opencode/plugins/chorus/index.js", config_dir / "plugins/chorus.js")
        skills = rendered / ".opencode/skills"
        if skills.exists():
            shutil.copytree(skills, config_dir / "skills")
        for name in ("data", "cache", "state", "run"):
            (output / name).mkdir(mode=0o700)
        password_file = output / "run/server.password"
        _write_private(password_file, secrets.token_urlsafe(48) + "\n")
        source = json.loads((rendered / "bundle.json").read_text())
        result = {"version": 1, "bundle": str(output), "config_dir": str(config_dir),
                  "password_file": str(password_file),
                  "adapter_config": {"username": "opencode", "password_file": str(password_file)},
                  "profile": profile_name, "role": role,
                  "source_hashes": source["source_hashes"],
                  "activation_hashes": {str(path.relative_to(output)): hashlib.sha256(path.read_bytes()).hexdigest()
                                        for path in sorted(config_dir.rglob("*")) if path.is_file()},
                  "enforcement": "trusted", "conformance_verified": False}
        _write_private(output / "deployment.json", json.dumps(result, indent=2) + "\n")
        return result
    except BaseException:
        # This directory was exclusively created above; never remove existing
        # operator configuration as rollback.
        shutil.rmtree(output)
        raise


def _bundle(bundle):
    bundle = Path(bundle).absolute()
    info = bundle.lstat()
    if not stat.S_ISDIR(info.st_mode) or info.st_uid != os.getuid() or info.st_mode & 0o077:
        raise OpenCodeError("OpenCode bundle must be an operator-owned private directory")
    manifest = json.loads((bundle / "deployment.json").read_text())
    if manifest.get("bundle") != str(bundle) or manifest.get("version") != 1:
        raise OpenCodeError("OpenCode deployment manifest does not match the selected bundle")
    for name, expected in manifest["activation_hashes"].items():
        path = bundle / name
        if not path.resolve().is_relative_to(bundle.resolve()) or path.is_symlink():
            raise OpenCodeError("OpenCode configuration contains an unexpected symlink")
        if hashlib.sha256(path.read_bytes()).hexdigest() != expected:
            raise OpenCodeError("OpenCode generated configuration changed; configure a new bundle revision")
    return manifest


def server_env(bundle, extra_env=None):
    """Return process-only settings, including a secret; never serialize this dict."""
    bundle = Path(bundle).absolute()
    manifest = _bundle(bundle)
    env = {key: value for key, value in (os.environ if extra_env is None else extra_env).items()
           if (not key.startswith("OPENCODE_") or key == "OPENCODE_API_KEY") and key != "CHORUS_SESSION_ID"}
    env.update({f"XDG_{name}_HOME": str(bundle / name.lower()) for name in ("CONFIG", "DATA", "CACHE", "STATE")})
    env.update({"OPENCODE_CONFIG_DIR": manifest["config_dir"],
                "OPENCODE_DB": str(bundle / "data/opencode/chorus.db"),
                "OPENCODE_PASSWORD": _private_file(manifest["password_file"])})
    # Keep project AGENTS.md guidance. V2 merges project configuration; readiness
    # refuses conflicting MCP/model settings instead of claiming OS isolation.
    return env


def _endpoint(endpoint):
    value = urlsplit(endpoint)
    if (value.scheme != "http" or value.hostname != "127.0.0.1" or value.username or value.password
            or value.path not in ("", "/") or value.query or value.fragment or not value.port):
        raise OpenCodeError("Dedicated OpenCode endpoint must be http://127.0.0.1:<port>")
    return value.port


def probe_version(executable, expected_version=None, environment=None):
    executable = Path(executable)
    if not executable.is_absolute():
        raise OpenCodeError("OpenCode executable must be an absolute path")
    try:
        result = subprocess.run([str(executable), "--version"], env=environment, text=True,
                                stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, timeout=10, check=True)
    except (OSError, subprocess.SubprocessError):
        raise OpenCodeError("Unable to run the selected OpenCode executable with --version") from None
    version = result.stdout.strip()
    # V2 beta releases use 0.0.0-beta.*; a numeric major is not an API contract.
    if not re.fullmatch(r"[0-9]+\.[0-9]+\.[0-9]+(?:[-+][A-Za-z0-9._-]+)?", version):
        raise OpenCodeError("OpenCode returned an unsupported version string")
    if expected_version is not None and version != expected_version:
        raise OpenCodeError("Installed OpenCode version differs from the configured exact version pin")
    return version


class _NoRedirect(HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        raise OpenCodeError("OpenCode readiness endpoint must not redirect")


class OwnedServer:
    """Only signals the Popen child owned by this invocation, never a saved PID."""
    def __init__(self, child, executable, endpoint, bundle, environment, cwd, log):
        self.child, self.executable, self.endpoint = child, str(executable), endpoint.rstrip("/")
        self.bundle, self.environment, self.cwd, self.log = Path(bundle), environment, str(cwd), log
        self.info = None
        self.opener = build_opener(ProxyHandler({}), _NoRedirect())

    def request(self, route, timeout=1):
        password = self.environment["OPENCODE_PASSWORD"]
        credential = base64.b64encode(("opencode:" + password).encode()).decode()
        request = Request(self.endpoint + route, headers={"Authorization": "Basic " + credential})
        try:
            with self.opener.open(request, timeout=timeout) as response:
                data = response.read(1024 * 1024 + 1)
            if len(data) > 1024 * 1024:
                raise OpenCodeError("OpenCode readiness response exceeds its size limit")
            return json.loads(data)
        except HTTPError as error:
            raise OpenCodeError(f"OpenCode readiness returned HTTP {error.code}") from None
        except (URLError, TimeoutError, OSError, ValueError):
            raise OpenCodeError("OpenCode readiness endpoint is unavailable or invalid") from None

    def verify_configuration(self):
        raw = (self.bundle / "config/opencode/opencode.json").read_text()
        # OpenCode substitutes env references before returning discovery entries.
        # Escape as JSON string content; credential values are never printed.
        expected = json.loads(re.sub(r"\{env:([A-Za-z_][A-Za-z0-9_]*)\}",
                                     lambda match: json.dumps(self.environment.get(match[1], ""))[1:-1], raw))
        entries = self.request("/api/config?" + urlencode({"location": json.dumps({"directory": self.cwd})}), timeout=3)
        if not isinstance(entries, list):
            raise OpenCodeError("OpenCode did not return its V2 configuration discovery entries")
        source = str(self.bundle / "config/opencode/opencode.json")
        if not any(entry.get("type") == "document" and entry.get("path") == source for entry in entries):
            raise OpenCodeError("OpenCode did not load the private Chorus configuration")
        merged_model, merged_mcp = None, None
        for entry in entries:
            config = entry.get("info", {})
            if "model" in config:
                merged_model = config["model"]
            if "chorus-api" in config.get("mcp", {}).get("servers", {}):
                merged_mcp = config["mcp"]["servers"]["chorus-api"]
        if merged_mcp != expected["mcp"]["servers"]["chorus-api"] or merged_model != expected.get("model"):
            raise OpenCodeError("Project OpenCode configuration overrides the Chorus MCP or selected model")
        expected_plugin = str(self.bundle / "config/opencode/plugins/chorus.js")
        deadline = time.monotonic() + 3
        while True:
            plugins = self.request("/api/plugin?" + urlencode({"location": json.dumps({"directory": self.cwd})}), timeout=1)
            candidates = plugins.get("data", []) if isinstance(plugins, dict) else []
            own = [plugin for plugin in candidates if plugin.get("source", {}).get("type") == "local"
                   and plugin.get("source", {}).get("path") == expected_plugin]
            if any(plugin.get("id") == "chorus" and plugin.get("state", {}).get("status") == "active" for plugin in own):
                return True
            if any(plugin.get("state", {}).get("status") == "failed" for plugin in own) or time.monotonic() >= deadline:
                raise OpenCodeError("OpenCode did not activate the generated Chorus policy plugin; inspect server.log")
            # A cold location activates plugins asynchronously. Do not enroll until
            # its own module is active, but never claim that this proves coverage.
            time.sleep(0.05)

    def status(self):
        code = self.child.poll()
        return {"state": "running" if code is None else "stopped", "pid": self.child.pid,
                "endpoint": self.endpoint, "exit_code": code,
                "runtime_version": self.info.get("version") if self.info else None}

    def attach(self, native_session_id):
        if not re.fullmatch(r"ses[A-Za-z0-9_-]+", native_session_id or ""):
            raise OpenCodeError("An explicit OpenCode native session ID is required")
        if self.child.poll() is not None:
            raise OpenCodeError("The owned OpenCode server has stopped")
        return subprocess.call([self.executable, self.cwd, "--server", self.endpoint, "--session", native_session_id],
                               cwd=self.cwd, env=self.environment)

    def close(self):
        try:
            if self.child.poll() is None:
                # The foreground server owns its new process group. Signal tools
                # there while its unreaped leader still proves ownership. Never
                # signal a stored PID or a group whose leader we already reaped.
                try:
                    group_owned = os.getpgid(self.child.pid) == self.child.pid
                    if group_owned:
                        os.killpg(self.child.pid, signal.SIGTERM)
                    else:
                        self.child.terminate()
                except ProcessLookupError:
                    group_owned = False
                try:
                    self.child.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    try:
                        if group_owned and os.getpgid(self.child.pid) == self.child.pid:
                            os.killpg(self.child.pid, signal.SIGKILL)
                        else:
                            self.child.kill()
                    except ProcessLookupError:
                        pass
                    self.child.wait(timeout=5)
        finally:
            self.log.close()

    def __enter__(self):
        return self

    def __exit__(self, *_):
        self.close()


def start_server(executable, expected_version, endpoint, bundle, environment, cwd, timeout=15):
    """Start a private server; fail closed on a busy port, wrong pin or config."""
    if not expected_version:
        raise OpenCodeError("An exact OpenCode version pin is required")
    port = _endpoint(endpoint)
    cwd = Path(cwd).resolve(strict=True)
    env = server_env(bundle, environment)
    probe_version(executable, expected_version, env)
    try:
        with socket.socket() as probe:
            probe.bind(("127.0.0.1", port))
    except OSError:
        raise OpenCodeError("OpenCode port is already occupied; choose another port or close its owner") from None
    log_path = Path(bundle) / "run/server.log"
    log = os.fdopen(os.open(log_path, os.O_WRONLY | os.O_CREAT | os.O_APPEND | os.O_NOFOLLOW, 0o600), "ab")
    try:
        child = subprocess.Popen([str(executable), "serve", "--hostname", "127.0.0.1", "--port", str(port)],
                                 cwd=cwd, env=env, stdin=subprocess.DEVNULL, stdout=log, stderr=log,
                                 start_new_session=True)
    except BaseException:
        log.close()
        raise
    owned = OwnedServer(child, executable, endpoint, bundle, env, cwd, log)
    try:
        until = time.monotonic() + timeout
        while time.monotonic() < until:
            if child.poll() is not None:
                raise OpenCodeError("OpenCode exited during startup; inspect its private server.log")
            try:
                info = owned.request("/api/info")
            except OpenCodeError:
                time.sleep(0.05)
                continue
            if (not isinstance(info, dict) or info.get("version") != expected_version
                    or info.get("pid") != child.pid or not isinstance(info.get("urls"), list)
                    or not isinstance(info.get("paths"), dict)):
                raise OpenCodeError("OpenCode V2 readiness identity does not match the owned server and version pin")
            owned.info = info
            owned.verify_configuration()
            return owned
        raise OpenCodeError("OpenCode startup timed out; inspect its private server.log")
    except BaseException:
        owned.close()
        raise
