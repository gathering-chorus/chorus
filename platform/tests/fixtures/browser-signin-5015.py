"""#5015 AC3 — Jeff's browser path for login.feature: sign in, idle, renew, two devices, sign out.

Drives the guard's REAL sign-in and sign-out (open_person_session, close_person_session,
sign_session, session_claims, session_decision) with a stub door, one action per call.
  browser-signin-5015.py STATE signin <device>      the guard's sign-in: door row → cookie
  browser-signin-5015.py STATE idle <seconds>       the clock moves
  browser-signin-5015.py STATE visit <device>       one request: prints state and whether it renewed
  browser-signin-5015.py STATE signout <device>     his sign-out closes that device's row
The stub door answers 201 + a row name, or 401 "login has no id" when DOOR_5015=401
(the #4517 regression this card fixes). Prints one JSON line per call.
"""
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
GUARD = os.environ.get("GUARD_5015") or os.path.join(HERE, "..", "..", "scripts", "chorus-share-guard.py")
STATE = sys.argv[1]
# The guard's own session store is this scenario's file; its server never starts
# (the module only serves under __main__).
os.environ["SHARE_PERSON_SESSIONS"] = STATE + ".sessions"
# Silas 13:38: importing the guard loads its OIDC state at import (guard.py:560) and,
# when the key is missing, writes a new one. Never the live ~/.chorus/share-oidc.json:
# point it, and every other store, at this scenario's world, offline.
os.environ["SHARE_STATE_FILE"] = STATE + ".oidc.json"
os.environ["SHARE_OIDC_OFFLINE"] = "1"
os.environ["ATHENA_MAKE_URL"] = "http://127.0.0.1:9"
os.environ["FUSEKI_QUERY"] = "http://127.0.0.1:9/none"
sys.path.insert(0, os.path.join(HERE, "..", "..", "scripts", "lib"))
g = {"__name__": "browser-5015", "__file__": GUARD}
exec(compile(open(GUARD).read(), GUARD, "exec"), g)

KEY = b"test-key-5015-not-the-live-one-----"
JEFF = "https://id.example.test/jeff/profile/card#me"
TTL, RENEW = g["SESSION_TTL"], g["RENEW_EVERY"]   # the guard's live settings, not copies
T0 = 1_795_100_000


def door(n):
    """The stub door: 201 + a row name, or 401 "login has no id" (DOOR_5015=401)."""
    code = int(os.environ.get("DOOR_5015", "201"))

    def http(method, url, bearer, body):
        if url.endswith("/v1/identity/logins") and method == "POST":
            return (201, {"name": f"jeff-browser-sig{n:04d}"}) if code == 201 else (401, {"error": "login has no id"})
        return 200, {}
    return http


def main():
    action, arg = sys.argv[2], sys.argv[3]
    st = json.load(open(STATE)) if os.path.exists(STATE) else {"now": T0, "cookies": {}, "n": 0}
    now = st["now"]
    g["person_session"].principal_for = lambda webid, sparql: ("jeff", "jeffbridwell")
    g["time"].time = lambda: now
    out = {}
    if action == "signin":
        st["n"] += 1
        g["_http"] = door(st["n"])
        tok = {"id_token": f"h.p.sig{st['n']}", "refresh_token": "r", "expires_in": 3600}
        sid = g["open_person_session"](JEFF, tok)          # the guard's real sign-in path
        st["cookies"][arg] = g["sign_session"](JEFF, KEY, TTL, now, now, sid)
        out = {"device": arg, "sid": sid, "sessions": len(g["load_state"](g["SESSIONS_FILE"]))}
    elif action == "idle":
        st["now"] = now + int(arg)
        out = {"now": st["now"]}
    elif action == "visit":
        cookie = st["cookies"].get(arg)
        claims = g["session_claims"](cookie, KEY, now) if cookie else None
        recs = g["load_state"](g["SESSIONS_FILE"])
        state, renewed = g["session_decision"](claims, recs, {JEFF}, KEY, now, ttl=TTL, renew_every=RENEW)
        if renewed:
            st["cookies"][arg] = renewed
        out = {"device": arg, "state": state, "renewed": renewed is not None}
    elif action == "signout":
        g["_http"] = door(0)
        g["close_person_session"](g["session_claims"](st["cookies"][arg], KEY, now))   # the guard's real sign-out
        out = {"device": arg, "sessions": len(g["load_state"](g["SESSIONS_FILE"]))}
    json.dump(st, open(STATE, "w"))
    print(json.dumps(out))


if __name__ == "__main__":
    main()
