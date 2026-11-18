#!/usr/bin/env python3
"""#4022 — each role's live write scope, checked against the Permission rows.

The door publishes Permission rows (agent, mode, accessTo graph). Each class is
served from one graph. So the rows say, for every role and class, whether a
write should be let in. This compares that with what the door actually does.

  plan    <permissions.json> <class-graphs.json> <role>
          prints one line per class: "deny <Class>" for every class the rows
          refuse the role, "append <Class>" for every class on a graph the role
          holds acl:Append on, then "allow <Class>" for every class it may write.
          A class whose serving graph could not be read is "unknown" (red).
  expect  <class-graphs.json> <refused graph> ...
          the single API walk's plan (#4022): "deny" for every class on a graph
          the walking identity must be refused, "allow" for every other class.
  graphs  <api>
          prints {class: servedFrom graph} for every class the door serves.
  collections <api>
          prints "<Class> <collection path>" for every class the door serves.
  classify <Class> <http code> <body file>
          #4517 AC2 — reads the door's answer to ONE fixed, deliberately invalid
          create body: 403 = the door refuses, 422 = the door allows (the body
          was judged, so authz passed), anything else = unmeasured, named.
          Nothing is ever written. Prints a runner-format line for verdict.
  verdict <plan.txt> <runner.out> [door-written.txt]
          reads the 4267 runner's lines for those classes and prints every class
          where the door disagrees with the rows. Exit 1 if any, 0 if none.
          #5002 AC6 — a class in door-written.txt (one per line) must be refused
          whatever the rows grant: grants are per graph, the door-only rule per
          class. A door-written class the door lets a session write is red.
  door-written <shape.ttl> ...
          prints every class whose shape says chorus:writeAuthority "door".
  owned   <api>
          prints {class: true|false}: whether the class carries ownedBy (the
          door's own rule, row_owner_governed), read from each class's openapi.
  quartet <permissions.json> <class-graphs.json> <owned.json>
          #5002 AC6 (Jeff 11-18) — the quartet test principal holds Append, and
          Write only on graphs whose classes carry no ownedBy. Exit 1 if not.
          A NOT-PERM the door gives as door-managed or deploy-only (#4661) is
          not a scope question: no principal writes those classes.

Wren, 2026-10-13: take the expected denials from the Permission rows and let it
go red; two sources disagreeing on who may write is the product bug.
"""
import json
import sys

WRITE = "http://www.w3.org/ns/auth/acl#Write"
APPEND = "http://www.w3.org/ns/auth/acl#Append"


def plan(perms, graphs, role):
    rows = perms.get("data", perms) if isinstance(perms, dict) else perms
    mine = [r for r in rows if r.get("agent") == f"principal-{role}"]
    writable = {r["accessTo"] for r in mine if r.get("mode") == WRITE}
    # #4022 reopen 2026-10-14 — since #4517 an acl:Append row opens a create and
    # the caller's own row, never another owner's. Counting only Write read every
    # Append class as deny, and the door (rightly) let the create through: red 3/3.
    # Each Append class is walked: its own-row cycle must pass.
    appendable = {r["accessTo"] for r in mine if r.get("mode") == APPEND} - writable
    known = {c: g for c, g in graphs.items() if g.startswith("urn:")}
    unknown = sorted(c for c in graphs if c not in known)
    deny = sorted(c for c, g in known.items() if g not in writable | appendable)
    append = sorted(c for c, g in known.items() if g in appendable)
    allow = sorted(c for c, g in known.items() if g in writable)
    return ([("unknown", c) for c in unknown] + [("deny", c) for c in deny]
            + [("append", c) for c in append] + [("allow", c) for c in allow])


def expect(graphs, refused):
    known = {c: g for c, g in graphs.items() if g.startswith("urn:")}
    return ([("unknown", c) for c in sorted(set(graphs) - set(known))]
            + [("deny" if g in refused else "allow", c) for c, g in sorted(known.items())])


def fetch_graphs(api):
    import urllib.request
    out = {}
    for p in json.load(urllib.request.urlopen(api + "/", timeout=20))["primitives"]:
        try:
            out[p["kind"]] = json.load(urllib.request.urlopen(
                api + p["collection"] + "?limit=1", timeout=40)).get("servedFrom", "")
        except Exception:
            out[p["kind"]] = ""
    return out


def fetch_collections(api):
    import urllib.request
    return [(p["kind"], p["collection"]) for p in json.load(urllib.request.urlopen(api + "/", timeout=20))["primitives"]]


def classify(cls, code, body):
    """#4517 AC2 — the door's decision, read off its answer to an invalid body."""
    body = " ".join(body.split())[:200]
    if code == "403":
        return f"{cls} NOT-PERM {body}"
    if code == "422":
        return f"{cls} PASS door allows (422: the invalid body was judged, so authz let it in)"
    return f"{cls} UNMEASURED door answered {code or 'nothing'} {body}".rstrip()


def runner_results(lines):
    """class -> (verdict, detail) from the runner's `CLASS VERDICT detail` lines."""
    out = {}
    for line in lines:
        parts = line.split(None, 2)
        if len(parts) >= 2 and parts[1] in ("PASS", "FAIL", "NOT-PERM", "UNMEASURED"):
            out[parts[0]] = (parts[1], parts[2].strip() if len(parts) > 2 else "")
    return out


