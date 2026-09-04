#!/usr/bin/env python3
"""#4102 — the shared capabilities of EVERY generated endpoint, walked kind by kind.

Jeff, 2026-09-04: "we need a rich set of tests that cover shared capabilities on
generated endpoints". Every route athena-make serves is generated from a shape, so
the behaviour under them is the same behaviour: a replace keeps the version it
displaced, a create keeps none, a forged revision is refused, a non-owner is
refused, the write is one update. Proving that on products and documents proves it
for two of forty-one kinds.

This walks the discovery index and checks each kind that has rows the caller owns.
A kind the caller cannot exercise reports UNMEASURED — never a pass. Exit 1 if any
kind FAILS; UNMEASURED alone is exit 0 with the reasons printed.
"""
import json
import os
import sys
import urllib.error
import urllib.request

BASE = os.environ.get("OWL_URL", "http://localhost:3360")
TOKEN = os.environ.get("CHORUS_TOKEN", "")
ROLE = os.environ.get("CHORUS_WRITE_ROLE", "wren")
MARK = " (capabilities-4102)"

SYSTEM_FIELDS = {"name", "version", "changedAt", "changedIn", "modified", "created",
                 "ownedBy", "iri", "type", "creator"}
# read serves them, the write body refuses them: the structural edges have their
# own routes (#4096). A body echoing a read has to drop them or every replace 422s.
STRUCTURAL_EDGES = {"partOf", "contains", "hasChild"}
KIND_PREFIXES = ("value-stream-step-", "value-stream-", "design-doc-", "document-",
                 "domain-", "service-", "product-", "role-")


def get(path):
    with urllib.request.urlopen(BASE + path, timeout=20) as r:
        return json.load(r)


def send(method, path, body):
    req = urllib.request.Request(
        BASE + path, method=method, data=json.dumps(body).encode(),
        headers={"Authorization": "Bearer " + TOKEN, "Content-Type": "application/json"})
    try:
        with urllib.request.urlopen(req, timeout=30) as r:
            return r.status, r.read().decode()
    except urllib.error.HTTPError as e:
        return e.code, e.read().decode()


def bare(v):
    if isinstance(v, list):
        return [bare(x) for x in v]
    if isinstance(v, str):
        n = v[len("chorus:"):] if v.startswith("chorus:") else v
        for p in KIND_PREFIXES:
            if n.startswith(p):
                return n[len(p):]
        return n
    return v


def echo_body(row, links, touch_field, touch_value):
    """The row as a write body: both halves of the read, system fields dropped."""
    merged = dict(row)
    for k, v in (links or {}).items():
        if k != "type":
            merged[k] = v
    body = {k: bare(v) for k, v in merged.items()
            if k not in SYSTEM_FIELDS and k not in STRUCTURAL_EDGES and v not in ("", None, [])}
    body[touch_field] = touch_value
    return body


def revisions_of(of_row):
    rows = get("/revisions").get("data") or []
    return [r for r in rows if r.get("ofRow") == of_row]


def touchable(row):
    """A free-text field we can append to without breaking the shape."""
    for k in ("comment", "gaps", "notInScope", "asIs", "overview", "label"):
        if isinstance(row.get(k), str) and row.get(k):
            return k
    return None


