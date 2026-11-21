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
        if method == "POST" and url.endswith("/v1/identity/logins"):
            return self.post_status, ({"name": "jeff-9f2c", "created": True} if self.post_status == 201 else {"error": "refused"})
        if method == "POST":
            return 200, {}
        return 200, {}


logs = []
door = Door()
rec = ps.open_session(JEFF, TOK, NOW, 12 * 3600, sparql_for("jeff"), door, "http://door", logs.append)
# #4517 AC6 — sign-in asks the door with HIS verified login; the door writes the row
posts = [c for c in door.calls if c[0] == "POST"]
check("sign-in asks the door once, at the logins route", len(posts) == 1 and posts[0][1] == "http://door/v1/identity/logins")
check("it asks with HIS token as the bearer, never a service credential", posts and posts[0][2] == TOK["id_token"])
check("it names only the channel and his role: owner, token id and expiry are the door's",
      posts and posts[0][3] == {"channel": "browser", "actsAs": "role-jeff"})
# #5026 NEGATIVE PROOF: a sign-in that names no role leaves an open row OpenSessionActsAsARole fails
check("NEGATIVE (#5026): the ask always names the role it acts as, never the channel alone",
      posts and posts[0][3] != {"channel": "browser"} and posts[0][3].get("actsAs") == "role-jeff")
check("NEGATIVE: it never writes a Session row itself", not [c for c in door.calls if c[0] in ("PUT", "PATCH") or c[1].endswith("/v1/identity/sessions")])
check("the record keeps the name the door chose, and what sign-out needs",
      rec and rec["row"]["name"] == "jeff-9f2c" and rec["refresh_token"] == "rt-1")

# #5026 — the record keeps the row's expiry, so the guard can drop an ended record and #4966's
# close-at-sign-in stops treating every old sid as live (4517 kept only the name: 50 rows stayed open)
check("the record keeps the row's expiresAt, the login's own expiry",
      rec and rec["row"].get("expiresAt") == ps.iso(NOW + TOK["expires_in"]))
check("NEGATIVE (#5026): a record past its expiry is pruned, one before it is kept",
      rec and ps.record_ended(rec, NOW + TOK["expires_in"]) and not ps.record_ended(rec, NOW))
check("a name-only record from before #5026 is pruned by its login's own exp, never kept forever",
      ps.record_ended({"row": {"name": "x"}, "exp": NOW}, NOW + 1) and not ps.record_ended({"row": {"name": "x"}, "exp": NOW + 60}, NOW)
      and not ps.record_ended({"row": {"name": "x"}}, NOW + 10**6))
touched = ps.touch_session(dict(rec, exp=NOW + 7200), NOW, None, Door(post_status=200), "http://door", lambda t: None, logs.append) if rec else None
check("a renewal moves the record's expiresAt with the row's", touched and touched["row"]["expiresAt"] == ps.iso(NOW + 7200))

# NEGATIVE PROOF (Silas 01:34): no verified login, no row
door_n, logs_n = Door(), []
nolog = ps.open_session(JEFF, dict(TOK, id_token=""), NOW, 3600, sparql_for("jeff"), door_n, "http://door", logs_n.append)
check("NEGATIVE: no login -> nothing asked, no row", nolog is None and not [c for c in door_n.calls if c[0] == "POST"])

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

# sign-out asks the door to end the row; the door stamps how it ended
door4 = Door()
ok = ps.close_session(rec, NOW + 60, door4, "http://door", lambda rt: None, logs.append)
outs = [c for c in door4.calls if c[0] == "POST"]
check("sign-out asks the door's logout for his row, by name only",
      ok and len(outs) == 1 and outs[0][1] == "http://door/v1/identity/logouts" and outs[0][3] == {"session": "jeff-9f2c"})

# NEGATIVE PROOF: an expired token with no refresh closes nothing
door5, logs5 = Door(), []
stale = dict(rec, exp=NOW - 10)
closed = ps.close_session(stale, NOW, door5, "http://door", lambda rt: None, logs5.append)
check("NEGATIVE: expired token, no refresh -> nothing asked, said in the log",
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
