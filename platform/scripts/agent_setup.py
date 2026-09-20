"""Guided operator setup for an EXISTING Chorus deployment, with no import side effects.

All local paths/credentials are supplied by the deployment account. Development
tests replace subprocesses and HTTP with fixtures; never install on a contributor
machine to exercise this code.
"""
import argparse
from contextlib import contextmanager
import fcntl
import getpass
import http.client
import json
import os
from pathlib import Path
import plistlib
import re
import select
import shlex
import shutil
import socket
import stat
import subprocess
import sys
import tempfile
import threading
import time
import uuid
from urllib.parse import urlsplit
from agent_opencode import OpenCodeError

ROLES = ("wren", "silas", "kade")
GAPS = ["Tool telemetry is projected snapshots, not a complete durable event stream",
        "Pre-tool interception requires a separately verified plugin",
        "Session permissions do not provide filesystem isolation",
        "Version compatibility is experimental; pin expected_version"]
PACKAGES = ("directing/clearing", "platform/mcp-server", "platform/agent-adapters",
            "platform/api", "platform/pulse")
MAX_BYTES = 4 * 1024 * 1024


class SetupError(Exception):
    pass


class AdmissionError(SetupError):
    def __init__(self, message, session):
        super().__init__(message)
        self.session = session


def private_dir(path):
    path = Path(path)
    if path.is_symlink():
        raise SetupError(f"Refusing symlink directory: {path}")
    path.mkdir(parents=True, exist_ok=True, mode=0o700)
    if path.stat().st_uid != os.getuid():
        raise SetupError(f"Directory belongs to a different account: {path}")
    if path.stat().st_mode & 0o022:
        raise SetupError(f"Operator directory must not be writable by other accounts: {path}")
    # Never chmod an existing parent such as a custom config directory or HOME.
    return path


def private_file(path):
    path = Path(path)
    try:
        info = path.lstat()
    except FileNotFoundError:
        raise SetupError(f"Missing private file: {path}") from None
    if not stat.S_ISREG(info.st_mode) or info.st_uid != os.getuid() or info.st_mode & 0o077:
        raise SetupError(f"Expected an owner-only regular file (0600): {path}")
    return path


def atomic_bytes(path, data, backup=False):
    path = Path(path)
    private_dir(path.parent)
    if path.exists() or path.is_symlink():
        private_file(path)
        if backup:
            atomic_bytes(path.with_name(path.name + ".bak"), path.read_bytes())
    fd, name = tempfile.mkstemp(prefix=".agent-", dir=path.parent)
    try:
        with os.fdopen(fd, "wb") as stream:
            stream.write(data)
            stream.flush()
            os.fsync(stream.fileno())
        os.replace(name, path)
    finally:
        if os.path.exists(name):
            os.unlink(name)


def atomic_json(path, data, backup=False):
    atomic_bytes(path, (json.dumps(data, indent=2) + "\n").encode(), backup)


def read_json(path, default=None):
    path = Path(path)
    if not path.exists() and not path.is_symlink():
        return default
    private_file(path)
    if path.stat().st_size > MAX_BYTES:
        raise SetupError(f"Configuration too large: {path}")
    return json.loads(path.read_text())


def executable(value):
    found = shutil.which(str(value))
    if not found:
        raise SetupError(f"Required executable unavailable: {value}")
    # Keep rustup/nvm symlink names; resolving argv[0] can change behavior.
    return str(Path(found).absolute())


def run(args, *, env=None, cwd=None, capture=False, timeout=60):
    try:
        result = subprocess.run([str(x) for x in args], env=env, cwd=cwd,
                                capture_output=capture, text=True, timeout=timeout)
    except (OSError, subprocess.TimeoutExpired):
        raise SetupError(f"Could not complete {Path(str(args[0])).name}; check installation and service availability") from None
    if result.returncode:
        # Captured stderr can contain credentials from external services. Never echo it.
        raise SetupError(f"{Path(str(args[0])).name} failed (exit {result.returncode})")
    return result.stdout if capture else None


def http_json(url, method="GET", body=None, token=None, extra_headers=None):
    parsed = urlsplit(url)
    if (parsed.scheme not in ("http", "https") or not parsed.hostname or parsed.username
            or parsed.password or parsed.fragment or parsed.query):
        raise SetupError("Service URL must have no credentials, query, or fragment")
    if parsed.scheme == "http" and parsed.hostname not in ("localhost", "127.0.0.1", "::1"):
        raise SetupError("Service URLs require HTTPS except on loopback")
    cls = http.client.HTTPSConnection if parsed.scheme == "https" else http.client.HTTPConnection
    connection = cls(parsed.hostname, parsed.port, timeout=6)
    headers = {"Content-Type": "application/json"}
    headers.update(extra_headers or {})
    if token:
        headers["Authorization"] = "Bearer " + token
    try:
        connection.request(method, parsed.path or "/", json.dumps(body) if body is not None else None, headers)
        response = connection.getresponse()
        data = response.read(MAX_BYTES + 1)
        if len(data) > MAX_BYTES:
            raise SetupError("Service response exceeds limit")
        # No redirects and no reflection of potentially sensitive service errors.
        try:
            document = json.loads(data)
        except (ValueError, UnicodeError):
            document = None
        return response.status, document
    except (OSError, http.client.HTTPException):
        raise SetupError(f"Service unavailable at {parsed.hostname}:{parsed.port or (443 if parsed.scheme == 'https' else 80)}") from None
    finally:
        connection.close()


