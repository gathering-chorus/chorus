"""#4503 — one browser holds one session: signing in again replaces it.

Jeff, 2026-10-12: five open sessions for one person ("no this is a bandaid").
Loads the WHOLE door module (its real sign_in_session, open/close, cookie
signing) with the session store and state in a temp dir and person_session's
network calls stubbed: no IdP, no model door, no running guard. Counts the
Session rows left open. Each case prints PASS or FAIL; exit 1 on any FAIL.
Usage: one-browser-4503.py [path-to-chorus-share-guard.py]
"""
import os
import sys
import tempfile
import types

HERE = os.path.dirname(os.path.abspath(__file__))
GUARD = sys.argv[1] if len(sys.argv) > 1 else os.path.join(HERE, "..", "..", "scripts", "chorus-share-guard.py")
TMP = tempfile.mkdtemp(prefix="one-browser-4503-")
os.environ.update({
    "SHARE_STATE_FILE": os.path.join(TMP, "state.json"),
    "SHARE_PERSON_SESSIONS": os.path.join(TMP, "sessions.json"),
    "SHARE_ALLOW": "/about/",
    "SHARE_OIDC_OFFLINE": "1",
})
sys.path.insert(0, os.path.join(os.path.dirname(GUARD), "lib"))

g = {"__name__": "one-browser-4503", "__file__": GUARD}
exec(compile(open(GUARD).read(), GUARD, "exec"), g)

# the model door, as the open rows it holds
OPEN = set()
n = [0]


def open_session(webid, tok, now, ttl, sparql, http, api, log):
    n[0] += 1
    name = f"jeff-browser-{n[0]}"
    OPEN.add(name)
    return {"row": {"name": name, "expiresAt": "2999-01-01T00:00:00Z"}}


def close_session(rec, now, http, api, refresh, log):
    OPEN.discard(rec["row"]["name"])


g["person_session"] = types.SimpleNamespace(**{**vars(g["person_session"]), "open_session": open_session, "close_session": close_session})

FAILS = []
JEFF = "https://id.example.test/jeff/profile/card#me"
ANNE = "https://id.example.test/anne/profile/card#me"


def check(name, cond):
    print(("PASS " if cond else "FAIL ") + name)
    if not cond:
        FAILS.append(name)


def cookie_claims(sid, webid=JEFF):
    """What this browser's cookie says after a sign-in that opened `sid`."""
    return g["session_claims"](g["sign_session"](webid, g["COOKIE_KEY"], sid=sid), g["COOKIE_KEY"])


first = g["sign_in_session"](None, JEFF, {})
again = g["sign_in_session"](cookie_claims(first), JEFF, {})
check("two sign-ins from one browser leave one open session", OPEN == {again} and first != again)
third = g["sign_in_session"](cookie_claims(again), JEFF, {})
check("and a third still leaves one", OPEN == {third})
other = g["sign_in_session"](None, JEFF, {})
check("a second browser (no cookie) is a second session, not a replace", OPEN == {third, other})
g["sign_in_session"](cookie_claims(other, ANNE), JEFF, {})
check("NEGATIVE: another person's cookie closes none of his sessions", {third, other} <= OPEN)

# --- the access line says when and from what client (#4503, the 13:00 loop) ---
import io, contextlib, re
H = g["Guard"].__new__(g["Guard"])
H.headers = {"User-Agent": 'Mozilla/5.0 "quoted"', "Cookie": ""}
H.client_address = ("127.0.0.1", 1)
H.address_string = lambda: "127.0.0.1"
buf = io.StringIO()
with contextlib.redirect_stderr(buf):
    H.log_message('"%s" %s %s', "GET /_auth/logout HTTP/1.1", "302", "-")
line = buf.getvalue().strip()
check("the access line carries a UTC time", re.match(r"^\[guard\] \d{4}-\d\d-\d\dT\d\d:\d\d:\d\dZ 127\.0\.0\.1 anon ", line) is not None)
check("and the client's user agent, quotes made safe", line.endswith("ua=\"Mozilla/5.0 'quoted'\""))
sys.exit(1 if FAILS else 0)
