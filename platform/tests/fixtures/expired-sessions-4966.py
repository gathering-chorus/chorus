"""#4966 AC6 — at his sign-in, his own expired browser sessions are closed, as him.

Measured at the door 2026-11-15 21:16: 123 of jeff's browser Session rows are open with an
expiresAt that has passed (found on #4920), 1 is live. The guard used to close an old session
with that session's own long-expired token ("token expired, no refresh"), so they stayed open.
Jeff, 13:27 "how do we clean it up?", 13:34 "yes": the door uses his NEW sign-in, as him.

Drives platform/scripts/lib/person_session.py with a stub door: no network, no store.
Each case prints PASS or FAIL; exit 1 on any FAIL.
Usage: expired-sessions-4966.py [path-to-person_session.py's directory]
"""
import os
import sys

LIB = sys.argv[1] if len(sys.argv) > 1 else os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..", "scripts", "lib")
sys.path.insert(0, LIB)
import person_session as ps  # noqa: E402

FAILS = []
NOW = 1_794_780_000
FRESH = "fresh-sign-in-token"


def check(name, cond):
    print(("PASS " if cond else "FAIL ") + name)
    if not cond:
        FAILS.append(name)


def row(name, owner, channel, state, expires, **more):
    # the door serves unset fields as empty strings; they must not be sent back
    r = {"name": name, "label": f"{name} label", "ownedBy": owner, "actsAs": "role-jeff", "channel": channel,
         "sessionState": state, "issuedAt": ps.iso(NOW - 99999), "startedAt": ps.iso(NOW - 99999),
         "lastSeenAt": ps.iso(NOW - 99999), "expiresAt": expires, "hostAccount": "jeffbridwell",
         "tokenId": "tid", "endedAt": "", "endedBy": "", "status": "", "comment": ""}
    r.update(more)
    return r


PAST, PAST2, FUTURE = ps.iso(NOW - 3600), ps.iso(NOW - 86400), ps.iso(NOW + 3600)
ROWS = [
    row("jeff-browser-old-1", "principal-jeff", "browser", "open", PAST),
    row("jeff-browser-old-2", "principal-jeff", "browser", "open", PAST2),
    row("jeff-browser-live", "principal-jeff", "browser", "open", FUTURE),          # unexpired: stays open
    row("jeff-browser-new", "principal-jeff", "browser", "open", FUTURE),           # the sign-in just made
    row("mark-browser-old", "principal-marknakib", "browser", "open", PAST),        # another person's
    row("wren-pane-1", "principal-wren", "pane", "open", PAST),                     # an agent session (#4948)
    row("jeff-pane-odd", "principal-jeff", "pane", "open", PAST),                   # his, but not a browser
    row("jeff-browser-noexp", "principal-jeff", "browser", "open", ""),             # no expiry written
    row("jeff-browser-done", "principal-jeff", "browser", "closed", PAST),          # already closed
]


class Door:
    def __init__(self, rows, put_status=200, get_status=200):
        self.rows, self.put_status, self.get_status, self.calls = rows, put_status, get_status, []

    def __call__(self, method, url, bearer, body):
        self.calls.append((method, url, bearer, body))
        if method == "GET":
            return self.get_status, {"data": self.rows}
        return self.put_status, {}


if not hasattr(ps, "close_expired_sessions"):
    print("FAIL person_session has no close_expired_sessions")
    sys.exit(1)

logs = []
door = Door(ROWS)
n = ps.close_expired_sessions("jeff", FRESH, NOW, "jeff-browser-new", door, "http://door", logs.append)
puts = [c for c in door.calls if c[0] == "PATCH"]
closed = sorted(c[1].rsplit("/", 1)[-1] for c in puts)
check("his two expired open browser sessions are closed, and nothing else", closed == ["jeff-browser-old-1", "jeff-browser-old-2"])
check("it reports how many it closed", n == 2)
check("each is closed with his NEW sign-in as the bearer", all(c[2] == FRESH for c in puts))
check("each ends at its own expiresAt, the true end, not now",
      sorted(c[3]["endedAt"] for c in puts) == sorted([PAST, PAST2]) and all(c[3]["sessionState"] == "closed" for c in puts))