class UnixConnection(http.client.HTTPConnection):
    def __init__(self, path):
        super().__init__("localhost", timeout=60)
        self.path = str(path)

    def connect(self):
        self.sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        self.sock.settimeout(self.timeout)
        self.sock.connect(self.path)


def uds(path, route, body=None):
    connection = UnixConnection(path)
    try:
        connection.request("POST" if body is not None else "GET", route,
                           json.dumps(body) if body is not None else None,
                           {"Content-Type": "application/json"})
        response = connection.getresponse()
        raw = response.read(MAX_BYTES + 1)
        if len(raw) > MAX_BYTES:
            raise SetupError("Supervisor response exceeds limit")
        document = json.loads(raw)
        if response.status != 200:
            message = document.get("error", {}).get("message", "Request refused")
            raise SetupError(f"Supervisor: {message}")
        return document
    except (OSError, http.client.HTTPException, ValueError):
        raise SetupError("Supervisor unavailable or incompatible. Run install, then check.") from None
    finally:
        connection.close()


class Settings:
    def __init__(self, root=None, state=None):
        self.root = Path(root or Path(__file__).resolve().parents[2]).resolve()
        self.state = Path(state or os.environ.get("CHORUS_AGENT_STATE_DIR", Path.home() / ".chorus")).absolute()
        self.config = Path(os.environ.get("CHORUS_AGENT_CONFIG", self.state / "agent-profiles.json")).absolute()
        self.operator = self.state / "agent-setup.json"
        self.socket = Path(os.environ.get("CHORUS_AGENT_SOCKET", self.state / "run/chorus-agent.sock"))
        self.bin = self.state / "bin"
        self.api = os.environ.get("CHORUS_API_URL", "http://127.0.0.1:3340").rstrip("/")
        self.mcp = os.environ.get("CHORUS_MCP_URL", "http://127.0.0.1:3341/mcp")

    def env(self):
        env = dict(os.environ, CHORUS_ROOT=str(self.root), CHORUS_HOME=str(self.root),
                   CHORUS_AGENT_CONFIG=str(self.config), CHORUS_AGENT_STATE_DIR=str(self.state),
                   CHORUS_AGENT_SOCKET=str(self.socket), CHORUS_API_URL=self.api,
                   CHORUS_MCP_IDENTITY_MODE="strict")
        for name in ("CHORUS_IDENTITY_TOKEN", "CHORUS_SESSION_ID", "CHORUS_ROLE", "DEPLOY_ROLE"):
            env.pop(name, None)
        env["PATH"] = str(self.bin) + os.pathsep + env.get("PATH", "")
        return env

    def profiles(self):
        result = read_json(self.config, {"version": 1, "profiles": {}, "roles": {},
                                       "role_workspaces": {}, "max_concurrent_jobs": 3})
        if result.get("version") != 1:
            raise SetupError("Unsupported profile configuration version")
        return result

    def metadata(self):
        value = read_json(self.operator, {"version": 1, "deployments": {}})
        if value.get("version") != 1:
            raise SetupError("Unsupported setup configuration version")
        return value

    @contextmanager
    def lock(self):
        private_dir(self.state)
        path = self.state / "agent-setup.lock"
        fd = os.open(path, os.O_CREAT | os.O_RDWR | os.O_NOFOLLOW, 0o600)
        try:
            try:
                fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
            except BlockingIOError:
                raise SetupError("Another setup/switch operation is in progress") from None
            yield
        finally:
            os.close(fd)


def prompt(label, default=None, supplied=None):
    if supplied is not None:
        return supplied
    if not sys.stdin.isatty():
        if default is not None:
            return default
        raise SetupError(f"Missing {label}; provide the corresponding command option")
    value = input(label + (f" [{default}]" if default is not None else "") + ": ").strip()
    if not value and default is None:
        raise SetupError(f"{label} is required")
    return value or default


def confirm(message, yes=False):
    if not yes and (not sys.stdin.isatty() or input(message + " [y/N] ").lower().strip() != "y"):
        raise SetupError("No changes applied. Use the explicit confirmation option for unattended operation.")


def credential_source(settings, role):
    directory = Path(os.environ.get("CHORUS_IDENTITY_DIR", Path.home() / ".chorus/identity"))
    credential = private_file(directory / role / "cred.json")
    data = json.loads(credential.read_text())
    bound = data.get("hostAccount")
    if bound and bound != getpass.getuser():
        raise SetupError(f"{role}'s identity is bound to macOS account {bound}; run this command in that account. Do not copy or edit its credentials.")
    return directory


class CredentialBridge:
    """Refresh through the existing issuer; publish an atomic, private session file."""
    def __init__(self, settings, role):
        self.settings, self.role = settings, role
        self.path = settings.state / "agent-credentials" / (role + ".token")
        self.error = None
        self.done = threading.Event()
        self.thread = None

    def refresh(self):
        directory = credential_source(self.settings, self.role)
        env = self.settings.env()
        env["CHORUS_IDENTITY_DIR"] = str(directory)
        value = run(["bash", self.settings.root / "platform/scripts/chorus-identity-token", self.role],
                    env=env, capture=True, timeout=40).strip()
        if not value or len(value) > 16384 or "\n" in value:
            raise SetupError("Identity issuer returned an invalid token; no credential was installed")
        atomic_bytes(self.path, value.encode())

    def __enter__(self):
        self.refresh()
        def maintain():
            while not self.done.wait(45):
                try:
                    self.refresh()
                except Exception:
                    self.error = "Chorus identity renewal failed; resolve issuer/account access before continuing."
                    return
        self.thread = threading.Thread(target=maintain, daemon=True)
        self.thread.start()
        return self

    def __exit__(self, *_):
        self.done.set()
        if self.thread:
            self.thread.join(timeout=45)


