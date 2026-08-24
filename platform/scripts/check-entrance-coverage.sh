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
