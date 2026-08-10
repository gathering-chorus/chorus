#!/usr/bin/env bash
# ROW 9 — the page shows who the door lets in. (#3785)
#
# WHY THIS ROW EXISTS
#
# On 2026-08-10 owl-api served /principals with 10 records while the doors
# enforced 11. Neither side was "broken": the store was correct and the doors
# read it live. The gap was that owl-api RESOLVED the principals graph at its
# Jul-30 boot and never re-read the model after it repointed — so the served
# page and the enforced door disagreed, and every check that read either side
# ALONE stayed green. That is merged-is-not-running at the MODEL layer, and the
# only thing that catches it is comparing the two SURFACES against each other.
#
# Deliberately cross-surface, never the store twice (both graph reads would
# agree while the running service disagreed with both):
#   SERVED   = owl-api /principals over HTTP — what the page tells a reader.
#   ENFORCED = the DOOR's live admitted set — config/share-principals.txt, the
#              file the guard consults per request (the #3776 projection). A real
#              door fact, resolved, NOT a hardcoded constant (Wren's row-4 lesson:
#              assume nothing, resolve it — or you blame stale-serve for a
#              correct, intended removal).
#
# The assertion: every identity the door admits must appear on the served page.
# The door's set is the smaller one (canSignIn principals); the page is a
# superset (all principals). A door-admitted webid MISSING from the page is the
# stale-serve drift. A webid removed from the door is not this row's concern —
# it simply drops from the enforced set, and its absence from the page is then
# correct, not a finding.
#
# Verdicts: 0 = every door-admitted identity is served; 1 = at least one is
# admitted but not served (page and door disagree); 2 = a surface could not be
# read, or the enforced set is empty — UNMEASURABLE, never a pass.
set -u

OWL="${FITNESS_OWL_API:-http://localhost:3360}"
CHORUS_ROOT="${CHORUS_ROOT:-/Users/jeffbridwell/CascadeProjects/chorus}"
# The DOOR's admitted set, resolved from the file the guard actually reads.
ENFORCED_FILE="${FITNESS_ENFORCED_FILE:-$CHORUS_ROOT/config/share-principals.txt}"

PROVE_RED=0
[ "${1:-}" = "--prove-red" ] && PROVE_RED=1

# --- ENFORCED: the door's live admitted webids (comment/blank lines stripped).
[ -r "$ENFORCED_FILE" ] || { echo "row9 UNMEASURABLE — the door's allow-set file is unreadable at $ENFORCED_FILE" >&2; exit 2; }
enforced_set() { grep -v '^\s*#' "$ENFORCED_FILE" | grep -v '^\s*$' | sed 's/^\s*//;s/\s*$//'; }
ENFORCED="$(enforced_set)"
[ -n "$ENFORCED" ] || { echo "row9 UNMEASURABLE — the door admits NO ONE (empty allow-set) — indistinguishable from a correct refusal of everyone" >&2; exit 2; }

# --- SERVED: the webId FIELD of every /principals record, over HTTP. Narrowed to
# the field (Wren's non-blocking note) — a webid in a comment/provenance string
# must not count as served.
SERVED_JSON=$(curl -s --max-time 15 "$OWL/principals" 2>/dev/null) || {
  echo "row9 UNMEASURABLE — owl-api /principals did not answer at $OWL" >&2; exit 2; }
SERVED_WEBIDS=$(printf '%s' "$SERVED_JSON" | python3 -c 'import json,sys
try:
    d=json.load(sys.stdin); rows=d.get("data", d if isinstance(d,list) else [])
except Exception:
    sys.exit(3)
if rows is None: sys.exit(3)
for r in rows:
    w=r.get("webId") if isinstance(r,dict) else None
    if w: print(w)') || { echo "row9 UNMEASURABLE — /principals did not return a parseable collection" >&2; exit 2; }
[ -n "$SERVED_WEBIDS" ] || { echo "row9 UNMEASURABLE — /principals served ZERO webids — a door reading empty is indistinguishable from a correct refusal" >&2; exit 2; }

# THE verdict path, over the two resolved surfaces. Injectable enforced set so
# the negative proof drives THIS EXACT CODE with a violating input.
verdict() {  # verdict <enforced-lines>; echoes RED lines, returns 0 all-served / 1 drift
  local missing="" w
  while IFS= read -r w; do
    [ -n "$w" ] || continue
    if ! grep -Fxq -- "$w" <<<"$SERVED_WEBIDS"; then
      missing="$missing$w"$'\n'
    fi
  done <<<"$1"
  if [ -n "$missing" ]; then printf '%s' "$missing"; return 1; fi
  return 0
}

if [ "$PROVE_RED" -eq 1 ]; then
  # Drive the REAL verdict path, both directions (Wren's blocker 1):
  #   A) a violating enforced set — a door-admitted webid ABSENT from served —
  #      MUST take the RED branch (return 1).
  #   B) a clean enforced set — only webids that ARE served — MUST pass (0).
  # Same verdict() the live run calls; not a re-implementation.
  SYNTH="https://id.lightlifeurbangardens.com/enforced-not-served-proof/profile/card#me"
  if verdict "$SYNTH" >/dev/null; then
    echo "NEGATIVE PROOF FAILED — an admitted-but-not-served webid did NOT trip the RED branch." >&2
    exit 1
  fi
  CONTROL=$(printf '%s\n' "$SERVED_WEBIDS" | head -1)
  if ! verdict "$CONTROL" >/dev/null; then
    echo "NEGATIVE PROOF FAILED — a served webid was wrongly reported as a drift (false positive)." >&2
    exit 1
  fi
  echo "NEGATIVE PROOF OK — an admitted-but-not-served identity trips RED; a served one passes. Both directions through verdict()."
  exit 0
fi

if MISSING=$(verdict "$ENFORCED"); then
  echo "row9 GREEN — every door-admitted identity appears on the served /principals page ($(printf '%s\n' "$SERVED_WEBIDS" | wc -l | tr -d ' ') served, $(printf '%s\n' "$ENFORCED" | wc -l | tr -d ' ') enforced)."
  exit 0
else
  echo "row9 RED — SERVED != ENFORCED: the door admits identities the page does not list, so the page says they cannot sign in while the door lets them in (#3785 stale-serve):"
  printf '%s\n' "$MISSING" | sed 's/^/  admitted-but-not-served: /'
  exit 1
fi