def check_services(settings, token_file):
    token = private_file(token_file).read_text().strip()
    status, identity = http_json(settings.api + "/api/chorus/identity/verify", "POST", {}, token)
    if (status != 200 or not isinstance(identity, dict)
            or not isinstance(identity.get("principal"), str) or not identity["principal"]
            or identity.get("role") not in ROLES):
        raise SetupError("Chorus API cannot verify this identity. Deploy the updated API and check CSS/model grants.")
    status, inventory = http_json(settings.api + "/api/chorus/agent-sessions", token=token)
    if (status != 200 or not isinstance(inventory, dict) or inventory.get("version") != 1
            or not isinstance(inventory.get("sessions"), list)):
        raise SetupError("Chorus API cannot reach the session supervisor. Check API deployment and socket/account alignment.")
    status, _ = http_json(settings.mcp)
    if status != 401:
        raise SetupError("Shared MCP is not verified strict (expected HTTP 401 without credentials). Migrate existing clients, then install --strict-mcp --restart-services.")
    hooks = Path(os.environ.get("CHORUS_HOOKS_RUN_DIR", Path.home() / ".chorus/run")) / "chorus-hooks.sock"
    connection = UnixConnection(hooks)
    try:
        connection.request("GET", "/health")
        if connection.getresponse().status != 200:
            raise SetupError("Chorus policy daemon failed its health check")
    except OSError:
        raise SetupError("Chorus policy daemon unavailable; start the updated chorus-hooks service") from None
    finally:
        connection.close()
    pulse = os.environ.get("CHORUS_PULSE_BASE_URL", os.environ.get("CHORUS_PULSE_URL", "http://127.0.0.1:3475")).rstrip("/")
    secret = os.environ.get("CHORUS_PULSE_SECRET")
    if not secret:
        secret = private_file(Path(os.environ.get("CHORUS_PULSE_SECRET_FILE", settings.state / "pulse-nudge.secret"))).read_text().strip()
    # A deliberately nonexistent recipient proves the authenticated route and
    # supervisor wiring without claiming, acknowledging or delivering a message.
    code, result = http_json(pulse + "/api/agent-inbox/claim", "POST",
                            {"role": identity["role"], "session_id": "readiness-" + uuid.uuid4().hex, "limit": 1},
                            extra_headers={"X-Chorus-Pulse-Secret": secret})
    if code != 409 or result != {"error": "session-not-available"}:
        raise SetupError("Pulse cannot verify its authenticated supervisor inbox route. Deploy/restart Pulse and check socket/shared-secret wiring.")
    return identity


def save_config(settings, document):
    previous = settings.config.read_bytes() if settings.config.exists() else None
    atomic_json(settings.config, document, backup=previous is not None)
    if settings.socket.exists():
        try:
            uds(settings.socket, "/v1/config/reload", {})
        except Exception:
            if previous is None:
                settings.config.unlink()
            else:
                atomic_bytes(settings.config, previous)
            raise


