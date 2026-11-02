#!/usr/bin/env python3
"""fake-dagu-server <dir> [code] — #4771 test double for dagu's server.
Answers one POST on 127.0.0.1 (port written to <dir>/port), records
"POST <path> <body>" to <dir>/dagu-called, answers <code> (default 200),
and exits after one request or 30s, so no test ever needs to stop it."""
import http.server, os, socketserver, sys, time

d, code = sys.argv[1], int(sys.argv[2]) if len(sys.argv) > 2 else 200


class H(http.server.BaseHTTPRequestHandler):
    def do_POST(self):
        body = self.rfile.read(int(self.headers.get("Content-Length") or 0)).decode()
        with open(f"{d}/dagu-called", "w") as f:
            f.write(f"POST {self.path} {body}\n")
        self.send_response(code)
        self.end_headers()
        self.wfile.write(b'{"dagRunId":"ok"}' if code == 200 else b'{"message":"refused by fake"}')

    def log_message(self, *a):
        pass


srv = socketserver.TCPServer(("127.0.0.1", 0), H)
srv.timeout = 0.5
with open(f"{d}/port.tmp", "w") as f:
    f.write(str(srv.server_address[1]))
os.rename(f"{d}/port.tmp", f"{d}/port")
end = time.time() + 30
while time.time() < end and not os.path.exists(f"{d}/dagu-called"):
    srv.handle_request()
