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


def load_allow():
    """#3744 — the allowlist is a GOVERNED FILE, not an env var captured at launch.

    Why: the live guard ran from 2026-07-22 to 2026-08-06 with SHARE_ALLOW=/about
    frozen in its process environment, started by hand with no LaunchAgent. The
    July /athena entries did not "regress" — they were never persisted anywhere.
    A policy that exists only inside a running process cannot be reviewed, cannot
    survive a restart, and cannot be measured. Now it lives in
    config/share-allowlist.txt, version-controlled and diffable.

    SHARE_ALLOW still wins when set, for tests and one-off shares. Absent BOTH a
    file and the env var, the guard REFUSES TO START rather than defaulting to
    "/" — a permissive default on a public-facing door is the fail-open shape
    this whole file exists to prevent.

    #3767 — a line may name the upstream that prefix routes to:

        /about   http://localhost:3000
        /athena

    A bare prefix uses SHARE_UPSTREAM. The two shares we publish live on
    different servers (the about page on :3000, the Athena pages on :3340), so a
    single global upstream can only ever serve one of them; on 2026-08-06 moving
    it to :3340 published Athena and 404'd /about the same minute. Routing
    belongs with the allowlist because it is the same question — what is public,
    and where does it come from — and splitting it across two files is how the
    two drift.
    """
    env = os.environ.get("SHARE_ALLOW")
    if env is not None:
        return [(p.strip(), None) for p in env.split(",") if p.strip()], "env:SHARE_ALLOW"
    path = os.environ.get(
        "SHARE_ALLOW_FILE",
        os.path.join(os.environ.get("CHORUS_ROOT", "/Users/jeffbridwell/CascadeProjects/chorus"),
                     "config", "share-allowlist.txt"),
    )
    try:
        with open(path) as fh:
            entries = []
            for line in fh:
                line = line.split("#", 1)[0].strip()
                if not line:
                    continue
                fields = line.split()
                entries.append((fields[0], fields[1].rstrip("/") if len(fields) > 1 else None))
        return entries, path
    except OSError:
        return [], path


def check_upstream(url):
    """A public door must never be pointed off-box by a config typo.

    Returns an error string, or None if the upstream is acceptable. Loopback and
    http/https only: the guard's whole premise is that it sits in front of things
    on THIS machine. An allowlist line reading `/x http://192.168.86.242:3000`
    would quietly turn the public tunnel into a proxy for another host, which is
    a reachability change nobody reviewing an allowlist would expect to be making.
    """
    if not url.startswith(("http://", "https://")):
        return f"upstream '{url}' is not http(s)"
    hostport = url.split("//", 1)[1].split("/", 1)[0]
    host = hostport.rsplit(":", 1)[0] if ":" in hostport else hostport
    if host.strip("[]") not in ("127.0.0.1", "::1", "localhost"):
        return f"upstream '{url}' is not loopback (host '{host}')"
    return None


ALLOW, ALLOW_SOURCE = load_allow()
AUTH = os.environ.get("SHARE_AUTH", "")
PORT = int(os.environ.get("SHARE_PORT", "8899"))

if not AUTH or ":" not in AUTH:
    print("chorus-share-guard: SHARE_AUTH=user:password is required — refusing to start unauthenticated", file=sys.stderr)
    sys.exit(2)

# #3744 — an empty allowlist is a misconfiguration, never "allow everything" and
# never a silent no-op. Fail closed, name the source that came up empty.
if not ALLOW:
    print(f"chorus-share-guard: allowlist is EMPTY (source: {ALLOW_SOURCE}) — refusing to start rather than "
          f"guessing a policy. Create the file or set SHARE_ALLOW.", file=sys.stderr)
    sys.exit(2)

# #3767 — every upstream the policy can reach is checked BEFORE serving anything,
# including the default. A typo that points a public prefix at another host is a
# reachability change, and it must stop the guard rather than be discovered by
# whoever notices the traffic.
for _prefix, _up in ALLOW + [("<SHARE_UPSTREAM>", UPSTREAM)]:
    _err = check_upstream(_up) if _up else None
    if _err:
        print(f"chorus-share-guard: {_prefix} -> {_err} — refusing to start. The guard fronts THIS "
              f"machine only; see {ALLOW_SOURCE}.", file=sys.stderr)
        sys.exit(2)

EXPECTED = "Basic " + base64.b64encode(AUTH.encode()).decode()

# Silas's #3644 envelope ask: fail-closed on bind misconfiguration — the guard is
# loopback-only by design (the tunnel dials OUT to it; nothing else may reach it).
BIND = os.environ.get("SHARE_BIND", "127.0.0.1")
if BIND not in ("127.0.0.1", "::1", "localhost"):
    print(f"chorus-share-guard: refusing non-loopback bind '{BIND}' — the tunnel is the only sanctioned ingress", file=sys.stderr)
    sys.exit(2)


def route(path, allow, default_upstream):
    """Return the upstream this path is served from, or None if it is not shared.

    #3767 — this replaces path_allowed(). It answers both questions in one pass
    because they are one decision: a path is public only if some prefix admits
    it, and the prefix that admits it also says where it comes from. Returning
    None for "not shared" keeps the caller's fail-closed shape — there is no way
    to get an upstream back without having matched an allowlist entry.

    First match wins, so ordering in the file is meaningful for overlapping
    prefixes; the longest-prefix subtlety is deliberately NOT introduced, since a
    reviewer reading top-to-bottom should be able to predict the outcome.
    """
    for p, upstream in allow:
        if p == "/" or path == p or path.startswith(p.rstrip("/") + "/"):
            return upstream or default_upstream
    return None


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
        upstream = route(self.path.split("?")[0], ALLOW, UPSTREAM)
        if upstream is None:
            return self._deny(404, "path not shared\n")
        req = urllib.request.Request(upstream + self.path, method="GET")
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
    # The startup line IS the policy record — #3744's finding was that nothing on
    # disk said what the public tunnel could reach. Print each prefix with the
    # upstream it resolves to, so the log answers "what is public and where does
    # it come from" without inspecting a running process.
    _routes = ", ".join(f"{p} -> {u or UPSTREAM}" for p, u in ALLOW)
    print(f"chorus-share-guard: {BIND}:{PORT}  default={UPSTREAM}  routes: {_routes} "
          f"(GET/HEAD only, basic-auth on, source={ALLOW_SOURCE})", file=sys.stderr)
    ThreadingHTTPServer((BIND if BIND != "localhost" else "127.0.0.1", PORT), Guard).serve_forever()