def setup_profile(settings, args):
    import agent_opencode
    role = args.role
    node = executable(args.node or "node")
    opencode = executable(args.executable or shutil.which("opencode2") or "opencode")
    executable(settings.bin / "chorus-hook-shim")
    detected = agent_opencode.probe_version(Path(opencode))
    version = prompt("Exact installed OpenCode V2 version", detected, args.version)
    if not version or any(ch.isspace() for ch in version):
        raise SetupError("Use the exact OpenCode version string, without surrounding text")
    anchor = Path(prompt("Role state directory", str(settings.root / "roles" / role), args.workspace)).expanduser().resolve()
    if not anchor.is_dir():
        raise SetupError(f"Role state directory does not exist: {anchor}")
    protocol = prompt("Provider (builtin, openai-chat, openai-responses)", "builtin", args.protocol)
    if protocol not in ("builtin", "openai-chat", "openai-responses"):
        raise SetupError("Provider must be builtin, openai-chat, or openai-responses")
    model = prompt("Model (provider/model for builtin; upstream model ID for custom endpoint)", supplied=args.model)
    if len(model) > 200 or any(ord(ch) < 32 for ch in model):
        raise SetupError("Model ID must be at most 200 characters with no control characters")
    if protocol == "builtin":
        print("Use the provider's standard credential variable (for example OPENAI_API_KEY). The private server does not import personal OAuth logins.")
    key = prompt("Provider credential environment variable NAME", supplied=args.key_env)
    if not re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]*", key):
        raise SetupError("Provide an environment variable name, never an API key")
    if key.startswith("CHORUS_") or (key.startswith("OPENCODE_") and key != "OPENCODE_API_KEY"):
        raise SetupError("Provider credentials must not reuse Chorus identity or OpenCode control variable names")
    provider = None
    if protocol == "builtin":
        if "/" not in model or not all(model.split("/", 1)):
            raise SetupError("Built-in OpenCode model must be provider/model")
    else:
        base = prompt("Provider base URL", supplied=args.base_url)
        provider = {"protocol": protocol, "base_url": base, "api_key_env": key, "model_id": model}
    permissions = prompt("OpenCode tool permissions (ask, allow)", "ask", args.tool_permissions)
    if permissions not in ("ask", "allow"):
        raise SetupError("Tool permissions must be ask or allow")
    print("ask: each tool approval requires a verified human token. allow: OpenCode runs tools without its own approval prompt; Chorus policy and MCP authorization still apply.")
    print("This profile uses trusted enrollment with these declared gaps:")
    for gap in GAPS:
        print("  - " + gap)
    confirm(f"Approve these gaps and OpenCode tool permissions={permissions} for this operator-controlled profile?", args.trust_runtime)
    alias = args.name or "opencode"
    if not re.fullmatch(r"[a-z][a-z0-9-]{0,50}", alias):
        raise SetupError("Profile name must use lowercase letters, digits and hyphens")
    with settings.lock():
        config = settings.profiles()
        previous_anchor = config.get("role_workspaces", {}).get(role)
        if previous_anchor and Path(previous_anchor).resolve() != anchor:
            raise SetupError("Existing role anchor differs. Reconcile its workspace binding explicitly before configuring another runtime.")
        name = f"{alias}-{role}-{uuid.uuid4().hex[:8]}"
        if args.port is not None and not 1 <= args.port <= 65535:
            raise SetupError("--port must be between 1 and 65535; omit it to select a free port")
        with socket.socket() as free:
            free.bind(("127.0.0.1", args.port or 0))
            port = free.getsockname()[1]
        directory = private_dir(settings.state / "agents") / name
        profile = {"runtime": "opencode", "mode": "managed", "enforcement": "trusted",
                   "approved_gaps": GAPS, "worker": node,
                   "worker_args": [str(settings.root / "platform/agent-adapters/dist/worker.js"), "--runtime", "opencode"],
                   "endpoint": f"http://127.0.0.1:{port}", "model": "chorus-endpoint/coder" if provider else model,
                   "timeout_secs": 900, "adapter_config": {"expected_version": version, "timeout_ms": 900000}}
        if permissions == "allow":
            profile["adapter_config"]["permissions"] = [{"action": "*", "resource": "*", "effect": "allow"}]
        if provider:
            profile["provider"] = provider
        config["profiles"][name] = profile
        config.setdefault("role_workspaces", {})[role] = str(anchor)
        if args.worktree_base:
            base = Path(args.worktree_base).expanduser().resolve()
            if not base.is_dir():
                raise SetupError("Worktree base must be an existing directory")
            config["worktree_base"] = str(base)
        elif not config.get("worktree_base"):
            base = Path(os.environ.get("CHORUS_WERK_BASE", settings.root.parent / "chorus-werk"))
            if not base.is_dir():
                raise SetupError("Cannot locate worktree base; supply --worktree-base for the existing deployment")
            config["worktree_base"] = str(base.resolve())
        candidate = settings.state / (".profiles-" + uuid.uuid4().hex + ".json")
        atomic_json(candidate, config)
        try:
            bundle = agent_opencode.prepare_bundle(settings.root, candidate, name, role, directory,
                                                    settings.bin / "chorus-hook-shim", Path(node))
            profile["adapter_config"].update(bundle["adapter_config"])
            save_config(settings, config)
        finally:
            candidate.unlink(missing_ok=True)
        metadata = settings.metadata()
        item = {
            "profile": name, "bundle": str(directory), "executable": opencode,
            "key_env": key, "version": version, "workspace": str(anchor), "role": role,
        }
        entries = metadata["deployments"].setdefault(role, {})
        if alias in entries:
            metadata.setdefault("history", {})[entries[alias]["profile"]] = entries[alias]
        entries[alias] = item
        metadata.setdefault("history", {})[name] = item
        atomic_json(settings.operator, metadata, backup=settings.operator.exists())
    print(f"Configured {role}: {alias} -> {model}. No role has been switched.")
    print(f"Next: chorus-agent-setup switch {role} {alias}")


def ensure_supervisor(settings):
    try:
        uds(settings.socket, "/v1/config")
        return
    except SetupError:
        if settings.socket.exists():
            raise SetupError("Supervisor socket exists but is unavailable/incompatible; reconcile it instead of starting another daemon") from None
    if sys.platform != "darwin":
        raise SetupError("Start chorus-agentd with this configuration, then rerun; automatic service startup supports macOS launchd")
    daemon = executable(settings.bin / "chorus-agentd")
    directory = Path.home() / "Library/LaunchAgents"
    directory.mkdir(parents=True, exist_ok=True)
    target = directory / "com.chorus.agent.plist"
    environment = {name: settings.env()[name] for name in
                   ("CHORUS_ROOT", "CHORUS_HOME", "CHORUS_AGENT_CONFIG", "CHORUS_AGENT_STATE_DIR",
                    "CHORUS_AGENT_SOCKET", "CHORUS_API_URL", "PATH")}
    desired = {"Label": "com.chorus.agent", "ProgramArguments": [daemon],
               "EnvironmentVariables": environment, "RunAtLoad": True, "KeepAlive": True,
               "ThrottleInterval": 10, "StandardOutPath": str(settings.state / "agentd.stdout.log"),
               "StandardErrorPath": str(settings.state / "agentd.stderr.log")}
    if target.exists():
        if plistlib.loads(target.read_bytes()) != desired:
            raise SetupError(f"Existing supervisor LaunchAgent differs: {target}. Reconcile its configuration explicitly.")
    else:
        # Do not change the permissions of the shared LaunchAgents directory.
        fd = os.open(target, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600)
        with os.fdopen(fd, "wb") as stream:
            stream.write(plistlib.dumps(desired))
    # Loaded-but-not-running jobs need a kickstart, not a duplicate bootstrap.
    domain = f"gui/{os.getuid()}"
    loaded = subprocess.run(["launchctl", "print", domain + "/com.chorus.agent"],
                            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=10).returncode == 0
    run(["launchctl", "kickstart", domain + "/com.chorus.agent"] if loaded else
        ["launchctl", "bootstrap", domain, target], capture=True)
    for _ in range(50):
        try:
            uds(settings.socket, "/v1/config")
            return
        except SetupError:
            time.sleep(0.1)
    raise SetupError(f"Supervisor did not become ready; inspect {settings.state / 'agentd.stderr.log'}")


