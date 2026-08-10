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
# The comparison is deliberately cross-surface, never the store twice (a check
# that reads the graph on both sides proves nothing here — both would agree
# while the running service disagreed with both):
#   SERVED   = owl-api /principals over HTTP — what the page tells a reader.
#   ENFORCED = a live DOOR admission — flow-probe signs in nightly, so it is a
#              known door-admitted identity; the page MUST list it.
#
# Verdicts: 0 = the door-admitted identity appears on the served page (and the
# served count is non-trivial); 1 = it does not — the page and the door
# disagree; 2 = a surface could not be reached — UNMEASURABLE, never a pass.
set -u

OWL="${FITNESS_OWL_API:-http://localhost:3360}"
# The identity we KNOW the live door admits (proven by the nightly sign-in flow,
# row 7's door-opens leg). Overridable so the fixture can name its own.
ENFORCED_WEBID="${FITNESS_ENFORCED_WEBID:-https://id.lightlifeurbangardens.com/flow-probe/profile/card#me}"

PROVE_RED=0
[ "${1:-}" = "--prove-red" ] && PROVE_RED=1

# SERVED, over HTTP — the page, not the graph.
SERVED=$(curl -s --max-time 15 "$OWL/principals" 2>/dev/null) || {
  echo "row9 UNMEASURABLE — owl-api /principals did not answer at $OWL" >&2; exit 2; }
COUNT=$(printf '%s' "$SERVED" | python3 -c 'import json,sys
try:
    d=json.load(sys.stdin); rows=d.get("data",d if isinstance(d,list) else [])
    print(len(rows))
except Exception: print(-1)')
[ "$COUNT" -ge 0 ] 2>/dev/null || { echo "row9 UNMEASURABLE — /principals did not return a parseable collection" >&2; exit 2; }
[ "$COUNT" -gt 0 ] || { echo "row9 UNMEASURABLE — /principals served ZERO — a door reading empty is indistinguishable from a correct refusal" >&2; exit 2; }

served_has() {  # served_has <webid> — does the served page carry this webid?
  printf '%s' "$SERVED" | python3 -c 'import json,sys
w=sys.argv[1]
try: d=json.load(sys.stdin); rows=d.get("data",d if isinstance(d,list) else [])
except Exception: sys.exit(3)
def vals(o):
    if isinstance(o,dict):
        for v in o.values(): yield from vals(v)
    elif isinstance(o,list):
        for v in o: yield from vals(v)
    else: yield str(o)
sys.exit(0 if any(w==v for r in rows for v in vals(r)) else 1)' "$1"
}

if [ "$PROVE_RED" -eq 1 ]; then
  # THE VIOLATION: an identity the door admits that the page does NOT list —
  # exactly the 2026-08-10 state (flow-probe enforced, not served). We name a
  # webid we know is NOT on the served page and assert the check catches the gap.
  ABSENT="https://id.lightlifeurbangardens.com/enforced-but-not-served-$$/profile/card#me"
  if served_has "$ABSENT"; then
    echo "NEGATIVE PROOF FAILED — a webid the page does not serve was reported as served." >&2
    exit 1
  fi
  echo "NEGATIVE PROOF OK — a door-admitted identity ABSENT from the served page makes the check fail (the 2026-08-10 stale-boot drift, reproduced)."
  exit 0
fi

if served_has "$ENFORCED_WEBID"; then
  echo "row9 GREEN — the door-admitted identity ($ENFORCED_WEBID) appears on the served /principals page; served ($COUNT) agrees with the door."
  exit 0
else
  echo "row9 RED — SERVED != ENFORCED: the door admits $ENFORCED_WEBID but owl-api /principals ($COUNT records) does not list it."
  echo "         The page tells a reader this identity cannot sign in while the door lets it in — the stale-served-graph drift (#3785)."
  exit 1
fi
