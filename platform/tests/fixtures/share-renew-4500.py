"""#4500 — the shared door renews a sign-in while he uses it, up to 30 days.

Jeff, 2026-10-12: "if i go to sleep and then return when i wake i typically
must login again". Drives the guard's REAL session functions (the definitions
slice, as verify-cookie-vectors.py does) and person_session.touch_session with
stub I/O: no network, no store, no running guard. Each case prints PASS or
FAIL; exit 1 on any FAIL.
"""
import base64
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
GUARD = os.path.join(HERE, "..", "..", "scripts", "chorus-share-guard.py")
sys.path.insert(0, os.path.join(HERE, "..", "..", "scripts", "lib"))
import person_session as ps  # noqa: E402

src = open(GUARD).read().split("ALLOW, ALLOW_SOURCE = load_allow()")[0]
g = {"__name__": "renew-4500", "__file__": GUARD}
exec(compile(src, GUARD, "exec"), g)

FAILS = []
KEY = b"test-key-4500-not-the-live-one-----"
JEFF = "https://id.example.test/jeff/profile/card#me"
PRINCIPALS = {JEFF}
HOUR, DAY = 3600, 86400
TTL, MAX = 12 * HOUR, 30 * DAY
T = 1_791_700_000  # sign-in
SID = "jeff-browser-1791700000"
RECS = {SID: {"row": {"name": SID}}}


def check(name, cond):
    print(("PASS " if cond else "FAIL ") + name)
    if not cond:
        FAILS.append(name)


def claims_of(cookie, now):
    return g["session_claims"](cookie, KEY, now, MAX)


def exp_of(cookie):
    return json.loads(g["b64u_decode"](cookie.split(".", 1)[0]))["exp"]


def decide(cookie, now, recs=RECS, principals=PRINCIPALS):
    return g["session_decision"](claims_of(cookie, now), recs, principals, KEY, now, TTL, 600, MAX)


signed = g["sign_session"](JEFF, KEY, TTL, T, None, SID, MAX)

# --- his night: signed in at 6pm, back at 7am ---------------------------------
check("without use, the sign-in still runs out after the idle window",
      claims_of(signed, T + 13 * HOUR) is None)
state, renewed = decide(signed, T + 2 * HOUR)
check("used two hours in, it renews to a fresh 12 hours", state == "ok" and renewed and exp_of(renewed) == T + 14 * HOUR)
check("the renewed cookie keeps his sign-in time and session",
      claims_of(renewed, T + 2 * HOUR).get("iat") == T and claims_of(renewed, T + 2 * HOUR).get("sid") == SID)
check("used at 11pm, he is still in at 7am (13h after sign-in)",
      claims_of(g["sign_session"](JEFF, KEY, TTL, T + 5 * HOUR, T, SID, MAX), T + 13 * HOUR) is not None)
state, renewed = decide(signed, T + 60)
check("inside the first ten minutes nothing is re-issued", state == "ok" and renewed is None)

# --- Silas's ask 2: 30 days is the end, however recent the use -------------------
cookie, t = signed, T
while t + 11 * HOUR < T + MAX:  # used every 11 hours for 30 days
    t += 11 * HOUR
    state, nxt = decide(cookie, t)
    if state != "ok":
        break
    cookie = nxt or cookie
check("used every 11 hours, he stays signed in for the 30 days", state == "ok")
check("no renewal ever runs past sign-in + 30 days", exp_of(cookie) <= T + MAX)
last_use = T + MAX - 60
state, nxt = decide(cookie, last_use)
cookie = nxt or cookie
check("NEGATIVE: past 30 days it refuses, though it was used a minute ago",
      claims_of(cookie, T + MAX) is None and decide(cookie, T + MAX)[0] == "anonymous")
forged_long = g["sign_session"](JEFF, KEY, 40 * DAY, T + MAX - 60, T, SID, 400 * DAY)
check("NEGATIVE: a cookie signed with a long exp is still refused past iat + 30 days",
      claims_of(forged_long, T + MAX + 1) is None)

# --- Silas's ask 1: revoked or logged out fails on its next request ---------------
state, renewed = decide(signed, T + 2 * HOUR, recs={})
check("NEGATIVE: signed out (no record for its session) is refused and not renewed",
      state == "logged-out" and renewed is None)
state, renewed = decide(signed, T + 2 * HOUR, principals=set())
check("NEGATIVE: revoked (off the allow-set) is refused and not renewed",
      state == "not-authorized" and renewed is None)
legacy = g["b64u"](json.dumps({"webid": JEFF, "exp": T + TTL}).encode())
legacy += "." + g["b64u"](__import__("hmac").new(KEY, legacy.encode(), __import__("hashlib").sha256).digest())
state, renewed = decide(legacy, T + 11 * HOUR)
check("a cookie from before #4500 (no session id) is accepted until it runs out, never renewed",
      state == "ok" and renewed is None and claims_of(legacy, T + TTL) is None)

# --- the row follows the cookie --------------------------------------------------
FAKE_JWT = ".".join(["h", base64.urlsafe_b64encode(json.dumps({"jti": "tok-1"}).encode()).decode().rstrip("="), "s"])
calls = []


def http_ok(method, url, bearer, body):
    calls.append((method, url, bearer, body))
    return 200, {}


rec = {"principal": "jeff", "row": {"name": SID, "sessionState": "open", "expiresAt": ps.iso(T + TTL)},
       "id_token": FAKE_JWT, "refresh_token": "rt-1", "exp": T + 2 * HOUR}
out = ps.touch_session(dict(rec), T + 2 * HOUR - 30 * 60, T + 14 * HOUR, http_ok, "http://door", lambda rt: None, lambda m: None)
check("renewal PUTs his row with lastSeenAt now and expiresAt the cookie's",
      out and calls[-1][0] == "PUT" and calls[-1][3]["lastSeenAt"] == ps.iso(T + 2 * HOUR - 30 * 60)
      and calls[-1][3]["expiresAt"] == ps.iso(T + 14 * HOUR))
fresh = {"id_token": FAKE_JWT + "x", "refresh_token": "rt-2", "expires_in": 3600}
out = ps.touch_session(dict(rec), T + 3 * HOUR, T + 15 * HOUR, http_ok, "http://door", lambda rt: fresh, lambda m: None)
check("an expired token is refreshed, used, and kept", out and calls[-1][2] == FAKE_JWT + "x" and out["refresh_token"] == "rt-2")
n = len(calls)
out = ps.touch_session(dict(rec), T + 3 * HOUR, T + 15 * HOUR, http_ok, "http://door", lambda rt: None, lambda m: None)
check("NEGATIVE: an expired token that cannot be refreshed writes nothing", out is None and len(calls) == n)
out = ps.touch_session(dict(rec), T + 2 * HOUR, T + 14 * HOUR, lambda *a: (403, {}), "http://door", lambda rt: None, lambda m: None)
check("NEGATIVE: a refused PUT is not reported as renewed", out is None)

# --- one browser, one session (reopen 2, 10-12 10:21) ---------------------------
rs = g["replaced_sid"]
check("signing in again with his own old cookie replaces that session", rs({"webid": JEFF, "sid": SID}, JEFF) == SID)
check("NEGATIVE: another person's cookie closes nothing", rs({"webid": JEFF, "sid": SID}, "https://id.example.test/anne#me") is None)
check("NEGATIVE: a cookie with no session id, or no cookie, closes nothing", rs({"webid": JEFF}, JEFF) is None and rs(None, JEFF) is None)

sys.exit(1 if FAILS else 0)