def primary(settings, role):
    sessions = uds(settings.socket, "/v1/sessions")["sessions"]
    matches = [s for s in sessions if s["role"] == role and s["primary"] and s["state"] != "stopped"]
    if len(matches) > 1:
        raise SetupError("Multiple primary sessions require operator reconciliation")
    return matches[0] if matches else None


def selection(settings, role, alias=None, session=None):
    metadata = settings.metadata()
    entries = metadata["deployments"].get(role, {})
    if alias:
        if alias not in entries:
            raise SetupError(f"No {alias} setup for {role}. Run setup first.")
        return entries[alias]
    name = session["profile"] if session else settings.profiles()["roles"].get(role)
    for item in entries.values():
        if item["profile"] == name:
            return item
    if name in metadata.get("history", {}):
        item = metadata["history"][name]
        if item.get("role") != role:
            raise SetupError("The selected historical profile belongs to a different role")
        return item
    raise SetupError("Selected session/profile was not created by setup; use its original runtime controls")


def safe_handoff(path):
    if not path:
        raise SetupError("An existing session needs --handoff FILE describing task state, worktree, evidence and open obligations")
    source = Path(path)
    if not source.is_file() or source.stat().st_size > 128 * 1024:
        raise SetupError("Handoff must be a text file no larger than 128 KiB")
    value = source.read_text().strip()
    if not value:
        raise SetupError("Handoff cannot be empty")
    return value


def switch_session(settings, role, item, token_path, old=None, handoff=None, resume=False):
    """Only commit the default after authoritative session admission succeeds."""
    if resume:
        if not old:
            raise SetupError("No primary session to reopen; use switch")
        result = uds(settings.socket, f"/v1/sessions/{old['session_id']}/resume", {})
    elif old:
        result = uds(settings.socket, f"/v1/sessions/{old['session_id']}/switch",
                     {"profile": item["profile"], "context": handoff,
                      "credential_file": str(token_path)})
    else:
        result = uds(settings.socket, "/v1/sessions", {"version": 1, "profile": item["profile"],
                     "role": role, "cwd": item["workspace"], "credential_file": str(token_path), "primary": True})
        if handoff:
            try:
                uds(settings.socket, f"/v1/sessions/{result['session_id']}/context", {"text": handoff})
            except Exception:
                raise AdmissionError(f"Session {result['session_id']} created but handoff was not stored. No prompt was sent; retain the handoff file and reconcile this exact session before continuing.", result) from None
    config = settings.profiles()
    config["roles"][role] = result["profile"]
    try:
        save_config(settings, config)
    except Exception as error:
        # Admission might have mutated business state. Never retry/switch it back.
        raise AdmissionError(f"Session {result['session_id']} was created, but saving its default failed. Do not retry switch; inspect status and reconcile configuration: {error}", result) from None
    return result


def readable_session(session):
    gaps = session.get("capabilities", {}).get("gaps", [])
    return (f"{session['role']}: {session['runtime']} / {session.get('model') or 'runtime default'}\n"
            f"Session: {session['session_id']}  State: {session['state']}  Enforcement: {session['enforcement']}\n"
            + ("Declared gaps: " + "; ".join(gaps) + "\n" if gaps else "")
            + ("Switch blockers: " + "; ".join(session.get("switch_blockers", [])) + "\n"
               if session.get("switch_blockers") else ""))