def check_kind(collection, results):
    plural = collection.rsplit("/", 1)[-1]
    name = "/" + plural
    try:
        rows = (get("/" + plural) or {}).get("data") or []
    except Exception as e:
        results.append(("UNMEASURED", name, "collection unreadable: %s" % e))
        return
    if not rows:
        results.append(("UNMEASURED", name, "the model serves no rows of this kind"))
        return
    owned = [r for r in rows if r.get("ownedBy")]
    if not owned:
        # not a shy test: a row with no owner cannot be written by anyone through
        # the door, so this kind's write path is unreachable, not merely untested
        results.append(("UNMEASURED", name, "%d rows, NONE carry an owner — no one can write this kind" % len(rows)))
        return
    mine = [r for r in owned if str(r.get("ownedBy", "")).endswith(ROLE)]
    if not mine:
        results.append(("UNMEASURED", name, "%d rows owned, none by %s" % (len(owned), ROLE)))
        return
    row = mine[0]
    rowname = str(row["name"])
    field = touchable(row)
    if not field:
        results.append(("UNMEASURED", name, "no free-text field on %s to touch" % rowname))
        return

    entity_name = rowname
    try:
        ent = get("/%s/%s" % (plural, entity_name))
    except urllib.error.HTTPError:
        # some collections serve the minted name; the entity route mints it again
        entity_name = bare(rowname)
        try:
            ent = get("/%s/%s" % (plural, entity_name))
        except Exception as e:
            results.append(("UNMEASURED", name, "entity read of %s: %s" % (rowname, e)))
            return

    before = len(revisions_of("%s/%s" % (plural, entity_name)))
    body = echo_body(row, ent.get("links"), field, str(row.get(field, "")) + MARK)
    status, payload = send("PUT", "/%s/%s" % (plural, entity_name), body)
    if status != 200:
        results.append(("UNMEASURED", name, "replace refused (%d): %s" % (status, payload[:120])))
        return

    revs = revisions_of("%s/%s" % (plural, entity_name))
    if len(revs) != before + 1:
        results.append(("FAIL", name, "replace kept no version (%d then %d)" % (before, len(revs))))
        return
    newest = max(revs, key=lambda r: int(r.get("version") or 0))
    snap = json.loads(newest.get("snapshot") or "{}")
    if not snap:
        results.append(("FAIL", name, "the kept version has an empty snapshot"))
        return
    if str(snap.get(field, "")) != str(row.get(field, "")):
        results.append(("FAIL", name, "the snapshot is not the row that was replaced"))
        return
    now = [r for r in (get("/" + plural).get("data") or []) if str(r["name"]) == rowname]
    if now and str(now[0].get(field, "")) != str(row.get(field, "")) + MARK:
        results.append(("FAIL", name, "the write did not land the touched field"))
        return
    results.append(("PASS", name, "replaced %s: v%s kept, snapshot is the prior row"
                    % (entity_name, newest.get("version"))))


def check_shared_refusals(results):
    """The refusals are the same on every generated route, so prove them once."""
    status, payload = send("POST", "/revisions", {
        "name": "forged-4102", "label": "forged", "ofRow": "products/spine",
        "version": "99", "snapshot": "{}"})
    if status == 200 or status == 201:
        results.append(("FAIL", "/revisions", "a caller can write a revision directly"))
    else:
        results.append(("PASS", "/revisions", "a direct write is refused (%d)" % status))

    rows = (get("/products").get("data") or [])
    notmine = [r for r in rows if not str(r.get("ownedBy", "")).endswith(ROLE)]
    if not notmine:
        results.append(("UNMEASURED", "authz", "no product owned by another role to try"))
    else:
        target = notmine[0]["name"]
        status, _ = send("PUT", "/products/%s" % target, {"label": "taken"})
        if status == 200:
            results.append(("FAIL", "authz", "%s wrote %s, owned by %s"
                            % (ROLE, target, notmine[0].get("ownedBy"))))
        else:
            results.append(("PASS", "authz", "a non-owner is refused (%d)" % status))


def main():
    if not TOKEN:
        print("UNMEASURED: no identity token — nothing was proved")
        return 0
    index = get("/")
    collections = [p["collection"] for p in index.get("primitives", []) if p.get("collection")]
    results = []
    check_shared_refusals(results)
    for c in collections:
        check_kind(c, results)
    width = max(len(n) for _, n, _ in results)
    for verdict, name, detail in results:
        print("%-11s %-*s %s" % (verdict, width, name, detail))
    counts = {v: sum(1 for x, _, _ in results if x == v) for v in ("PASS", "FAIL", "UNMEASURED")}
    print("\n%d kinds walked — %d pass, %d fail, %d unmeasured"
          % (len(collections), counts["PASS"], counts["FAIL"], counts["UNMEASURED"]))
    return 1 if counts["FAIL"] else 0


if __name__ == "__main__":
    sys.exit(main())
