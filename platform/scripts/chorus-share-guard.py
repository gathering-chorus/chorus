#!/usr/bin/env python3
"""chorus-share-guard — read-only, path-allowlisted, identity-authenticated proxy (#3644).

The tunnel-facing guard for reaching Chorus pages off home infra. Whatever
tunnel provider fronts it, the guarantees live HERE, on our side of the boundary:

  - GET/HEAD only — every write verb is 405 before it touches the upstream
  - path-prefix allowlist — anything else is 404, so :3030/:3340 and the rest
    of the LAN write plane are unreachable BY CONSTRUCTION, not by trust
  - per-prefix upstreams, loopback only (#3767)
  - people sign in as THEMSELVES at the identity provider we already run, and
    are authorized against a governed allow-set of WebIDs (#3770)

#3770 retired HTTP Basic auth. It was a #3644 leftover: that card was a one-off
(share a page for an afternoon, generate a random password, throw it away), and
the surface then became the permanent public front door without anyone revisiting
the lock. The result asked Jeff — a modelled Principal with a WebID at an
identity provider we run ourselves — for a shared secret a script invented and
never told him. One system, not two: authentication is the IdP's answer, and
authorization is a WebID allow-set governed exactly like the path allowlist.

Authentication and authorization are separate on purpose. Signing in proves who
you are; it grants nothing. Reach comes only from being named in the allow-set,
so revoking someone is removing a line, and it does not disturb anyone else.

Env (overridable for tests):
  SHARE_UPSTREAM        default upstream base
  SHARE_ALLOW           comma-separated path prefixes (tests / one-offs)
  SHARE_ALLOW_FILE      path-prefix allowlist file
  SHARE_PRINCIPALS_FILE WebID allow-set file
  SHARE_OIDC_ISSUER     identity provider base URL
  SHARE_STATE_FILE      runtime client registration + cookie key (never committed)
  SHARE_PUBLIC_ORIGIN   public origin used to build the redirect URI
  SHARE_PORT            listen port, default 8899
"""
import base64
import hashlib
import hmac
import json
import os
import secrets
import sys
import time
import urllib.error
import urllib.parse
import urllib.request
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

SESSION_COOKIE = "chorus_share_session"
SESSION_TTL = int(os.environ.get("SHARE_SESSION_TTL", str(12 * 3600)))
AUTH_PREFIX = "/_auth/"

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


def load_principals():
    """#3770 — WHO may read, as a governed file of WebIDs.

    Same shape as the path allowlist and for the same reason (#3744): a reach
    policy that lives anywhere but a reviewable file cannot be reviewed. One
    WebID per line; '#' comments and blank lines ignored.

    Authentication is not authorization. A person who signs in successfully but
    is not named here is REFUSED — proving who you are grants nothing on its own,
    and that separation is what makes revocation a one-line edit affecting one
    person.
    """
    path = os.environ.get(
        "SHARE_PRINCIPALS_FILE",
        os.path.join(os.environ.get("CHORUS_ROOT", "/Users/jeffbridwell/CascadeProjects/chorus"),
                     "config", "share-principals.txt"),
    )
    try:
        with open(path) as fh:
            entries = []
            for line in fh:
                # A WebID ENDS in a fragment — https://host/profile/card#me — so
                # the '#' comment rule used elsewhere in this file cannot apply
                # here: splitting on the first '#' silently truncates every entry
                # to a WebID nobody has, and the allow-set then matches no one
                # while looking perfectly correct in the file. Caught by a test
                # (a session verified, then was refused); comments must therefore
                # be whole-line, or set off by whitespace.
                line = line.strip()
                if not line or line.startswith("#"):
                    continue
                entries.append(line.split(" #", 1)[0].split("\t#", 1)[0].strip())
        return entries, path
    except OSError:
        return [], path


