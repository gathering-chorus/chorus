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

sys.exit(1 if FAILS else 0)