# Silas, measured 2026-11-15 18:36: the door refuses its own served row sent back (422, prefixed links)
check("the close is a PATCH of exactly the two fields that change, never the served row",
      all(sorted(c[3].keys()) == ["endedAt", "sessionState"] for c in puts))
check("no whole-row write is made at all", not [c for c in door.calls if c[0] == "PUT"])
check("the list is read once, unsigned (reads are open)", [c[0] for c in door.calls].count("GET") == 1)
# measured at the door 2026-11-15: this URL returns his 124 open browser rows; ?ownedBy=jeff returns 0 (Abby)
get = [c for c in door.calls if c[0] == "GET"][0]
check("the list asks the door for open browser sessions, with no owner filter the door would misread",
      get[1] == "http://door/v1/identity/sessions?endedAt.absent=true&channel=browser&limit=5000" and get[2] is None)

# NEGATIVE PROOFS, each on the stub door above
check("NEGATIVE: an unexpired earlier session of his stays open", "jeff-browser-live" not in closed)
check("NEGATIVE: the session just made is never closed, even if the clock says so", "jeff-browser-new" not in closed)
check("NEGATIVE: an agent (pane) session is never closed by expiry (#4948)", "wren-pane-1" not in closed and "jeff-pane-odd" not in closed)
check("NEGATIVE: another person's expired session is not his to close", "mark-browser-old" not in closed)
check("NEGATIVE: a row with no expiry, or already closed, is left alone", "jeff-browser-noexp" not in closed and "jeff-browser-done" not in closed)

# the new sign-in itself expired by a wrong clock must still be kept
door2 = Door([row("jeff-browser-new", "principal-jeff", "browser", "open", PAST)])
check("NEGATIVE: keep wins over an expiry on the new session", ps.close_expired_sessions("jeff", FRESH, NOW, "jeff-browser-new", door2, "http://door", logs.append) == 0)

# the door still holds a session whose store row says expired (a renewal that never reached the store)
door6 = Door([row("jeff-browser-held", "principal-jeff", "browser", "open", PAST), row("jeff-browser-old-9", "principal-jeff", "browser", "open", PAST)])
ps.close_expired_sessions("jeff", FRESH, NOW, {"jeff-browser-new", "jeff-browser-held"}, door6, "http://door", logs.append)
closed6 = sorted(c[1].rsplit("/", 1)[-1] for c in door6.calls if c[0] == "PATCH")
check("NEGATIVE: a session the door still holds live is never closed, though its row says expired", closed6 == ["jeff-browser-old-9"])
# an expiresAt that is not the store's time form is not judged
door7 = Door([row("jeff-browser-odd", "principal-jeff", "browser", "open", "yesterday"), row("jeff-browser-num", "principal-jeff", "browser", "open", 5)])
logs7 = []
check("NEGATIVE: an expiry that is not a time is left alone, and nothing raises",
      ps.close_expired_sessions("jeff", FRESH, NOW, "x", door7, "http://door", logs7.append) == 0)
check("and it says so: how many were left open, and why", any("2 open browser session(s) left open" in l for l in logs7))

# no token, no writes
door3 = Door(ROWS)
check("with no sign-in token nothing is read or written", ps.close_expired_sessions("jeff", "", NOW, "x", door3, "http://door", logs.append) == 0 and not door3.calls)

# a door that refuses the write: counted as not closed, said in the log, never raises
door4 = Door(ROWS, put_status=403)
logs.clear()
check("a refused close is not counted", ps.close_expired_sessions("jeff", FRESH, NOW, "jeff-browser-new", door4, "http://door", logs.append) == 0)
check("and the log says how many were not closed, and the door's status", any("2 NOT closed (HTTP 403)" in line for line in logs))

# a door that cannot be listed: nothing written, never raises
door5 = Door(ROWS, get_status=500)
check("a list that fails writes nothing", ps.close_expired_sessions("jeff", FRESH, NOW, "x", door5, "http://door", logs.append) == 0
      and not [c for c in door5.calls if c[0] in ("PUT", "PATCH")])

sys.exit(1 if FAILS else 0)