def load_state(path):
    """Runtime secrets: the cookie signing key and the registered OIDC client.

    Never committed. Regenerating the key invalidates outstanding sessions, which
    is the intended effect of losing this file — sessions fail closed rather than
    surviving a key we no longer hold.
    """
    try:
        with open(path) as fh:
            return json.load(fh)
    except (OSError, ValueError):
        return {}


def save_state(path, state):
    os.makedirs(os.path.dirname(path), exist_ok=True)
    fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600)
    with os.fdopen(fd, "w") as fh:
        json.dump(state, fh)


def b64u(raw):
    return base64.urlsafe_b64encode(raw).decode().rstrip("=")


def b64u_decode(txt):
    return base64.urlsafe_b64decode(txt + "=" * (-len(txt) % 4))


def sign_session(webid, key, ttl=SESSION_TTL, now=None):
    """A session is a signed statement of WHO, with an expiry. Nothing else.

    Deliberately not a store: no server-side session table to leak or to grow.
    The cookie carries the WebID; the signature is what makes it trustworthy, and
    the allow-set is consulted on EVERY request rather than baked in at sign-in —
    so removing someone takes effect on their next request, not when their
    session happens to expire.
    """
    now = int(time.time()) if now is None else now
    payload = b64u(json.dumps({"webid": webid, "exp": now + ttl}).encode())
    mac = b64u(hmac.new(key, payload.encode(), hashlib.sha256).digest())
    return f"{payload}.{mac}"


def verify_session(cookie, key, now=None):
    """Return the WebID a valid session names, or None. Never raises on garbage."""
    now = int(time.time()) if now is None else now
    try:
        payload, mac = cookie.split(".", 1)
        expected = b64u(hmac.new(key, payload.encode(), hashlib.sha256).digest())
        # constant-time: a timing oracle here would leak the signature byte by byte
        if not hmac.compare_digest(mac, expected):
            return None
        claims = json.loads(b64u_decode(payload))
        if int(claims.get("exp", 0)) <= now:
            return None
        return claims.get("webid") or None
    except Exception:
        return None


ALLOW, ALLOW_SOURCE = load_allow()
PRINCIPALS, PRINCIPALS_SOURCE = load_principals()


def current_principals():
    """Re-read the allow-set per request, so revoking is immediate.

    The startup snapshot is the wrong thing to authorize against: removing
    someone would then take effect whenever the guard next restarted, which is
    to say at an unknown time, which is to say revocation would not be a thing
    we could honestly claim to have. A test caught exactly this — the file was
    edited, the guard kept serving the removed WebID.

    An unreadable or emptied file returns the last known-good set rather than
    admitting everyone: mid-edit truncation must not become an open door. The
    empty-file case still refuses at STARTUP, where it means a misconfiguration
    rather than a half-written save.
    """
    global PRINCIPALS
    fresh, _ = load_principals()
    if fresh:
        PRINCIPALS = fresh
    return PRINCIPALS
PORT = int(os.environ.get("SHARE_PORT", "8899"))

# #3770 — an empty allow-set is a misconfiguration, never "everyone who can sign
# in". Same fail-closed shape as the empty path allowlist.
if not PRINCIPALS:
    print(f"chorus-share-guard: WebID allow-set is EMPTY (source: {PRINCIPALS_SOURCE}) — refusing to start "
          f"rather than admitting everyone who can authenticate.", file=sys.stderr)
    sys.exit(2)

# Basic auth is GONE (#3770). If someone still sets SHARE_AUTH, say so loudly
# rather than silently ignoring it — a config that looks like it grants access
# but does nothing is how people believe a door is locked when it is not.
if os.environ.get("SHARE_AUTH"):
    print("chorus-share-guard: SHARE_AUTH is set but basic auth was retired in #3770 — sign-in is via the "
          "identity provider. Remove SHARE_AUTH; it grants nothing.", file=sys.stderr)
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

# --- #3770 identity wiring -------------------------------------------------

