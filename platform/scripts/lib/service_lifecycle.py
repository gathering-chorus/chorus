# @domain: services
"""#4446 — a com.chorus.* python service logs its own start, stop and failure.

The python twin of platform/services/shared/service_lifecycle.rs (and the
node and bash ones): same events, same fields, same rule for a kill the
service could not log (its next start asks launchd how the previous run ended).

    from service_lifecycle import service_lifecycle
    lifecycle = service_lifecycle("com.chorus.share-guard")   # name when not under launchd
    lifecycle.started()                                       # once up; installs SIGTERM/SIGINT
    ...                                                       # a crash -> service.failed via excepthook
"""
import hashlib
import os
import signal
import subprocess
import sys


def parse_last_exit(text):
    """{'exit_code': int|None, 'signal': str|None}, or None when launchd has no previous run."""
    last, seen = {"exit_code": None, "signal": None}, False
    for line in str(text).split("\n"):
        if not line.startswith("\t") or line.startswith("\t\t") or " = " not in line:
            continue
        k, v = line[1:].split(" = ", 1)
        if k == "last exit code":
            head = v.split(":")[0].strip()
            # "(never exited)" on a first run is not a number, and not a failure
            last["exit_code"] = int(head) if head.lstrip("-").isdigit() else None
            seen = True
        elif k == "last terminating signal":
            last["signal"], seen = v.strip(), True
    return last if seen else None


def clean(last):
    if last["signal"]:
        return last["signal"].startswith("Terminated")
    return (last["exit_code"] or 0) == 0


def start_events(service, pid, version, previous):
    out = []
    if previous and not clean(previous):
        f = {"service": service, "reason": "previous run ended abnormally"}
        if previous["signal"]:
            f["signal"] = previous["signal"]
        else:
            f["exit_code"] = str(previous["exit_code"])
        out.append(("service.failed", f))
    out.append(("service.started", {"service": service, "pid": str(pid), "version": version}))
    return out


def launchd_label():
    # everything a service starts inherits XPC_SERVICE_NAME: launchd must be the parent
    if os.getppid() != 1:
        return None
    label = os.environ.get("XPC_SERVICE_NAME", "")
    return label if label.startswith("com.chorus.") else None


def previous_run(label):
    try:
        out = subprocess.run(["/bin/launchctl", "print", f"gui/{os.getuid()}/{label}"],
                             capture_output=True, text=True, timeout=5)
        return parse_last_exit(out.stdout)
    except Exception:
        return None


def script_version(path=None):
    try:
        with open(path or sys.argv[0], "rb") as f:
            return hashlib.sha256(f.read()).hexdigest()[:12]
    except Exception:
        return "unknown"


def emit(event, fields):
    home = os.environ.get("CHORUS_HOME") or os.path.join(os.environ.get("HOME", ""), "CascadeProjects/chorus")
    args = ["bash", os.path.join(home, "platform/scripts/chorus-log"), event, "system"]
    args += [f"{k}={v}" for k, v in fields.items()]
    if event == "service.failed":
        args.append("--level=error")
    try:
        subprocess.run(args, capture_output=True, timeout=5)
    except Exception:
        pass  # a log line never takes the service down


class _Lifecycle:
    def __init__(self, name):
        self.service = launchd_label() or name
        self.pid = os.getpid()

    def started(self, version=None):
        label = launchd_label()
        previous = previous_run(label) if label else None
        for event, fields in start_events(self.service, self.pid, version or script_version(), previous):
            emit(event, fields)
        for sig in (signal.SIGTERM, signal.SIGINT):
            signal.signal(sig, self._on_signal)
        hook = sys.excepthook

        def on_crash(kind, value, tb):
            self.failed(f"{kind.__name__}: {value}", 1)
            hook(kind, value, tb)
        sys.excepthook = on_crash

    def _on_signal(self, signo, _frame):
        self.stopped(signal.Signals(signo).name)
        sys.exit(0)

    def stopped(self, reason):
        emit("service.stopped", {"service": self.service, "pid": str(self.pid), "reason": reason})

    def refuse(self, reason, exit_code=2):
        """#4446 round 2 — refuse to start on a bad config: say why on stderr,
        log service.failed with the same reason, and exit. A service that stops
        itself never does so silently."""
        print(reason, file=sys.stderr)
        self.failed(reason, exit_code)
        sys.exit(exit_code)

    def failed(self, reason, exit_code=1):
        emit("service.failed", {"service": self.service, "pid": str(self.pid), "reason": reason,
                                "exit_code": str(exit_code)})


def service_lifecycle(name):
    return _Lifecycle(name)
