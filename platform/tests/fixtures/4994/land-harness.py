#!/usr/bin/env python3
# @domain: logs
# land-harness.py (#4994 AC4) — runs platform/scripts/land-4994-rows --apply against a fake
# launchd (a launchctl that logs each call and whether the new positions exist yet), a fake
# :9080 serving <metrics file>, a fake ps printing <cpu>, stub log-archive and log-rotate
# scripts, and a scratch shared-observability repo. Prints the land's exit code, then the
# calls in order, then what is installed and what is left, so a test reads the whole land.
# A fake Loki answers the land's spine probe with the marker chorus-log wrote, unless the
# mode is "nospine" (the spine stopped reaching Loki).
#   land-harness.py <land script> <scratch dir> <metrics file> <cpu> [rerun|nospine]
import hashlib, os, subprocess, sys, threading, time
from http.server import BaseHTTPRequestHandler, HTTPServer

land, d, metrics, cpu = sys.argv[1:5]
mode = sys.argv[5] if len(sys.argv) > 5 else ""
rerun = mode == "rerun"
body = open(metrics, "rb").read()


class M(BaseHTTPRequestHandler):
    def do_GET(self):
        out = body
        if self.path.startswith("/loki/"):
            m = open(f"{d}/marker").read().strip() if os.path.isfile(f"{d}/marker") else ""
            vals = [] if mode == "nospine" or not m or m not in __import__("urllib.parse").parse.unquote(self.path) else [["1", m]]
            out = __import__("json").dumps({"data": {"result": [{"values": vals}] if vals else []}}).encode()
        self.send_response(200); self.end_headers(); self.wfile.write(out)

    def log_message(self, *a):
        pass


srv = HTTPServer(("127.0.0.1", 0), M)
threading.Thread(target=srv.serve_forever, daemon=True).start()
canon = f"{d}/canon/platform/scripts/launchagents-canonical"
for sub in ("agents", canon[len(d) + 1:], "canon/platform/config/promtail", "old", "state", "bin", "obs/config/promtail"):
    os.makedirs(f"{d}/{sub}", exist_ok=True)
log = f"{d}/calls.log"
open(f"{d}/bin/launchctl", "w").write(f"""#!/bin/sh
echo "launchctl $1 $(basename "$2") [new positions: $(grep -c . {d}/state/positions.yaml 2>/dev/null || echo none)]" >> {log}
case "$1" in
  print) grep -qx "up $2" {d}/up 2>/dev/null && {{ echo "	pid = 4242"; exit 0; }}; exit 113;;
  bootout) grep -vx "up $2" {d}/up > {d}/up.n 2>/dev/null; mv {d}/up.n {d}/up; exit 0;;
  bootstrap) echo "up $2/$(basename "$3" .plist)" >> {d}/up; exit 0;;
esac
""")
open(f"{d}/bin/ps", "w").write(f"#!/bin/sh\necho {cpu}\n")
open(f"{d}/bin/promtail", "w").write("#!/bin/sh\nexit 0\n")
for b in ("launchctl", "ps", "promtail"):
    os.chmod(f"{d}/bin/{b}", 0o755)
for label in ("com.gathering.promtail", "com.chorus.log-archive"):
    open(f"{canon}/{label}.plist", "w").write(f"new {label}\n")
    open(f"{d}/agents/{label}.plist", "w").write(f"old {label}\n")
open(f"{d}/canon/platform/config/promtail/promtail-native.yaml", "w").write("server: {}\n")
open(f"{d}/canon/platform/scripts/log-archive", "w").write(
    f"import sys\nopen('{log}', 'a').write('log-archive ' + ' '.join(sys.argv[1:]) + '\\n')\n")
open(f"{d}/canon/platform/scripts/gathering-log-rotate.sh", "w").write(f"echo log-rotate >> {log}\n")
open(f"{d}/bin/chorus-log", "w").write(f"#!/bin/sh\necho \"chorus-log $1\" >> {log}\nfor a; do case $a in probe=*) echo ${{a#probe=}} > {d}/marker;; esac; done\n")
os.chmod(f"{d}/bin/chorus-log", 0o755)
open(f"{d}/old/positions.yaml", "w").write("positions:\n  a.log: '42'\n")
if rerun:  # an earlier land already moved on: the new positions are newer and hold more
    open(f"{d}/state/positions.yaml", "w").write("positions:\n  a.log: '99'\n  b.log: '7'\n")
    t = time.time()
    os.utime(f"{d}/old/positions.yaml", (t - 600, t - 600))
open(f"{d}/up", "w").write(f"up gui/{os.getuid()}/com.gathering.promtail\n")
for n, b in (("promtail-native.yaml", "old native\n"), ("promtail-config.yaml", "docker\n"), ("promtail-bedroom.yaml", "bedroom\n")):
    open(f"{d}/obs/config/promtail/{n}", "w").write(b)
for c in (["init", "-q"], ["add", "."], ["-c", "user.email=t@t", "-c", "user.name=t", "commit", "-q", "-m", "seed"]):
    subprocess.run(["git", "-C", f"{d}/obs"] + c, check=True, capture_output=True)
env = dict(os.environ, CHORUS_HOME=f"{d}/canon", LAUNCHCTL=f"{d}/bin/launchctl", PS_BIN=f"{d}/bin/ps",
           PROMTAIL_BIN=f"{d}/bin/promtail", LAUNCH_AGENTS_DIR=f"{d}/agents", OLD_POSITIONS=f"{d}/old/positions.yaml",
           NEW_POSITIONS=f"{d}/state/positions.yaml", PROMTAIL_METRICS_URL=f"http://127.0.0.1:{srv.server_port}/metrics",
           OBS_REPO=f"{d}/obs", PROMTAIL_SETTLE_SECS="0", PROMTAIL_CPU_SAMPLES="2", PROMTAIL_SAMPLE_GAP_SECS="0",
           LAUNCHD_RELEASE_WAIT="3", PROMTAIL_READY_WAIT="5", CHORUS_LOG_BIN=f"{d}/bin/chorus-log",
           LOKI_URL=f"http://127.0.0.1:{srv.server_port}", SPINE_WAIT="3",
           GIT_AUTHOR_NAME="t", GIT_AUTHOR_EMAIL="t@t", GIT_COMMITTER_NAME="t", GIT_COMMITTER_EMAIL="t@t",
           OLD_CONFIG_SHA256=hashlib.sha256(b"old native\n").hexdigest())
r = subprocess.run([sys.executable, land, "--apply"], env=env, capture_output=True, text=True)
print(f"exit {r.returncode}")
print(open(log).read().strip())
print("installed:", open(f"{d}/agents/com.gathering.promtail.plist").read().strip())
print("old positions kept:", os.path.isfile(f"{d}/old/positions.yaml"))
print("new positions:", open(f"{d}/state/positions.yaml").read().replace("\n", " ").strip() if os.path.isfile(f"{d}/state/positions.yaml") else "none")
print("obs left:", ",".join(sorted(os.listdir(f"{d}/obs/config/promtail"))))
print("stderr:", r.stderr.strip()[-300:])
