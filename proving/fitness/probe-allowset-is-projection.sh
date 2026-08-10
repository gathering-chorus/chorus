#!/usr/bin/env bash
# ROW 8 — the enforced allow-set IS the declared one. (#3776, ADR-057)
#
# WHY THIS ROW EXISTS
#
# Four copies of "who may enter" diverged by 2026-08-10 (flow-probe in one
# graph and not another; a hand file still refusing someone the model admits).
# The fix is projection — the guard's file renders from the model — and this
# row is what keeps it fixed: it re-renders the projection and diffs it against
# the file the guard actually reads. A hand edit, a stale render, or a second
# opinion anywhere in that file is a red line on the score page, not a
# discovery during an incident.
#
# Verdicts: 0 = file matches the model's projection; 1 = it does not (names the
# diff); 2 = could not render (riot missing / source unparseable) — UNMEASURABLE,
# never a pass.
set -u

CHORUS_ROOT="${CHORUS_ROOT:-/Users/jeffbridwell/CascadeProjects/chorus}"
RENDER="$CHORUS_ROOT/platform/scripts/render-share-principals.sh"
LIVE="${SHARE_PRINCIPALS_FILE:-$CHORUS_ROOT/config/share-principals.txt}"
SRC="$CHORUS_ROOT/roles/silas/ontology/identity-principals-3613.ttl"

PROVE_RED=0
[ "${1:-}" = "--prove-red" ] && PROVE_RED=1

[ -x "$RENDER" ] || { echo "row8 UNMEASURABLE — renderer missing at $RENDER" >&2; exit 2; }
[ -r "$LIVE" ] || { echo "row8 UNMEASURABLE — enforced file missing at $LIVE" >&2; exit 2; }

FRESH=$(mktemp)
trap 'rm -f "$FRESH"' EXIT
if ! "$RENDER" "$SRC" "$FRESH" >/dev/null 2>&1; then
  echo "row8 UNMEASURABLE — projection did not render; cannot compare" >&2
  exit 2
fi

# Compare CONTENT, not comments: strip comment/blank lines from both sides and
# sort, so a reworded header can never mask (or fake) a membership change.
members() { grep -v '^\s*#' "$1" | grep -v '^\s*$' | sort; }

if [ "$PROVE_RED" -eq 1 ]; then
  # THE VIOLATION, CONSTRUCTED: a hand-added webId in the enforced file — the
  # exact edit this row exists to catch. Run the same comparison against it.
  VIOLATED=$(mktemp)
  cp "$LIVE" "$VIOLATED"
  echo "https://id.lightlifeurbangardens.com/hand-added-by-nobody/profile/card#me" >> "$VIOLATED"
  if diff -q <(members "$VIOLATED") <(members "$FRESH") >/dev/null 2>&1; then
    rm -f "$VIOLATED"
    echo "NEGATIVE PROOF FAILED — a hand-added webId was not distinguishable from the projection." >&2
    exit 1
  fi
  rm -f "$VIOLATED"
  echo "NEGATIVE PROOF OK — a hand-added webId makes the comparison fail."
  exit 0
fi

if DIFF=$(diff <(members "$LIVE") <(members "$FRESH")); then
  echo "row8 GREEN — the allow-set the guard enforces is exactly the model's projection ($(members "$LIVE" | wc -l | tr -d ' ') webids)."
  exit 0
else
  echo "row8 RED — the enforced allow-set is NOT the model's projection. The drift, exactly:"
  printf '%s\n' "$DIFF" | sed 's/^</  enforced-only: /; s/^>/  model-only:   /' | grep -v '^---'
  exit 1
fi
