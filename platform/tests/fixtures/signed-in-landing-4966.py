"""#4966 — a signed-in caller at /_auth/ is sent on to where they were going, never shown the sign-in page.

Measured 2026-11-14 21:12:49: the guard logged jeff, with a session 59 minutes old, at
/_auth/?next=<the Clearing>, and showed him the sign-in page anyway; he signed in again.
Loads the WHOLE door module with its state in a temp dir (no IdP, no model door, no running
guard) and asks its real signed_in_landing. Each case prints PASS or FAIL; exit 1 on any FAIL.
Usage: signed-in-landing-4966.py [path-to-chorus-share-guard.py]
"""
import os
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
GUARD = sys.argv[1] if len(sys.argv) > 1 else os.path.join(HERE, "..", "..", "scripts", "chorus-share-guard.py")
TMP = tempfile.mkdtemp(prefix="signed-in-landing-4966-")
os.environ.update({
    "SHARE_STATE_FILE": os.path.join(TMP, "state.json"),
    "SHARE_PERSON_SESSIONS": os.path.join(TMP, "sessions.json"),
    "SHARE_ALLOW": "/about/",
    "SHARE_OIDC_OFFLINE": "1",
})
sys.path.insert(0, os.path.join(os.path.dirname(GUARD), "lib"))

g = {"__name__": "signed-in-landing-4966", "__file__": GUARD}
exec(compile(open(GUARD).read(), GUARD, "exec"), g)

FAILS = []
ROOM = "https://" + g["RETURN_HOST_SUFFIX"] + "/clearing-v2/"


def case(name, got, want):
    ok = got == want
    print(("PASS " if ok else "FAIL ") + name + ("" if ok else f": got {got!r}, want {want!r}"))
    if not ok:
        FAILS.append(name)


landing = g.get("signed_in_landing")
if landing is None:
    print("FAIL the door has no signed_in_landing")
    sys.exit(1)

case("a good session is sent on to where it was going", landing("ok", ROOM), ROOM)
case("a good session going to a path on this door is sent there", landing("ok", "/athena/model.html"), "/athena/model.html")
# the sign-in page (None) for every state that is not a good session
for state in ("anonymous", "logged-out", "not-authorized"):
    case(f"{state} gets the sign-in page, not a redirect", landing(state, ROOM), None)
# a signed-in caller is never bounced to another site
case("a good session with a foreign next lands on this door's default, not there",
     landing("ok", "https://evil.example/clearing-v2/"), g["pub"](g["LANDING"]))
case("a good session with a //host next lands on the default", landing("ok", "//evil.example/x"), g["pub"](g["LANDING"]))

# Silas 2026-11-15: a crafted link must not sign a signed-in person out, or loop on this page
HOME = g["pub"](g["LANDING"])
for nxt in ("/_auth/logout", "/_auth/", "/_auth", "/_auth/login?next=/x",
            "https://chorus." + g["RETURN_HOST_SUFFIX"] + "/_auth/logout",
            "https://" + g["RETURN_HOST_SUFFIX"] + "/chorus/_auth/logout"):
    case(f"a good session with next={nxt} lands on the default, not on the door's own routes", landing("ok", nxt), HOME)
# NEGATIVE PROOF: a path that only looks like it is still sent on
case("a good session going to /authors is sent there", landing("ok", "/authors"), "/authors")

# gate-code 2026-11-16, cases by Silas: what a browser or a header reads differently from us
safe = g["safe_return"]
CRLF = "/x\r\nSet-Cookie: a=b"          # how `/x%0d%0aSet-Cookie:...` arrives after the query is decoded
for name, nxt in (("a line break (header injection)", CRLF),
                  ("an encoded line break", "/x%0d%0aSet-Cookie:%20a=b"),
                  ("a backslash host", "/\\evil.example/x"),
                  ("an encoded backslash host", "/%5cevil.example/x"),
                  ("a double slash host", "//evil.example/x"),
                  ("an encoded double slash host", "%2f%2fevil.example/x"),
                  ("a tab", "/x\ty")):
    case(f"safe_return refuses {name}", safe(nxt), HOME)
    case(f"and a good session is not sent there ({name})", landing("ok", nxt), HOME)
case("no refused form puts a control character in the target",
     all(not any(ord(c) < 0x20 for c in safe(n)) for n in (CRLF, "/x%0d%0a", "/a\x00b")), True)
case("a good session with next=/_auth%2Flogout (encoded) lands on the default, not on sign-out", landing("ok", "/_auth%2Flogout"), HOME)
# NEGATIVE PROOF: ordinary targets still pass, unchanged
for nxt in ("/athena/model.html", "/clearing-v2/?room=team&x=1", "/a%20b", ROOM):
    case(f"an ordinary target passes unchanged: {nxt}", safe(nxt), nxt)

# The /_auth/ route itself (gate-quality, run 2: testing only signed_in_landing lets a route that
# ignores its answer survive). The real handler, with only its session read and its outputs stubbed.
import urllib.parse as _up


def at_auth(state, nxt):
    h = object.__new__(g["Guard"])
    h.path = g["AUTH_PREFIX"] + "?next=" + _up.quote(nxt, safe="")
    out = {}
    h._session_claims = lambda: {}
    h._renewal = lambda claims: (state, "renewed=1" if state == "ok" else None)
    h._redirect = lambda location, cookie=None: out.update(redirect=location, cookie=cookie)
    h._sign_in_page = lambda n: out.update(page=n) or True
    h._auth_routes()
    return out


case("the route sends a good session on, with its renewed cookie", at_auth("ok", ROOM), {"redirect": ROOM, "cookie": "renewed=1"})
case("the route never sends a good session to a line-break target", at_auth("ok", CRLF), {"redirect": HOME, "cookie": "renewed=1"})
# NEGATIVE PROOF: with no session, or one that has run out, the route shows the sign-in page and redirects nowhere
for st in ("none", "expired"):
    case(f"the route shows the sign-in page to a {st} session", at_auth(st, ROOM), {"page": ROOM})

sys.exit(1 if FAILS else 0)