ISSUER = os.environ.get("SHARE_OIDC_ISSUER", "https://id.lightlifeurbangardens.com").rstrip("/")
PUBLIC_ORIGIN = os.environ.get("SHARE_PUBLIC_ORIGIN", "https://chorus.lightlifeurbangardens.com").rstrip("/")
REDIRECT_URI = PUBLIC_ORIGIN + AUTH_PREFIX + "callback"
STATE_FILE = os.environ.get("SHARE_STATE_FILE", os.path.join(os.path.expanduser("~"), ".chorus", "share-oidc.json"))
OFFLINE = os.environ.get("SHARE_OIDC_OFFLINE") == "1"  # tests: sign sessions without a live IdP

STATE = load_state(STATE_FILE)
if "cookie_key" not in STATE:
    STATE["cookie_key"] = b64u(secrets.token_bytes(32))
    save_state(STATE_FILE, STATE)
COOKIE_KEY = b64u_decode(STATE["cookie_key"])

# In-flight authorization attempts: state -> (verifier, return_path, created).
# Bounded and short-lived; an unfinished sign-in is forgotten, not accumulated.
PENDING = {}


USER_AGENT = "chorus-share-guard/1.0 (+https://chorus.lightlifeurbangardens.com)"


def fetch(url, data=None, headers=None, timeout=20):
    """Every outbound request the guard makes, with an identity attached.

    #3771 — urllib's default User-Agent is `Python-urllib/3.9`, and Cloudflare
    answers it with 403. So the guard could not reach its OWN identity provider:
    discovery failed, client registration never happened, and every sign-in
    returned 503. Measured directly — same URL, same moment, 403 with the default
    agent and 200 with any other.

    A tool that talks to a service across a CDN has to say who it is. The default
    library agent is not an identity, it is the absence of one, and infrastructure
    increasingly treats it as a bot signature.
    """
    req = urllib.request.Request(url, data=data, method="POST" if data else "GET")
    req.add_header("User-Agent", USER_AGENT)
    for k, v in (headers or {}).items():
        req.add_header(k, v)
    return urllib.request.urlopen(req, timeout=timeout)


def discover():
    with fetch(ISSUER + "/.well-known/openid-configuration", timeout=15) as r:
        return json.load(r)


def register_client(conf):
    """Register this guard as an OIDC client, once, and remember it.

    Dynamic registration on every start would litter the provider with clients
    and change the client_id under live sessions, so the registration is stored
    beside the cookie key in the runtime state file.
    """
    body = json.dumps({
        "client_name": "chorus-share-guard",
        "redirect_uris": [REDIRECT_URI,
                          "https://share.lightlifeurbangardens.com" + AUTH_PREFIX + "callback"],
        "grant_types": ["authorization_code", "refresh_token"],
        "response_types": ["code"],
        "token_endpoint_auth_method": "client_secret_basic",
        "scope": "openid webid profile offline_access",
    }).encode()
    with fetch(conf["registration_endpoint"], data=body,
               headers={"Content-Type": "application/json"}) as r:
        return json.load(r)


def oidc():
    """Lazy: the guard must start and serve refusals even if the IdP is down.

    A provider outage should mean "nobody can sign in", not "the guard is dead"
    — a dead guard is indistinguishable from a broken tunnel to whoever is
    looking, and it takes the honest 404s and 405s down with it.
    """
    if OFFLINE:
        return None, None
    if "conf" not in STATE.setdefault("_cache", {}):
        conf = discover()
        STATE["_cache"]["conf"] = conf
    conf = STATE["_cache"]["conf"]
    if "client" not in STATE:
        STATE["client"] = register_client(conf)
        save_state(STATE_FILE, {k: v for k, v in STATE.items() if k != "_cache"})
    return conf, STATE["client"]