def chat(settings, session, bridge, human_file=None):
    session_id = session["session_id"]
    route = f"/v1/sessions/{session_id}"
    cursor = session.get("last_event_sequence", 0)
    print(readable_session(session))
    print("Type a message. /paste ends with /end. /status /cancel /approve ID once|reject /quit")
    print("This terminal owns the private OpenCode server. Keep it open for Pulse delivery.")
    multiline = None
    show_prompt = True
    last_state = session["state"]
    while True:
        if bridge.error:
            raise SetupError(bridge.error)
        page = uds(settings.socket, route + f"/events?after={cursor}")
        for event in page["events"]:
            kind, data = event["type"], event.get("data", {})
            if kind == "turn.started":
                print("\nAgent> ", end="", flush=True)
            elif kind in ("message.delta", "assistant.text", "message.replaced"):
                print(data.get("text", ""), end="", flush=True)
            elif kind == "approval.required":
                print("\nApproval needed: " + json.dumps(data, ensure_ascii=False))
                show_prompt = True
            elif kind in ("turn.failed", "turn.completed", "turn.interrupted"):
                print("\n" + ("Turn failed; inspect /status before submitting more work." if kind == "turn.failed" else ""))
                show_prompt = True
        cursor = page["next_cursor"]
        current = uds(settings.socket, route)
        if current["state"] in ("stopped", "disconnected"):
            print("\nSession is " + current["state"] + "; closing this terminal connection.")
            return
        if current["state"] != last_state and current["state"] != "running":
            show_prompt = True
        last_state = current["state"]
        if page.get("has_more"):
            continue
        if show_prompt:
            print("You> " if multiline is None else "... ", end="", flush=True)
            show_prompt = False
        try:
            # Poll even while waiting for typing, so Pulse work, permission
            # requests and provider errors are visible without another prompt.
            readable, _, _ = select.select([sys.stdin], [], [], 0.25)
            if not readable:
                continue
            raw = sys.stdin.readline()
        except KeyboardInterrupt:
            print("\nCancellation requested.")
            uds(settings.socket, route + "/cancel", {})
            show_prompt = True
            continue
        if raw == "":
            return
        line = raw.rstrip("\r\n")
        pasted = False
        show_prompt = True
        if multiline is not None:
            if line != "/end":
                multiline.append(line)
                if sum(len(x.encode()) + 1 for x in multiline) > 128 * 1024:
                    multiline = None
                    print("Message exceeds 128 KiB; discarded without sending.")
                continue
            line, multiline, pasted = "\n".join(multiline), None, True
        elif line == "/paste":
            multiline = []
            print("Enter multiline text; /end on a line of its own sends it.")
            continue
        if not pasted and line == "/quit":
            return
        if not line:
            continue
        if not pasted and line == "/status":
            print(readable_session(current))
            continue
        if not pasted and line == "/cancel":
            try:
                print(uds(settings.socket, route + "/cancel", {}))
            except SetupError as error:
                print(error)
            continue
        if not pasted and line.startswith("/approve "):
            fields = line.split()
            if len(fields) != 3 or fields[2] not in ("once", "reject") or not human_file:
                print("Approval requires --human-token-file for a verified human, then /approve ID once|reject")
                continue
            try:
                private_file(human_file)
                print(uds(settings.socket, route + "/approve", {"credential_file": str(Path(human_file).absolute()),
                          "request_id": fields[1], "decision": fields[2]}))
            except SetupError as error:
                print(error)
            continue
        if not pasted and line.startswith("/"):
            print("Unknown command; use /status, /cancel, /approve, /paste or /quit")
            continue
        if len(line.encode()) > 128 * 1024:
            print("Message exceeds 128 KiB; not sent.")
            continue
        if current["state"] != "idle":
            print("The session is " + current["state"] + "; input was not queued. Wait, approve or /cancel first.")
            continue
        result = uds(settings.socket, route + "/send", {"version": 1, "message_id": "operator:" + uuid.uuid4().hex,
                     "kind": "human_input", "input": line})
        if result.get("persisted") is False or result.get("status") == "queued":
            print("Input was not delivered; inspect /status. It has not been retried.")
            continue


def launch(settings, args, resume=False):
    import agent_opencode
    ensure_supervisor(settings)
    with settings.lock():
        uds(settings.socket, "/v1/config/reload", {})
        old = primary(settings, args.role)
        if getattr(args, "profile", None):
            item = selection(settings, args.role, session={"profile": args.profile})
            if old and old["profile"] != args.profile:
                raise SetupError("The selected default differs from the live session; use switch with an explicit handoff")
            resume = old is not None
        else:
            item = selection(settings, args.role, getattr(args, "name", None), old if resume else None)
        profile = settings.profiles()["profiles"][item["profile"]]
        handoff = None
        if resume and not old:
            raise SetupError("No primary session to reopen; use switch")
        if old and not resume:
            if old.get("switch_blockers"):
                raise SetupError("Cannot switch: " + "; ".join(old["switch_blockers"]))
            handoff = safe_handoff(args.handoff)
        elif not old:
            if getattr(args, "handoff", None):
                handoff = safe_handoff(args.handoff)
            if not getattr(args, "legacy_stopped", False):
                confirm(f"Confirm {args.role}'s old Claude/native session is stopped (the new registry cannot prove this)")
        if not os.environ.get(item["key_env"]):
            raise SetupError(f"Export provider credential {item['key_env']} in this terminal; its value is never saved to profile/config")
    with CredentialBridge(settings, args.role) as bridge:
        identity = check_services(settings, bridge.path)
        if identity.get("role") != args.role:
            raise SetupError("Verified identity does not match the selected role")
        env = settings.env()
        env.update(CHORUS_ROLE=args.role, DEPLOY_ROLE=args.role, CHORUS_SESSION_TOKEN_FILE=str(bridge.path))
        with agent_opencode.start_server(Path(item["executable"]), item["version"], profile["endpoint"],
                                        Path(item["bundle"]), env, Path(old["cwd"] if old else item["workspace"])):
            session = None
            try:
                with settings.lock():
                    session = switch_session(settings, args.role, item, bridge.path, old, handoff, resume)
                token = private_file(bridge.path).read_text().strip()
                code, remote = http_json(settings.api + "/api/chorus/agent-sessions/" + session["session_id"], token=token)
                if code != 200 or not isinstance(remote, dict) or remote.get("session_id") != session["session_id"]:
                    raise SetupError(f"Session {session['session_id']} enrolled, but API cannot resolve it. No prompt sent. Check API/supervisor account alignment, then reopen explicitly.")
                chat(settings, session, bridge, args.human_token_file)
            except AdmissionError as error:
                session = error.session
                raise
            finally:
                # Keep explicit IDs for resume, never release an uncertain lease on EOF.
                if session:
                    try:
                        current = uds(settings.socket, f"/v1/sessions/{session['session_id']}")
                        if current["state"] != "stopped":
                            uds(settings.socket, f"/v1/sessions/{session['session_id']}/disconnect", {})
                    except SetupError as error:
                        print(f"Disconnect needs reconciliation for {session['session_id']}: {error}", file=sys.stderr)
                    print(f"Conversation retained. Reopen with: chorus-agent-setup open {args.role}")


