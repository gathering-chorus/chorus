#!/usr/bin/env python3
# @domain: logs
# watch-events.py (#4994 AC2) — runs a scratch promtail (its own folder) tailing one
# file and pushing to a local sink (a dry run never writes positions), with its positions file where the caller says, at log level info, and counts the
# "received file watcher event" lines naming the positions file. 0 = the positions writes
# never wake promtail's own watcher. Prints the count.
#   watch-events.py <promtail> <scratch dir> <positions: "subdir" | "watched">
import subprocess, sys, time, os, threading
from http.server import BaseHTTPRequestHandler, HTTPServer

class Sink(BaseHTTPRequestHandler):  # stands in for Loki: accepts every push
    def do_POST(self):
        self.rfile.read(int(self.headers.get("Content-Length", 0))); self.send_response(204); self.end_headers()
    def log_message(self, *a): pass

srv = HTTPServer(("127.0.0.1", 0), Sink)
threading.Thread(target=srv.serve_forever, daemon=True).start()
promtail, d, where = sys.argv[1:4]
os.makedirs(d + "/logs/state", exist_ok=True)
open(d + "/logs/app.log", "w").write("line\n")
pos = d + ("/logs/state/positions.yaml" if where == "subdir" else "/logs/positions.yaml")
open(d + "/p.yaml", "w").write(f"""server: {{ disable: true, log_level: info }}
positions: {{ filename: {pos}, sync_period: 1s }}
clients: [{{ url: http://127.0.0.1:{srv.server_port}/loki/api/v1/push }}]
scrape_configs:
  - job_name: t
    static_configs: [{{ targets: [localhost], labels: {{ job: t, __path__: {d}/logs/app.log }} }}]
""")
err = open(d + "/err.log", "w")
p = subprocess.Popen([promtail, "-log.level=info", "-config.file", d + "/p.yaml"], stdout=subprocess.DEVNULL, stderr=err)
try:
    time.sleep(8)
    for i in range(5):
        open(d + "/logs/app.log", "a").write(f"more {i}\n"); time.sleep(1)
    time.sleep(4)
finally:
    p.terminate(); p.wait(10)
n = sum(1 for l in open(d + "/err.log") if "received file watcher event" in l and os.path.basename(pos) in l)
print(n)
