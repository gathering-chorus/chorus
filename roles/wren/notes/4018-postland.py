#!/usr/bin/env python3
"""#4018 post-land, through the door as wren (owner of both rows). Dry run by default; --apply writes.

Rewrites pulse and the Clearing in the product design template (4018-postland-designs.json) once
the model run has deployed product-design-4018.ttl (job, whyNow, outcomes, openBets). The door
keeps each replaced row as a version (#4102), so the old design stays walkable.

The old door saves consumes/consumesEvent as strings on a PUT (#4478), so they are left OUT of
the body and put back as links by Jeff's batch afterwards (4018-postland-jeff.sh). partOf rides
its own edge route. Every write is read back."""
import json, os, sys, subprocess, urllib.request
APPLY = "--apply" in sys.argv
HERE = os.path.dirname(os.path.abspath(__file__))
B = "http://127.0.0.1:3360/v1/products/products"
T = subprocess.run(["bash", "/Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/chorus-identity-token", "wren"],
                   capture_output=True, text=True, check=True).stdout.strip() if APPLY else ""
STAMPS = {"iri", "created", "creator", "modified", "contentHash", "provenance", "changedAt", "changedIn", "writeCount", "version"}
NOT_IN_BODY = {"ownedBy", "partOf", "consumes", "consumesEvent", "type"}

def call(m, path, body=None):
    r = urllib.request.Request(B + path, method=m, data=json.dumps(body).encode() if body is not None else None)
    r.add_header("Content-Type", "application/json")
    if T: r.add_header("Authorization", "Bearer " + T)
    try:
        with urllib.request.urlopen(r, timeout=30) as x: return x.status, json.loads(x.read() or b"{}")
    except urllib.error.HTTPError as e: return e.code, json.loads(e.read() or b"{}")

def bare(v):
    if isinstance(v, list): return [bare(x) for x in v]
    v = v.split(":")[-1]
    for p in ("value-stream-step-", "document-"):
        if v.startswith(p): return v[len(p):]
    return v

designs = {k: v for k, v in json.load(open(f"{HERE}/4018-postland-designs.json")).items() if not k.startswith("_")}
failed = False
for name, new in designs.items():
    c, row = call("GET", "/" + name)
    assert c == 200, (name, c)
    body = {k: v for k, v in row["data"].items() if k not in STAMPS and k not in NOT_IN_BODY and v not in (None, "")}
    body.update({k: bare(v) for k, v in row["links"].items() if k not in NOT_IN_BODY})
    body.update(new)
    print(f"{name}: {', '.join(sorted(new))}")
    if not APPLY: continue
    c, r = call("PUT", "/" + name, body)
    print(f"  PUT {c}", "" if c < 300 else r)
    if c >= 300: failed = True; continue
    if "partOf" not in call("GET", "/" + name)[1]["links"]:
        print("  partOf", call("POST", f"/{name}/partof", {"target": "chorus"})[0])
    got = call("GET", "/" + name)[1]
    missing = [k for k, v in new.items() if k != "hasDomain" and got["data"].get(k) != v]
    print("  read back:", "all fields as written" if not missing else f"DIFFERENT: {missing}", "| hasDomain", got["links"].get("hasDomain"))
    failed |= bool(missing)
print("next: Jeff runs 4018-postland-jeff.sh to put the consumes links back" if APPLY and not failed else "")
sys.exit(1 if failed else 0)
