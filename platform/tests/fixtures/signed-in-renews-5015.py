"""#5015 AC2 — Jeff's browser sign-in keeps renewing once the door records it.

Drives the guard's REAL functions (sign_session, session_claims, session_decision;
the definitions slice, as share-renew-4500.py does) and person_session.open_session
with a stub door. Case DOOR=201: the door records the login (#5015 AC1 gives a no-jti
token its signature id), so the cookie carries the row's sid and is renewed after the
renew window. Case DOOR=401 (AC1 reverted: "login has no id"): no row, a sid-less
cookie, never renewed: the sign-out Jeff saw. Prints PASS/FAIL; exit 1 on any FAIL.
"""
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
GUARD = os.environ.get("GUARD_5015") or os.path.join(HERE, "..", "..", "scripts", "chorus-share-guard.py")
sys.path.insert(0, os.path.join(HERE, "..", "..", "scripts", "lib"))
import person_session as ps  # noqa: E402

src = open(GUARD).read().split("ALLOW, ALLOW_SOURCE = load_allow()")[0]
g = {"__name__": "signed-in-5015", "__file__": GUARD}
exec(compile(src, GUARD, "exec"), g)

FAILS = []
KEY = b"test-key-5015-not-the-live-one-----"
JEFF = "https://id.example.test/jeff/profile/card#me"
TTL, RENEW = g["SESSION_TTL"], g["RENEW_EVERY"]   # the guard's live settings, not copies
T = 1_795_100_000
TOK = {"id_token": "header.payload.signature-without-jti", "refresh_token": "r", "expires_in": 3600}


def check(name, cond):
    print(("PASS " if cond else "FAIL ") + name)
    if not cond:
        FAILS.append(name)


def sign_in(door_status):
    """The guard's sign-in path: open_session asks the door, the cookie carries what it got."""
    def sparql(_q):
        return ("jeff", "jeffbridwell")
    ps_principal = ps.principal_for
    ps.principal_for = lambda webid, sparql: ("jeff", "jeffbridwell")
    try:
        def http(method, url, bearer, body):
            assert method == "POST" and url.endswith("/v1/identity/logins")
            if door_status == 201:
                return 201, {"name": "jeff-browser-sig0123456789abcdef"}
            return 401, {"error": "login has no id"}
        rec = ps.open_session(JEFF, TOK, T, TTL, sparql, http, "http://door", lambda m: None)
    finally:
        ps.principal_for = ps_principal
    sid = rec["row"]["name"] if rec else None
    cookie = g["sign_session"](JEFF, KEY, TTL, T, T, sid)
    return sid, cookie, ({sid: rec} if sid else {})


def after_idle(cookie, recs, idle):
    now = T + idle
    claims = g["session_claims"](cookie, KEY, now)
    return g["session_decision"](claims, recs, {JEFF}, KEY, now, ttl=TTL, renew_every=RENEW)


sid, cookie, recs = sign_in(201)
check("door 201: the cookie carries the Session's sid", sid == "jeff-browser-sig0123456789abcdef")
state, renewed = after_idle(cookie, recs, 3600 + 60)
check("door 201: an idle hour later the sign-in is ok and renewed", state == "ok" and renewed is not None)

sid0, cookie0, recs0 = sign_in(401)
check("door 401 (AC1 reverted): no row, the cookie has no sid", sid0 is None)
state0, renewed0 = after_idle(cookie0, recs0, 3600 + 60)
check("door 401: an idle hour later it is NOT renewed (runs out: the sign-out)", renewed0 is None)

sys.exit(1 if FAILS else 0)