def webid_from_id_token(id_token):
    """Read the WebID claim. The token came from the token endpoint over TLS.

    Signature verification against the provider's JWKS is the correct belt to add
    here; this reads the claim from a token obtained directly from the issuer's
    token endpoint, which is why it is not itself the trust boundary. Noted rather
    than glossed: if this code ever accepts a token from anywhere but that direct
    exchange, it MUST verify the signature first.
    """
    payload = id_token.split(".")[1]
    claims = json.loads(b64u_decode(payload))
    return claims.get("webid") or claims.get("sub")


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
        self.send_header("Content-Type", "text/plain")
        self.end_headers()
        self.wfile.write(msg.encode())

    def _wants_html(self):
        return "text/html" in self.headers.get("Accept", "")

    def _page(self, code, title, body_html):
        """Render a guard-owned page.

        Self-contained by necessity, not preference: these pages are exactly what
        a visitor sees when the upstream is unreachable, when the identity
        provider is down, or before they have any right to reach either. A page
        that fetched a stylesheet through the guard would be blank in every one
        of those cases.
        """
        html = f"""<!doctype html>
<html lang="en"><head><meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>{title} — Chorus</title>
<style>
  :root {{ color-scheme: light dark;
    --ink:#1b1d21; --dim:#5f6873; --line:#dfe3e8; --bg:#f7f8fa; --card:#fff; --accent:#2f5d50; }}
  @media (prefers-color-scheme: dark) {{ :root {{
    --ink:#e9ecef; --dim:#a0a8b4; --line:#2b3138; --bg:#131619; --card:#1a1e23; --accent:#79c4ab; }} }}
  * {{ box-sizing:border-box; }}
  body {{ margin:0; min-height:100vh; display:grid; place-items:center; padding:2rem;
    background:var(--bg); color:var(--ink);
    font:16px/1.6 ui-serif, Georgia, 'Iowan Old Style', serif; }}
  main {{ width:min(30rem,100%); background:var(--card); border:1px solid var(--line);
    border-radius:3px; padding:2.25rem 2rem; }}
  h1 {{ margin:0 0 .5rem; font-size:1.4rem; letter-spacing:-.01em; }}
  p {{ margin:0 0 1.1rem; color:var(--dim); }}
  .mark {{ font:600 .68rem/1 ui-monospace,SFMono-Regular,Menlo,monospace;
    letter-spacing:.16em; text-transform:uppercase; color:var(--dim); margin:0 0 1.4rem; }}
  a.btn {{ display:inline-block; background:var(--accent); color:var(--bg);
    text-decoration:none; padding:.7rem 1.4rem; border-radius:2px; font-family:system-ui,sans-serif;
    font-size:.95rem; font-weight:600; }}
  a.btn:focus-visible {{ outline:3px solid var(--accent); outline-offset:3px; }}
  code {{ font:.85em ui-monospace,SFMono-Regular,Menlo,monospace; word-break:break-all; color:var(--ink); }}
</style></head>
<body><main>
<p class="mark">Chorus</p>
{body_html}
</main></body></html>"""
        raw = html.encode()
        self.send_response(code)
        self.send_header("Content-Type", "text/html; charset=utf-8")
        self.send_header("Content-Length", str(len(raw)))
        self.end_headers()
        self.wfile.write(raw)
        return True

    def _sign_in_page(self, next_path):
        nxt = urllib.parse.quote(next_path, safe="")
        return self._page(401, "Sign in", f"""
<h1>Sign in to continue</h1>
<p>These pages are private. Sign in with your Chorus identity — the same one you
use for the Clearing.</p>
<p><a class="btn" href="{AUTH_PREFIX}login?next={nxt}">Sign in</a></p>""")

    def _redirect(self, location, cookie=None):
        self.send_response(302)
        self.send_header("Location", location)
        if cookie:
            self.send_header("Set-Cookie", cookie)
        self.send_header("Content-Length", "0")
        self.end_headers()

    def _session_webid(self):
        raw = self.headers.get("Cookie", "")
        for part in raw.split(";"):
            name, _, value = part.strip().partition("=")
            if name == SESSION_COOKIE:
                return verify_session(value, COOKIE_KEY)
        return None

    def _auth_routes(self):
        """Sign-in endpoints. Served BY the guard, never proxied upstream.

        These live under /_auth/ and are handled before the allowlist, because
        the allowlist governs what of the UPSTREAM is public — the sign-in flow
        is the guard's own surface and must be reachable to an anonymous caller
        or nobody could ever authenticate.
        """
        path, _, query = self.path.partition("?")
        args = urllib.parse.parse_qs(query)

        if path == AUTH_PREFIX + "login":
            try:
                conf, client = oidc()
            except Exception:
                conf = None
            if not conf:
                # An outage here is not the visitor's fault and not their
                # problem to debug. Say what broke, in a page, and let them retry.
                if self._wants_html():
                    return self._page(503, "Sign-in unavailable", """
<h1>Can't reach the sign-in service</h1>
<p>Chorus is up, but the service that verifies who you are isn't answering.
Nothing is wrong with your account. Try again in a moment.</p>""")
                return self._deny(503, "identity provider unavailable\n")
            verifier = b64u(secrets.token_bytes(32))
            challenge = b64u(hashlib.sha256(verifier.encode()).digest())
            state = b64u(secrets.token_bytes(16))
            PENDING[state] = (verifier, args.get("next", ["/"])[0], time.time())
            # forget stale attempts rather than growing without bound
            for k, v in list(PENDING.items()):
                if time.time() - v[2] > 600:
                    PENDING.pop(k, None)
            q = urllib.parse.urlencode({
                "response_type": "code",
                "client_id": client["client_id"],
                "redirect_uri": REDIRECT_URI,
                "scope": "openid webid profile",
                "state": state,
                "code_challenge": challenge,
                "code_challenge_method": "S256",
            })
            return self._redirect(conf["authorization_endpoint"] + "?" + q)

        if path == AUTH_PREFIX + "callback":
            try:
                conf, client = oidc()
            except Exception:
                conf = None
            if not conf:
                return self._deny(503, "identity provider unavailable\n")
            state = args.get("state", [""])[0]
            pending = PENDING.pop(state, None)
            if not pending:
                # unknown state = not a sign-in we started (or a replay)
                return self._deny(400, "sign-in expired or not recognised\n")
            verifier, return_path, _ = pending
            code = args.get("code", [""])[0]
            body = urllib.parse.urlencode({
                "grant_type": "authorization_code",
                "code": code,
                "redirect_uri": REDIRECT_URI,
                "code_verifier": verifier,
            }).encode()
            basic = base64.b64encode(f"{client['client_id']}:{client.get('client_secret','')}".encode()).decode()
            try:
                with fetch(conf["token_endpoint"], data=body, headers={
                    "Authorization": "Basic " + basic,
                    "Content-Type": "application/x-www-form-urlencoded",
                }) as r:
                    tok = json.load(r)
            except Exception:
                return self._deny(502, "could not complete sign-in with the identity provider\n")
            webid = webid_from_id_token(tok.get("id_token", ""))
            if not webid:
                return self._deny(502, "identity provider returned no WebID\n")
            # NOTE: the allow-set is NOT consulted here. Signing in always
            # succeeds; reach is decided per request, so revocation is immediate.
            cookie = (f"{SESSION_COOKIE}={sign_session(webid, COOKIE_KEY)}; Path=/; HttpOnly; "
                      f"Secure; SameSite=Lax; Max-Age={SESSION_TTL}")
            return self._redirect(return_path if return_path.startswith("/") else "/", cookie)

        if path == AUTH_PREFIX + "whoami":
            webid = self._session_webid()
            allowed = bool(webid) and webid in current_principals()
            payload = json.dumps({"webid": webid, "allowed": allowed}).encode()
            self.send_response(200 if webid else 401)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(payload)))
            self.end_headers()
            self.wfile.write(payload)
            return True

        if path in (AUTH_PREFIX, AUTH_PREFIX.rstrip("/")):
            return self._sign_in_page(args.get("next", ["/"])[0])

        if path == AUTH_PREFIX + "logout":
            return self._redirect("/", f"{SESSION_COOKIE}=; Path=/; Max-Age=0")

        return self._deny(404, "no such sign-in route\n")

    def _handle(self, body_allowed):
        if self.path.split("?")[0].startswith(AUTH_PREFIX):
            return self._auth_routes()

        webid = self._session_webid()
        if not webid:
            # A person gets a page; a script gets a status. Bouncing a browser
            # straight to the identity provider means the first thing a visitor
            # sees is someone else's domain asking for credentials, with no
            # indication of what asked or why — which is the shape of a phishing
            # page. Say where they are first, then let them choose to go.
            if self._wants_html():
                return self._sign_in_page(self.path)
            return self._redirect(f"{AUTH_PREFIX}login?next={urllib.parse.quote(self.path, safe='')}")
        # Authentication is not authorization (#3770): a valid session proves who
        # you are and nothing more. Checked per request so removing a line from
        # the allow-set revokes on the NEXT request, not at session expiry.
        if webid not in current_principals():
            if self._wants_html():
                return self._page(403, "Not authorized", f"""
<h1>You're signed in, but not on the list</h1>
<p>Chorus knows who you are — <code>{webid}</code> — and that isn't the same as
being allowed in. Access is granted per person; ask Jeff to add you.</p>
<p><a class="btn" href="{AUTH_PREFIX}logout">Sign out</a></p>""")
            return self._deny(403, "signed in, but not authorized for this surface\n")

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
        # The audit line names WHO, not just "authed" — a shared password could
        # only ever say that someone knew it. This is the point of #3770.
        authed = self._session_webid() or "anon"
        print(f"[guard] {self.address_string()} {authed} {fmt % args}", file=sys.stderr)


