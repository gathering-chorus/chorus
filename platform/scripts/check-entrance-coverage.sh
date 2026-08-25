#!/usr/bin/env bash
# check-entrance-coverage.sh (#4003) — every link the /chorus entrance renders
# must be carried by the governed allowlist, or this REDS naming the paths.
#
# Why an external check: the guard's own log cannot tell you this. Its refusals
# read `"GET /x" 404 -` identically whether the path is unlisted or the upstream
# has no such file, so a section going dark (the #4002 Borg case) stays invisible
# until a human clicks a dead tile. Coverage is measured from outside instead:
# rendered links vs the governed list.
#
# Why a check and not a wider allowlist: the entrance renders 77 links today,
# 37 of them root-level one-off pages that #3994/#4001 retire or re-home.
# Allowlisting whatever the page happens to render would invert the door — the
# PAGE would decide what is public — and would cement the graveyard the
# retirement is clearing. The governed file stays the authority; uncovered
# links surface here as the retirement worklist.
#
# Exit 0 = every rendered link is covered. Exit 1 = uncovered links, listed.
# Exit 2 = cannot measure (ui-pages unreachable / empty) — never a vacuous green.
set -uo pipefail
R="${CHORUS_ROOT:-/Users/jeffbridwell/CascadeProjects/chorus}"
ALLOW="${SHARE_ALLOW_FILE:-$R/config/share-allowlist.txt}"
UI="${UI_PAGES_URL:-http://localhost:3340/api/chorus/ui-pages}"

[ -f "$ALLOW" ] || { echo "entrance-coverage: allowlist missing at $ALLOW" >&2; exit 2; }

BODY="$(curl -sf --max-time 10 "$UI" 2>/dev/null)" || {
  echo "entrance-coverage: UNMEASURABLE — $UI unreachable (refusing vacuous green)" >&2; exit 2; }

# first field only — a line may carry an upstream after the prefix (#3767)
PREFIXES="$(sed 's/#.*//' "$ALLOW" | awk 'NF {print $1}')"

printf '%s' "$BODY" | PREFIXES="$PREFIXES" python3 -c '
import json, os, sys
prefixes = [p for p in os.environ["PREFIXES"].split("\n") if p.strip()]
try:
    d = json.load(sys.stdin)
except Exception as e:
    print(f"entrance-coverage: UNMEASURABLE — ui-pages is not JSON ({e})", file=sys.stderr)
    sys.exit(2)
rows = d.get("misc", []) + [x for v in d.get("claimed", {}).values()
                            if isinstance(v, list) for x in v]
hrefs = sorted({r["href"] for r in rows
                if isinstance(r, dict) and str(r.get("href", "")).startswith("/")})
def covered(h):
    # A bare "/" is the ROOT and only the root — the guard route() treats it as
    # an exact match (#3765). Treating it as a prefix here would have scored
    # every link covered and made this check vacuous: it reported 77/77 with
    # 37 links genuinely dead, the exact hollow-green shape #3734 forbids.
    return any(h == p or (p != "/" and h.startswith(p.rstrip("/") + "/"))
               for p in prefixes)
if not hrefs:
    print("entrance-coverage: UNMEASURABLE — zero links rendered", file=sys.stderr)
    sys.exit(2)
missing = [h for h in hrefs if not covered(h)]
print(f"entrance-coverage: {len(hrefs) - len(missing)}/{len(hrefs)} rendered links covered")
if missing:
    print(f"UNCOVERED ({len(missing)}) — retirement/re-home worklist (#3994/#4001):",
          file=sys.stderr)
    for h in missing:
        print(f"  {h}", file=sys.stderr)
    sys.exit(1)
'
LINK_RC=$?

# #4004 — ASSETS, not just links. The pass above reads ui-pages, a list of link
# rows, so anything a page pulls in that is not a declared link is invisible to
# it: a <script src>, a stylesheet, a fetch target. Wren found #4001 shipping
# /ui-inventory.js and /archive.html that the tunnel 404s — the script tag fails
# publicly and the entrance renders 2 tiles where localhost shows 10. The check
# said 77/77 while the page was broken, because it never looked at the page.
# So: fetch the entrance HTML and measure every same-origin path it references.
ENTRANCE="${ENTRANCE_URL:-http://localhost:3340/chorus}"
HTML="$(curl -sf --max-time 10 "$ENTRANCE" 2>/dev/null)" || {
  echo "entrance-coverage: UNMEASURABLE — $ENTRANCE unreachable (refusing vacuous green)" >&2
  exit 2; }

printf '%s' "$HTML" | PREFIXES="$PREFIXES" python3 -c '
import os, re, sys
prefixes = [p for p in os.environ["PREFIXES"].split("\n") if p.strip()]
html = sys.stdin.read()
if not html.strip():
    print("entrance-coverage: UNMEASURABLE — entrance returned an empty body", file=sys.stderr)
    sys.exit(2)
# Quoted same-origin paths that name a FILE — a script, a stylesheet, a data
# document. Navigation links are deliberately left to the ui-pages pass above,
# which has an authoritative list of them; re-measuring them from the HTML would
# re-report the same worklist through a blurrier lens and red on paths the first
# pass already governs. What only the HTML can tell us is what the page PULLS IN
# to render, which is the gap Wren found.
refs = sorted({m for m in re.findall(r"[\x27\"](/[A-Za-z0-9._/-]*\.[A-Za-z0-9]+)[\x27\"]", html)})
def covered(h):
    return any(h == p or (p != "/" and h.startswith(p.rstrip("/") + "/"))
               for p in prefixes)
if not refs:
    print("entrance-coverage: UNMEASURABLE — entrance references zero paths", file=sys.stderr)
    sys.exit(2)
missing = [h for h in refs if not covered(h)]
print(f"entrance-coverage: {len(refs) - len(missing)}/{len(refs)} referenced assets covered")
if missing:
    print(f"UNCOVERED ASSETS ({len(missing)}) — referenced by the entrance, not on the allowlist:",
          file=sys.stderr)
    for h in missing:
        print(f"  {h}", file=sys.stderr)
    sys.exit(1)
'
ASSET_RC=$?

# UNMEASURABLE beats a failure: a green half must never mask a half we could not read.
[ "$LINK_RC" = 2 ] || [ "$ASSET_RC" = 2 ] && exit 2
[ "$LINK_RC" = 0 ] && [ "$ASSET_RC" = 0 ] && exit 0
exit 1