def check(settings, role=None):
    failures = []
    def probe(name, action):
        try:
            result = action()
            print("OK    " + name)
            return result
        except (SetupError, OpenCodeError, OSError, ValueError) as error:
            failures.append(name)
            print("NEEDS " + name + ": " + str(error))
    for name in ("node", "npm", "cargo"):
        probe(name, lambda n=name: executable(n))
    for name in ("chorus-agent", "chorus-agentd", "chorus-hook-shim"):
        probe(name, lambda n=name: executable(settings.bin / n))
    for package in PACKAGES:
        def built(p=package):
            if not (settings.root / p / "dist").is_dir():
                raise SetupError("Run install in the deployment checkout")
        probe(package + " build", built)
    if settings.socket.exists():
        probe("supervisor", lambda: uds(settings.socket, "/v1/config"))
    else:
        print("INFO  Supervisor will be started by switch; no service is started by check.")
    if role:
        probe(role + " credential/account", lambda: credential_source(settings, role))
        item = probe(role + " configured runtime", lambda: selection(settings, role, "opencode"))
        if item:
            from agent_opencode import probe_version
            probe("configured OpenCode version", lambda: probe_version(Path(item["executable"]), item["version"]))
            def has_key():
                if not os.environ.get(item["key_env"]):
                    raise SetupError("Export " + item["key_env"] + " in the switch/open terminal")
            probe("provider credential reference", has_key)
        token = settings.state / "agent-credentials" / (role + ".token")
        if token.exists() and settings.socket.exists():
            probe("identity/API/MCP/policy readiness", lambda: check_services(settings, token))
        else:
            print("INFO  Live identity/service checks run on switch; check does not mint credentials.")
    print("No model request or deployment change was made by check.")
    return 1 if failures else 0


def service_plists():
    found = {}
    directory = Path.home() / "Library/LaunchAgents"
    for path in directory.glob("*.plist"):
        if path.is_symlink():
            continue
        try:
            value = plistlib.loads(path.read_bytes())
        except (ValueError, OSError, plistlib.InvalidFileException):
            continue
        if value.get("Label") in ("com.chorus.api", "com.chorus.hooks", "com.gathering.messaging", "com.chorus.mcp"):
            found[value["Label"]] = (path, value)
    return found


def install(settings, args):
    if sys.platform != "darwin":
        raise SetupError("The signed operator installer targets the existing macOS deployment")
    for binary in ("node", "npm", "cargo", "codesign"):
        executable(binary)
    selected_node = executable("node")
    if int(run([selected_node, "--version"], capture=True).strip().lstrip("v").split(".")[0]) < 22:
        raise SetupError("Node 22+ is required for the runtime worker")
    services = service_plists()
    planned = services
    if args.restart_services:
        from agent_install_services import prepare_services
        # Validate every activation before changing any build artifacts. Native
        # Node modules must run under the same Node version that installed them.
        planned = prepare_services(services, settings.root, selected_node)
    if args.strict_mcp and "com.chorus.mcp" not in services:
        raise SetupError("No com.chorus.mcp LaunchAgent found; configure strict mode in your actual MCP service manager before switching")
    print(f"Deployment checkout: {settings.root}\nInstall directory: {settings.bin}")
    print("Build locked TypeScript packages; build/sign/install supervisor, hooks and launcher.")
    if args.restart_services:
        print("Restart existing services: " + ", ".join(sorted(services)))
        print("Their launch configuration will use this checkout and the selected Node executable.")
    if args.strict_mcp:
        print("Require credentials on shared MCP. Existing tokenless clients must be migrated first.")
    confirm("Apply this deployment installation on this Mac?", args.yes)
    env = settings.env()
    env["PATH"] = str(Path(selected_node).parent) + os.pathsep + env["PATH"]
    env["CHORUS_BIN_NO_KICKSTART"] = "1"
    env.pop("BUILD_SKIP_INSTALL", None)
    # Existing signed installer always targets $HOME/.chorus/bin.
    if settings.bin != Path.home() / ".chorus/bin":
        raise SetupError("Signed installer uses ~/.chorus/bin; use the standard state directory for installation")
    for package in PACKAGES:
        run(["npm", "ci", "--no-audit", "--no-fund"], env=env, cwd=settings.root / package, timeout=1800)
        run(["npm", "run", "build"], env=env, cwd=settings.root / package, timeout=1800)
    script = settings.root / "platform/scripts/build-signed.sh"
    for name in ("chorus-hooks", "chorus-agent"):
        run(["bash", script, name], env=env, cwd=settings.root, timeout=3600)
    run(["bash", script, settings.root / "platform/services/chorus-awake", "com.chorus.awake", "chorus-awake"],
        env=env, cwd=settings.root, timeout=3600)
    for name in ("chorus-agent", "chorus-agentd", "chorus-awake", "chorus-hook-shim", "chorus-hooks"):
        executable(settings.bin / name)
    private_dir(settings.bin)
    target = settings.bin / "chorus-agent-setup"
    wrapper = ("#!/bin/sh\nexec " + shlex.quote(sys.executable) + " " +
               shlex.quote(str(settings.root / "platform/scripts/chorus-agent-setup")) + ' "$@"\n').encode()
    atomic_bytes(target, wrapper, backup=target.exists())
    target.chmod(0o700)
    with settings.lock():
        if not settings.config.exists():
            atomic_json(settings.config, settings.profiles())
    changed_services = dict(planned) if args.restart_services else {}
    if args.strict_mcp:
        path, value = planned["com.chorus.mcp"]
        value.setdefault("EnvironmentVariables", {})["CHORUS_MCP_IDENTITY_MODE"] = "strict"
        changed_services["com.chorus.mcp"] = (path, value)
    for path, value in changed_services.values():
        # Preserve unrelated plist fields, modes and the original backup. No
        # service environment or credential contents are echoed to the terminal.
        if path.is_symlink() or path.stat().st_uid != os.getuid():
            raise SetupError(f"LaunchAgent must be owned by this account: {path}")
        backup = path.with_suffix(".plist.agent-backup")
        if not backup.exists():
            shutil.copy2(path, backup)
        fd, temporary = tempfile.mkstemp(prefix=".agent-plist-", dir=path.parent)
        try:
            with os.fdopen(fd, "wb") as stream:
                stream.write(plistlib.dumps(value))
                stream.flush()
                os.fsync(stream.fileno())
            os.chmod(temporary, path.stat().st_mode & 0o777)
            os.replace(temporary, path)
        finally:
            if os.path.exists(temporary):
                os.unlink(temporary)
    if args.restart_services:
        domain = f"gui/{os.getuid()}"
        for label, (path, _) in changed_services.items():
            loaded = subprocess.run(["launchctl", "print", domain + "/" + label],
                                    stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=10).returncode == 0
            if loaded:
                run(["launchctl", "bootout", domain + "/" + label], capture=True)
            run(["launchctl", "bootstrap", domain, path], capture=True)
    print("Installed. Existing services must run the updated builds before switch can pass readiness checks.")
    if not args.restart_services and services:
        print("Before restarting manually, align service Node versions with this build (native-module ABI), or rerun install --restart-services for managed activation.")
    print("Next: chorus-agent-setup setup wren --runtime opencode")


