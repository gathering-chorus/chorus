#!/usr/bin/env python3
"""share-door-inventory — #5039 AC2: every Chorus page opens behind the sign-in door.

Jeff, 2026-11-24: "i feel i must be able to see all pages via public url". Measured 11-25:
33 of 93 served pages were refused by the door's allow list even when signed in, and about 40
data paths that let-through pages fetch were refused, so pages opened empty.

This reads three things and prints every disagreement, one line each:
  1. every HTML page under platform/api/public (the tree chorus-api serves);
  2. config/share-allowlist.txt, matched with the door's OWN route() (cut out of
     chorus-share-guard.py and run, never re-written here);
  3. config/share-shut.txt: paths kept shut on purpose, each with its reason.

Rules:
  PAGE   a served page must be let through, or be named in the shut file.
  DATA   a /api/… or /designing/… path that a let-through page or script names must be let
         through, or be named in the shut file. A page whose data is refused does not look
         broken; it looks old (share-allowlist.txt, #3813).
  STALE  a shut-file line that the allow list lets through anyway is a contradiction.

Named limit: DATA reads path literals out of the page source. A path built at run time from
pieces is not seen; the paths the model door serves (/owl/…) are one allow-list prefix.

Exit 0 when there is nothing to report, 1 otherwise. `--list` prints the inventory instead.
"""
import argparse
import ast
import os
import re
import sys

HERE = os.path.dirname(os.path.realpath(__file__))
ROOT = os.path.dirname(os.path.dirname(HERE))
DATA_LITERAL = re.compile(r"""[`'"](/(?:api|designing)/[A-Za-z0-9_\-./]*)""")
SKIP_DIRS = {"vendor", "node_modules"}


def door_route(guard_path):
    """The door's own route(path, allow, default_upstream), taken from its source."""
    with open(guard_path) as fh:
        tree = ast.parse(fh.read())
    for node in tree.body:
        if isinstance(node, ast.FunctionDef) and node.name == "route":
            scope = {}
            exec(compile(ast.Module(body=[node], type_ignores=[]), guard_path, "exec"), scope)
            return scope["route"]
    sys.exit(f"share-door-inventory: FAILED — {guard_path} has no route(); the door changed and this check must follow it")


def read_lines(path, what):
    """[(first field, rest of line)] for each non-comment line; a missing or empty file is fatal."""
    try:
        with open(path) as fh:
            rows = [l.strip() for l in fh if l.strip() and not l.strip().startswith("#")]
    except OSError as e:
        sys.exit(f"share-door-inventory: FAILED — cannot read the {what} {path}: {e.strerror}")
    if not rows:
        sys.exit(f"share-door-inventory: FAILED — the {what} {path} has no entries")
    return [(r.split()[0], r[len(r.split()[0]):].strip()) for r in rows]


def served_files(public_dir):
    """Every .html and .js file under the served tree, as (url path, file path)."""
    if not os.path.isdir(public_dir):
        sys.exit(f"share-door-inventory: FAILED — no served tree at {public_dir}")
    out = []
    for dp, dirs, files in os.walk(public_dir):
        dirs[:] = [d for d in dirs if d not in SKIP_DIRS]
        for f in sorted(files):
            if f.endswith((".html", ".js")):
                full = os.path.join(dp, f)
                out.append(("/" + os.path.relpath(full, public_dir).replace(os.sep, "/"), full))
    return sorted(out)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--allowlist", default=f"{ROOT}/config/share-allowlist.txt")
    ap.add_argument("--shut", default=f"{ROOT}/config/share-shut.txt")
    ap.add_argument("--public", default=f"{ROOT}/platform/api/public")
    ap.add_argument("--guard", default=f"{HERE}/chorus-share-guard.py")
    ap.add_argument("--list", action="store_true")
    a = ap.parse_args()

    route = door_route(a.guard)
    allow = [(p, rest or None) for p, rest in read_lines(a.allowlist, "allow list")]
    shut_rows = read_lines(a.shut, "shut list")
    no_reason = [p for p, why in shut_rows if not why]
    shut = [(p, None) for p, _ in shut_rows]
    opened = lambda p: route(p, allow, "x") is not None
    kept_shut = lambda p: route(p, shut, "x") is not None

    files = served_files(a.public)
    pages = [u for u, _ in files if u.endswith(".html")]
    if not pages:
        sys.exit(f"share-door-inventory: FAILED — no pages found under {a.public}")

    problems = []
    for p in no_reason:
        problems.append(f"SHUT-NO-REASON {p} — every shut line says why")
    for u in pages:
        if not opened(u) and not kept_shut(u):
            problems.append(f"PAGE {u} — served locally, refused by the door, and not named shut")
    named = {}
    for u, full in files:
        if not opened(u):
            continue
        with open(full, errors="ignore") as fh:
            for m in DATA_LITERAL.findall(fh.read()):
                named.setdefault(m.rstrip("/"), []).append(u)
    for path in sorted(named):
        if path and not opened(path) and not kept_shut(path):
            problems.append(f"DATA {path} — named by {named[path][0]}, refused by the door, and not named shut")
    for p, _ in shut:
        if opened(p):
            problems.append(f"STALE {p} — named shut, yet the allow list lets it through")

    if a.list:
        for u in pages:
            print(("open  " if opened(u) else "shut  " if kept_shut(u) else "LOST  ") + u)
        return 0
    for line in problems:
        print(line)
    print(f"share-door-inventory: {len(pages)} pages, {sum(opened(u) for u in pages)} open, "
          f"{len(named)} data paths named, {len(problems)} problem(s)")
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main())
