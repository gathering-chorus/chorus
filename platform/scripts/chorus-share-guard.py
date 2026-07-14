#!/usr/bin/env python3
"""chorus-share-guard — read-only, path-allowlisted, basic-auth reverse proxy (#3644).

The tunnel-facing guard for sharing pages off home infra. Whatever tunnel
provider fronts it (cloudflared quick tunnel, ngrok, ...), the guarantees live
HERE, on our side of the boundary:

  - GET/HEAD only — every write verb is 405 before it touches the upstream
  - path-prefix allowlist — anything else is 404, so :3030/:3340 and the rest
    of the LAN write plane are unreachable BY CONSTRUCTION, not by trust
  - HTTP Basic auth — credentials are per-share-session, never persisted

Env (set by chorus-share, overridable for tests):
  SHARE_UPSTREAM     upstream base, default http://localhost:3000
  SHARE_ALLOW        comma-separated path prefixes, e.g. "/about,/photos"
  SHARE_AUTH         user:password (required — the guard refuses to start naked)
  SHARE_PORT         listen port, default 8899
"""
import base64
import os
import sys
import urllib.error
import urllib.request
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

UPSTREAM = os.environ.get("SHARE_UPSTREAM", "http://localhost:3000").rstrip("/")
ALLOW = [p.strip() for p in os.environ.get("SHARE_ALLOW", "/").split(",") if p.strip()]
AUTH = os.environ.get("SHARE_AUTH", "")
PORT = int(os.environ.get("SHARE_PORT", "8899"))

if not AUTH or ":" not in AUTH:
    print("chorus-share-guard: SHARE_AUTH=user:password is required — refusing to start unauthenticated", file=sys.stderr)
    sys.exit(2)
EXPECTED = "Basic " + base64.b64encode(AUTH.encode()).decode()

# Silas's #3644 envelope ask: fail-closed on bind misconfiguration — the guard is
# loopback-only by design (the tunnel dials OUT to it; nothing else may reach it).
BIND = os.environ.get("SHARE_BIND", "127.0.0.1")
if BIND not in ("127.0.0.1", "::1", "localhost"):
    print(f"chorus-share-guard: refusing non-loopback bind '{BIND}' — the tunnel is the only sanctioned ingress", file=sys.stderr)
    sys.exit(2)


def path_allowed(path, allow):
    for p in allow:
        if p == "/":
            return True
        if path == p or path.startswith(p.rstrip("/") + "/"):
            return True
    return False


class Guard(BaseHTTPRequestHandler):
    server_version = "chorus-share-guard"

    def _deny(self, code, msg):
        self.send_response(code)
        if code == 401:
            self.send_header("WWW-Authenticate", 'Basic realm="chorus-share"')
        self.send_header("Content-Type", "text/plain")
        self.end_headers()
        self.wfile.write(msg.encode())

    def _handle(self, body_allowed):
        if self.headers.get("Authorization", "") != EXPECTED:
            return self._deny(401, "auth required\n")
        if not path_allowed(self.path.split("?")[0], ALLOW):
            return self._deny(404, "path not shared\n")
        req = urllib.request.Request(UPSTREAM + self.path, method="GET")
        # Deliberately NOT forwarding Accept-Encoding: upstream must send identity
        # bytes, because this guard re-emits the body without Content-Encoding.
        # (Forwarding it made Caddy gzip and browsers rendered binary mojibake.)
        for h in ("Accept", "If-None-Match", "If-Modified-Since"):
            if self.headers.get(h):
                req.add_header(h, self.headers[h])
        try:
            with urllib.request.urlopen(req, timeout=30) as r:
                data = r.read()
                self.send_response(r.status)
                for h in ("Content-Type", "Cache-Control", "ETag", "Last-Modified"):
                    if r.headers.get(h):
                        self.send_header(h, r.headers[h])
                self.send_header("Content-Length", str(len(data)))
                self.end_headers()
                if body_allowed:
                    self.wfile.write(data)
        except urllib.error.HTTPError as e:
            self._deny(e.code, f"upstream {e.code}\n")
        except Exception:
            self._deny(502, "upstream unreachable\n")

    def do_GET(self):
        self._handle(body_allowed=True)

    def do_HEAD(self):
        self._handle(body_allowed=False)

    # every write verb: refused before any upstream contact
    def _refuse_write(self):
        self._deny(405, "read-only share — write verbs are refused at the guard\n")

    do_POST = _refuse_write
    do_PUT = _refuse_write
    do_DELETE = _refuse_write
    do_PATCH = _refuse_write
    do_OPTIONS = _refuse_write

    # Per-request audit line on OUR side (Silas #3644: audit trail at the guard,
    # not just the provider's dashboard) — guard.log is the session record.
    def log_message(self, fmt, *args):
        authed = "authed" if self.headers.get("Authorization", "") == EXPECTED else "anon"
        print(f"[guard] {self.address_string()} {authed} {fmt % args}", file=sys.stderr)


if __name__ == "__main__":
    print(f"chorus-share-guard: {BIND}:{PORT} -> {UPSTREAM}  allow={ALLOW} (GET/HEAD only, basic-auth on)", file=sys.stderr)
    ThreadingHTTPServer((BIND if BIND != "localhost" else "127.0.0.1", PORT), Guard).serve_forever()