def parser():
    root = argparse.ArgumentParser(description="Set up and switch agents on an EXISTING Chorus deployment. No action without a subcommand.")
    root.add_argument("--root", help="Deployment checkout (default: this script's repository)")
    root.add_argument("--state-dir", help="Operator state directory (default: ~/.chorus)")
    commands = root.add_subparsers(dest="command", required=True)
    command = commands.add_parser("install", help="Build/sign/install; optionally activate existing services")
    command.add_argument("--restart-services", action="store_true")
    command.add_argument("--strict-mcp", action="store_true")
    command.add_argument("--yes", action="store_true")
    command = commands.add_parser("setup", help="Configure a role with the guided OpenCode wizard")
    command.add_argument("role", choices=ROLES)
    command.add_argument("--runtime", choices=("opencode",), default="opencode")
    for name in ("name", "node", "executable", "version", "workspace", "worktree-base", "model", "key-env", "base-url"):
        command.add_argument("--" + name)
    command.add_argument("--protocol", choices=("builtin", "openai-chat", "openai-responses"))
    command.add_argument("--tool-permissions", choices=("ask", "allow"),
                         help="OpenCode approval behavior; Chorus policy and human-only operations remain enforced")
    command.add_argument("--port", type=int)
    command.add_argument("--trust-runtime", action="store_true", help="Explicit operator approval of the displayed enforcement gaps")
    for verb in ("switch", "open", "wake"):
        command = commands.add_parser(verb, help="Open a managed terminal conversation" if verb == "open" else "Handoff/select a runtime and open a conversation")
        command.add_argument("role", choices=ROLES)
        if verb == "switch":
            command.add_argument("name", nargs="?", default="opencode")
            command.add_argument("--handoff", help="Task/worktree/evidence/obligations text file for an existing session")
            command.add_argument("--legacy-stopped", action="store_true", help="Assert the unenrolled legacy session has already been stopped")
        if verb == "wake":
            command.add_argument("--profile", required=True, help="Exact setup-managed profile selected by chorus-awake")
        command.add_argument("--human-token-file", help="Optional separate verified human credential for permission approvals")
    command = commands.add_parser("check", help="Read-only prerequisites; never mint credentials or call a model")
    command.add_argument("role", nargs="?", choices=ROLES)
    commands.add_parser("status", help="Show primary sessions and selected runtime defaults")
    command = commands.add_parser("stop", help="Release an idle primary; refuses unsettled work")
    command.add_argument("role", choices=ROLES)
    return root


def main(argv=None):
    args = parser().parse_args(argv)
    settings = Settings(args.root, args.state_dir)
    try:
        if args.command == "install":
            install(settings, args)
        elif args.command == "setup":
            setup_profile(settings, args)
        elif args.command in ("switch", "open", "wake"):
            launch(settings, args, resume=args.command == "open")
        elif args.command == "check":
            return check(settings, args.role)
        elif args.command == "status":
            print("Defaults: " + json.dumps(settings.profiles()["roles"]))
            for session in uds(settings.socket, "/v1/sessions")["sessions"]:
                if session["primary"] and session["state"] != "stopped":
                    print(readable_session(session))
        elif args.command == "stop":
            with CredentialBridge(settings, args.role), settings.lock():
                session = primary(settings, args.role)
                if not session:
                    raise SetupError("No enrolled primary session")
                if session.get("switch_blockers"):
                    raise SetupError("Reconcile before stop: " + "; ".join(session["switch_blockers"]))
                print(uds(settings.socket, f"/v1/sessions/{session['session_id']}/stop", {}))
        return 0
    except (SetupError, OpenCodeError, ValueError, OSError) as error:
        print("chorus-agent-setup: " + str(error), file=sys.stderr)
        return 1
    except KeyboardInterrupt:
        print("\nInterrupted. No uncertain operation will be retried; use status before continuing.", file=sys.stderr)
        return 130