def verdict(planned, results, door_written=frozenset()):
    """Every class where the door and the rows disagree, as readable lines."""
    wrong = []
    for want, cls in planned:
        if cls in door_written:
            got, detail = results.get(cls, ("MISSING", "the runner printed no line"))
            if got != "NOT-PERM":
                wrong.append(f"{cls}: door-written class, door answered {got} {detail}".rstrip())
            continue
        if want == "unknown":
            wrong.append(f"{cls}: the door did not say which graph serves it, so its scope is unmeasured")
            continue
        got, detail = results.get(cls, ("MISSING", "the runner printed no line"))
        if "door-managed" in detail:
            continue  # no principal may write it, either way; not a scope question
        if got == "NOT-PERM" and '"deploy-only"' in detail:
            continue  # #4661 — written only by the model deploy (#4220): not a scope question either
        if got == "NOT-PERM" and detail.startswith("append-only on an ownerless class") and want in ("allow", "append"):
            continue  # #4915 — Append opens a create only; with no row owner nothing could clean it up
        if want == "deny":
            if got == "NOT-PERM":
                continue
            wrong.append(f"{cls}: rows deny, door answered {got} {detail}".rstrip())
        elif got != "PASS":
            rows = "rows append (own row)" if want == "append" else "rows allow"
            wrong.append(f"{cls}: {rows}, door answered {got} {detail}".rstrip())
    return wrong


def door_written(paths):
    """Classes whose shape carries chorus:writeAuthority "door" (subject chorus:<Class>Shape)."""
    import re
    out = set()
    for path in paths:
        subject = None
        for line in open(path):
            m = re.match(r"chorus:(\w+)Shape\b", line)
            if m:
                subject = m.group(1)
            if subject and re.search(r'chorus:writeAuthority\s+"door"', line):
                out.add(subject)
    return sorted(out)


def fetch_owned(api):
    import urllib.request
    out = {}
    for kind, coll in fetch_collections(api):
        try:
            spec = json.load(urllib.request.urlopen(api + coll + "/openapi.json", timeout=40))
            props = spec.get("components", {}).get("schemas", {}).get(kind, {}).get("properties", {})
            out[kind] = "ownedBy" in props
        except Exception:
            out[kind] = None  # unread: never counted as ownerless
    return out


def quartet(perms, graphs, owned):
    """Append held; Write only on graphs none of whose classes carry ownedBy."""
    rows = perms.get("data", perms) if isinstance(perms, dict) else perms
    mine = [r for r in rows if r.get("agent") == "principal-quartet"]
    bad = []
    if not any(r.get("mode") == APPEND for r in mine):
        bad.append("holds no Append row")
    for g in sorted({r.get("accessTo", "") for r in mine if r.get("mode") == WRITE}):
        classes = [c for c, cg in graphs.items() if cg == g]
        held = [c for c in classes if owned.get(c) is not False]
        if not classes or held:
            why = ", ".join(held) if held else "no class the door serves"
            bad.append(f"Write on {g}: {why} (owned or unread)")
    return bad


def main(argv):
    if len(argv) == 5 and argv[1] == "plan":
        for want, cls in plan(json.load(open(argv[2])), json.load(open(argv[3])), argv[4]):
            print(want, cls)
        return 0
    if len(argv) >= 3 and argv[1] == "expect":
        for want, cls in expect(json.load(open(argv[2])), set(argv[3:])):
            print(want, cls)
        return 0
    if len(argv) == 3 and argv[1] == "graphs":
        json.dump(fetch_graphs(argv[2]), sys.stdout)
        return 0
    if len(argv) == 3 and argv[1] == "collections":
        for cls, path in fetch_collections(argv[2]):
            print(cls, path)
        return 0
    if len(argv) == 5 and argv[1] == "classify":
        print(classify(argv[2], argv[3], open(argv[4]).read()))
        return 0
    if len(argv) >= 3 and argv[1] == "door-written":
        for c in door_written(argv[2:]):
            print(c)
        return 0
    if len(argv) == 3 and argv[1] == "owned":
        json.dump(fetch_owned(argv[2]), sys.stdout)
        return 0
    if len(argv) == 5 and argv[1] == "quartet":
        bad = quartet(json.load(open(argv[2])), json.load(open(argv[3])), json.load(open(argv[4])))
        print("quartet: " + ("; ".join(bad) or "Append held, Write only on ownerless graphs"))
        return 1 if bad else 0
    if len(argv) in (4, 5) and argv[1] == "verdict":
        planned = [tuple(l.split()) for l in open(argv[2]) if l.strip()]
        door = frozenset(l.strip() for l in open(argv[4]) if l.strip()) if len(argv) == 5 else frozenset()
        wrong = verdict(planned, runner_results(open(argv[3])), door)
        for w in wrong:
            print(w)
        print(f"{len(planned)} classes checked · {len(wrong)} where the door and the Permission rows disagree")
        return 1 if wrong else 0
    print(__doc__, file=sys.stderr)
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv))
