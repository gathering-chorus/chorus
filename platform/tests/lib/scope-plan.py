#!/usr/bin/env python3
"""#4022 — each role's live write scope, checked against the Permission rows.

The door publishes Permission rows (agent, mode, accessTo graph). Each class is
served from one graph. So the rows say, for every role and class, whether a
write should be let in. This compares that with what the door actually does.

  plan    <permissions.json> <class-graphs.json> <role>
          prints one line per class: "deny <Class>" for every class the rows
          refuse the role, then "allow <Class>" for ONE class they let it write.
          A class whose serving graph could not be read is "unknown" (red).
  expect  <class-graphs.json> <refused graph> ...
          the single API walk's plan (#4022): "deny" for every class on a graph
          the walking identity must be refused, "allow" for every other class.
  graphs  <api>
          prints {class: servedFrom graph} for every class the door serves.
  verdict <plan.txt> <runner.out>
          reads the 4267 runner's lines for those classes and prints every class
          where the door disagrees with the rows. Exit 1 if any, 0 if none.

Wren, 2026-10-13: take the expected denials from the Permission rows and let it
go red; two sources disagreeing on who may write is the product bug.
"""
import json
import sys

WRITE = "http://www.w3.org/ns/auth/acl#Write"


def plan(perms, graphs, role):
    rows = perms.get("data", perms) if isinstance(perms, dict) else perms
    writable = {r["accessTo"] for r in rows
                if r.get("agent") == f"principal-{role}" and r.get("mode") == WRITE}
    known = {c: g for c, g in graphs.items() if g.startswith("urn:")}
    unknown = sorted(c for c in graphs if c not in known)
    deny = sorted(c for c, g in known.items() if g not in writable)
    allow = sorted(c for c, g in known.items() if g in writable)
    return ([("unknown", c) for c in unknown] + [("deny", c) for c in deny]
            + [("allow", c) for c in allow[:1]])


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


def runner_results(lines):
    """class -> (verdict, detail) from the runner's `CLASS VERDICT detail` lines."""
    out = {}
    for line in lines:
        parts = line.split(None, 2)
        if len(parts) >= 2 and parts[1] in ("PASS", "FAIL", "NOT-PERM", "UNMEASURED"):
            out[parts[0]] = (parts[1], parts[2].strip() if len(parts) > 2 else "")
    return out


def verdict(planned, results):
    """Every class where the door and the rows disagree, as readable lines."""
    wrong = []
    for want, cls in planned:
        if want == "unknown":
            wrong.append(f"{cls}: the door did not say which graph serves it, so its scope is unmeasured")
            continue
        got, detail = results.get(cls, ("MISSING", "the runner printed no line"))
        if "door-managed" in detail:
            continue  # no principal may write it, either way; not a scope question
        if want == "deny":
            if got == "NOT-PERM":
                continue
            wrong.append(f"{cls}: rows deny, door answered {got} {detail[:80]}".rstrip())
        elif got != "PASS":
            wrong.append(f"{cls}: rows allow, door answered {got} {detail[:80]}".rstrip())
    return wrong


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
    if len(argv) == 4 and argv[1] == "verdict":
        planned = [tuple(l.split()) for l in open(argv[2]) if l.strip()]
        wrong = verdict(planned, runner_results(open(argv[3])))
        for w in wrong:
            print(w)
        print(f"{len(planned)} classes checked · {len(wrong)} where the door and the Permission rows disagree")
        return 1 if wrong else 0
    print(__doc__, file=sys.stderr)
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv))