if __name__ == "__main__" and len(sys.argv) > 2 and sys.argv[1] == "--sign-session":
    # Test seam, deliberate and narrow. Tests must exercise the REAL signing code
    # rather than reimplementing it — a second implementation of session signing
    # is exactly the kind of thing that agrees with the first while both are
    # wrong. This grants no access it did not already have: producing a session
    # requires reading the cookie key from the 0600 state file, and anyone who
    # can read that file can already forge sessions directly.
    print(sign_session(sys.argv[2], COOKIE_KEY))
    sys.exit(0)

if __name__ == "__main__":
    # The startup line IS the policy record — #3744's finding was that nothing on
    # disk said what the public tunnel could reach. Print each prefix with the
    # upstream it resolves to, so the log answers "what is public and where does
    # it come from" without inspecting a running process.
    _routes = ", ".join(f"{p} -> {u or UPSTREAM}" for p, u in ALLOW)
    print(f"chorus-share-guard: {BIND}:{PORT}  default={UPSTREAM}  routes: {_routes} "
          f"(GET/HEAD only, source={ALLOW_SOURCE})", file=sys.stderr)
    # #3771 — say out loud whether sign-in can actually work. The failure that
    # prompted this was invisible for as long as nobody clicked: the guard
    # started cleanly, served refusals correctly, and rendered a polished "can't
    # reach the sign-in service" page — a graceful degradation indistinguishable
    # from a hard break. A door that cannot be opened is not a working door, and
    # the log should say so at the moment it starts, not when a person discovers it.
    if OFFLINE:
        _reach = "OFFLINE (test mode) — no provider contacted"
    else:
        try:
            discover()
            _reach = "reachable"
        except Exception as _e:
            _reach = f"UNREACHABLE ({type(_e).__name__}: {_e}) — NOBODY CAN SIGN IN"
    print(f"chorus-share-guard: identity provider {ISSUER} is {_reach}", file=sys.stderr)
    print(f"chorus-share-guard: sign-in via {ISSUER}; {len(PRINCIPALS)} WebID(s) authorized "
          f"(source={PRINCIPALS_SOURCE}). Signing in does not grant reach — the allow-set does.",
          file=sys.stderr)
    ThreadingHTTPServer((BIND if BIND != "localhost" else "127.0.0.1", PORT), Guard).serve_forever()
