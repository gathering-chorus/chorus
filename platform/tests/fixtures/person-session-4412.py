"""#4412 — the shared door writes his sign-in as a Session row, as him.

Drives platform/scripts/lib/person_session.py with stub I/O: no network, no
store. Each case prints PASS or FAIL; exit 1 on any FAIL.
"""
import base64
import json
import os
import sys

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..", "scripts", "lib"))
import person_session as ps  # noqa: E402

FAILS = []
JEFF = "https://id.lightlifeurbangardens.com/jeff/profile/card#me"
# A fake token built here (header.payload.sig, payload {"jti":"tok-1"}), so no
# token-shaped literal sits in the repo.
FAKE_JWT = ".".join(["h", base64.urlsafe_b64encode(json.dumps({"jti": "tok-1"}).encode()).decode().rstrip("="), "s"])
TOK = {"id_token": FAKE_JWT, "refresh_token": "rt-1", "expires_in": 3600}
NOW = 1_791_700_000


def check(name, cond):
    print(("PASS " if cond else "FAIL ") + name)
    if not cond:
        FAILS.append(name)


def sparql_for(owner, host="jeffbridwell"):
    def q(_query):
        if owner is None:
            return {"results": {"bindings": []}}
        b = {"p": {"value": f"https://jeffbridwell.com/chorus#principal-{owner}"}}
        if host:
            b["h"] = {"value": host}
        return {"results": {"bindings": [b]}}
    return q


class Door:
    def __init__(self, post_status=201):
        self.calls, self.post_status = [], post_status

    def __call__(self, method, url, bearer, body):
        self.calls.append((method, url, bearer, body))
        if method == "GET":
            return 200, {"data": [
                {"name": "silas-pane-1", "actsAs": "role-silas", "sessionState": "open"},
                {"name": "kade-pane-0", "actsAs": "role-kade", "sessionState": "closed"},
                {"name": "jeff-browser-0", "actsAs": "jeff", "sessionState": "open"}]}
        if method == "POST":
            return self.post_status, ({"name": body["name"]} if self.post_status == 201 else {"error": "refused"})
        return 200, {}


logs = []
door = Door()
rec = ps.open_session(JEFF, TOK, NOW, 12 * 3600, sparql_for("jeff"), door, "http://door", logs.append)
posts = [c for c in door.calls if c[0] == "POST"]
check("sign-in writes one Session row", len(posts) == 1)
row = posts[0][3] if posts else {}
check("the row is written with HIS token as the bearer", posts and posts[0][2] == TOK["id_token"])
check("the row is his: ownedBy and actsAs jeff, browser, open",
      row.get("ownedBy") == "jeff" and row.get("actsAs") == "jeff"
      and row.get("channel") == "browser" and row.get("sessionState") == "open")
check("it binds only the open ROLE sessions", row.get("binds") == ["silas-pane-1"])
check("it names the token, never holds it", row.get("tokenId") == "tok-1" and TOK["id_token"] not in str(row))
check("it carries the hostAccount the shape requires", row.get("hostAccount") == "jeffbridwell")
check("it lasts as long as the door's cookie", row.get("expiresAt") == ps.iso(NOW + 12 * 3600))
check("the record keeps what sign-out needs", rec and rec["row"]["name"] == row["name"] and rec["refresh_token"] == "rt-1")

# NEGATIVE PROOF: a WebID no Principal owns writes nothing
door2, logs2 = Door(), []
none = ps.open_session(JEFF, TOK, NOW, 3600, sparql_for(None), door2, "http://door", logs2.append)
check("NEGATIVE: no Principal for the WebID -> no row, said in the log",
      none is None and not [c for c in door2.calls if c[0] == "POST"] and "no Principal" in " ".join(logs2))

# NEGATIVE PROOF: a Principal with no hostAccount writes nothing (the shape would refuse it)
door_h, logs_h = Door(), []
noh = ps.open_session(JEFF, TOK, NOW, 3600, sparql_for("jeff", host=None), door_h, "http://door", logs_h.append)
check("NEGATIVE: no hostAccount -> no row, said in the log",
      noh is None and not [c for c in door_h.calls if c[0] == "POST"] and "no hostAccount" in " ".join(logs_h))

# NEGATIVE PROOF: a refused write returns nothing and never raises
door3, logs3 = Door(post_status=403), []
refused = ps.open_session(JEFF, TOK, NOW, 3600, sparql_for("jeff"), door3, "http://door", logs3.append)
check("NEGATIVE: a refused write stores nothing and says so",
      refused is None and "NOT written: HTTP 403" in " ".join(logs3))

# sign-out closes the row
door4 = Door()
ok = ps.close_session(rec, NOW + 60, door4, "http://door", lambda rt: None, logs.append)
puts = [c for c in door4.calls if c[0] == "PUT"]
check("sign-out PUTs his row closed with endedAt",
      ok and len(puts) == 1 and puts[0][3]["sessionState"] == "closed" and puts[0][3]["endedAt"] == ps.iso(NOW + 60))

# NEGATIVE PROOF: an expired token with no refresh closes nothing
door5, logs5 = Door(), []
stale = dict(rec, exp=NOW - 10)
closed = ps.close_session(stale, NOW, door5, "http://door", lambda rt: None, logs5.append)
check("NEGATIVE: expired token, no refresh -> no PUT, said in the log",
      closed is False and not door5.calls and "could not close" in " ".join(logs5))

# the guard's fixed client identity
doc = ps.client_document("https://chorus.example/_auth/clientid.jsonld",
                         ["https://a/_auth/callback", "https://b/_auth/callback", "https://a/_auth/callback"],
                         ["https://a/"])
check("the client document is a public client naming itself",
      doc["client_id"] == "https://chorus.example/_auth/clientid.jsonld"
      and doc["token_endpoint_auth_method"] == "none" and "offline_access" in doc["scope"])
check("it lists every callback once", doc["redirect_uris"] == ["https://a/_auth/callback", "https://b/_auth/callback"])

sys.exit(1 if FAILS else 0)
