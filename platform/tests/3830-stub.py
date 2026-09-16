#!/usr/bin/env python3
"""#3830 test world: one process standing in for the three servers chorus-provision
talks to before it touches the register — the roles/security store (SPARQL), the
write API's discovery document, and the profile-card server. Nothing here is the
register: CSS stays a dead port in every test, so no test can create anything.

  argv[1]  port
  argv[2]  card status for /ghost/profile/card   (401 = row without a pod)
  argv[3]  card status for /whole/profile/card   (200 = a whole user)
  argv[4]  optional lifetime in seconds — the stub exits by itself (no kill needed)
"""
import sys, json
from http.server import BaseHTTPRequestHandler, HTTPServer

port = int(sys.argv[1]); ghost = int(sys.argv[2]); whole = int(sys.argv[3])
if len(sys.argv) > 4:
    import threading, os
    threading.Timer(float(sys.argv[4]), lambda: os._exit(0)).start()
me = f"http://127.0.0.1:{port}"

class H(BaseHTTPRequestHandler):
    def log_message(self, *a): pass
    def send(self, code, body, ctype="application/json"):
        b = body.encode(); self.send_response(code)
        self.send_header("Content-Type", ctype); self.send_header("Content-Length", str(len(b)))
        self.end_headers(); self.wfile.write(b)
    def do_GET(self):
        p = self.path
        if p == "/":
            return self.send(200, json.dumps({"kind": "Discovery", "primitives": [
                {"kind": "Card", "collection": "/v1/cards/cards"},
                {"kind": "Principal", "collection": "/v1/identity/principals"}]}))
        if p.startswith("/pods/query"):
            if "ASK" in p:
                return self.send(200, json.dumps({"head": {}, "boolean": True}))
            rows = [("principal-ghost", f"{me}/ghost/profile/card#me"),
                    ("principal-whole", f"{me}/whole/profile/card#me")]
            return self.send(200, json.dumps({"head": {"vars": ["s", "w"]}, "results": {"bindings": [
                {"s": {"type": "uri", "value": f"https://jeffbridwell.com/chorus#{n}"},
                 "w": {"type": "literal", "value": w}} for n, w in rows]}}))
        if p == "/ghost/profile/card": return self.send(ghost, "", "text/turtle")
        if p == "/whole/profile/card": return self.send(whole, "", "text/turtle")
        self.send(404, "{}")

HTTPServer(("127.0.0.1", port), H).serve_forever()
