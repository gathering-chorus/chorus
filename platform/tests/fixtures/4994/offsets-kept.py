#!/usr/bin/env python3
# @domain: logs
# offsets-kept.py (#4994 AC2) — moving promtail's positions file must not re-ship history.
# A scratch promtail reads a 5-line log and writes positions (pushing to a local sink); the
# positions file is then copied to a new path (or not, when <mode> is "nocopy"); 2 lines
# are appended; a dry-run promtail on the new path prints what it would ship. Prints the
# number of lines it would ship: 2 = only the new appends, 7 = history re-shipped.
#   offsets-kept.py <promtail> <scratch dir> copy|nocopy
import subprocess, sys, time, os, shutil, threading
from http.server import BaseHTTPRequestHandler, HTTPServer

class Sink(BaseHTTPRequestHandler):  # stands in for Loki: accepts every push
    def do_POST(self):
        self.rfile.read(int(self.headers.get("Content-Length", 0))); self.send_response(204); self.end_headers()
    def log_message(self, *a): pass

promtail, d, mode = sys.argv[1:4]
srv = HTTPServer(("127.0.0.1", 0), Sink)
threading.Thread(target=srv.serve_forever, daemon=True).start()
os.makedirs(d + "/logs", exist_ok=True); os.makedirs(d + "/old", exist_ok=True); os.makedirs(d + "/new", exist_ok=True)
log = d + "/logs/app.log"
open(log, "w").write("".join(f"history {i}\n" for i in range(5)))

def cfg(pos, name):
    p = f"{d}/{name}.yaml"
    open(p, "w").write(f"""server: {{ disable: true }}
positions: {{ filename: {pos}, sync_period: 1s }}
clients: [{{ url: http://127.0.0.1:{srv.server_port}/loki/api/v1/push }}]
scrape_configs:
  - job_name: t
    static_configs: [{{ targets: [localhost], labels: {{ job: t, __path__: {log} }} }}]
""")
    return p

def run(args, secs, out):
    p = subprocess.Popen([promtail] + args, stdout=out, stderr=subprocess.DEVNULL)
    try: time.sleep(secs)
    finally: p.terminate(); p.wait(10)

run(["-config.file", cfg(d + "/old/positions.yaml", "first")], 6, subprocess.DEVNULL)
if mode == "copy":
    shutil.copy2(d + "/old/positions.yaml", d + "/new/positions.yaml")
open(log, "a").write("new 0\nnew 1\n")
with open(d + "/dry.out", "w") as out:
    run(["-dry-run", "-config.file", cfg(d + "/new/positions.yaml", "second")], 6, out)
print(sum(1 for l in open(d + "/dry.out") if "history " in l or "new " in l))
