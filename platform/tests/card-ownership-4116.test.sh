#!/usr/bin/env bash
# card-ownership-4116.sh — #4116. Every card row carries the owner the model declares.
#
# WHY THIS EXISTS
# The write door is fail-closed on ownership (athena-make::authz_allows): a node
# whose chorus:ownedBy is absent cannot be written by ANYONE, including the land
# carrying the model statement of who owns it. On 2026-09-05 that stopped #4102
# mid-land — merged, then PUT /cards/3645 as wren -> 403, so the code moved and
# the rows did not. All 231 live card rows had no owner. A row minted without one
# is a row that can never be written again, so this counts them.
#
# THE CHECK IS COUNTED, NOT SAMPLED: every card row in the graph, and every card
# row the model declares an owner for, both sides reported as N/N.
#
# NEGATIVE PROOF (#3734): --self-test feeds the same scorer a fixture in which one
# row has no owner and one row has an owner the model does not agree with, and
# asserts the check goes RED on each. Without that, a scorer that only ever sees
# a green store cannot be shown to distinguish the two states it exists to separate.
set -u

ROOT="${CHORUS_ROOT:-/Users/jeffbridwell/CascadeProjects/chorus}"
QUERY="${FUSEKI_QUERY:-http://localhost:3030/pods/query}"
GRAPH="${CARD_GRAPH:-urn:chorus:instances}"
NS="https://jeffbridwell.com/chorus#"

# score <live-csv> <declared-csv>
#   live-csv:     card,owner   (owner empty when the row carries no chorus:ownedBy)
#   declared-csv: card,owner   (what the ontology files state)
# Prints the readout; returns 0 only when every live row has an owner AND every
# declared owner matches the live one.
score() {
  local live="$1" declared="$2" rc=0
  local total owned unowned
  total=$(grep -c . "$live" || true)
  owned=$(awk -F, 'NF>1 && $2!=""' "$live" | wc -l | tr -d ' ')
  unowned=$((total - owned))

  echo "live card rows      ${owned}/${total} carry an owner"
  local literals
  literals=$(awk -F, 'NF>2 && $3=="literal"' "$live" | wc -l | tr -d ' ')
  if [ "$literals" -gt 0 ]; then
    rc=1
    echo "RED — ${literals} row(s) store the owner as a bare literal; CardShape declares sh:class chorus:Role:"
    awk -F, 'NF>2 && $3=="literal" {print "    card-" $1 " ownedBy \"" $2 "\" (want chorus:role-" $2 ")"}' "$live" | head -10
  fi
  if [ "$unowned" -gt 0 ]; then
    rc=1
    echo "RED — ${unowned} row(s) carry no owner; the write door refuses every one of them:"
    awk -F, 'NF<2 || $2==""' "$live" | head -10 | sed 's/^/    /'
  fi

  local dtotal dmatch mismatch
  dtotal=$(grep -c . "$declared" || true)
  dmatch=0; mismatch=""
  while IFS=, read -r card owner; do
    [ -n "$card" ] || continue
    local liveowner
    liveowner=$(awk -F, -v c="$card" '$1==c {print $2}' "$live" | head -1)
    if [ "$liveowner" = "$owner" ]; then
      dmatch=$((dmatch + 1))
    else
      mismatch="${mismatch}    ${card}: model says '${owner}', live says '${liveowner:-<none>}'
"
    fi
  done < "$declared"
  echo "declared owners     ${dmatch}/${dtotal} match live"
  if [ "$dmatch" -ne "$dtotal" ]; then
    rc=1
    echo "RED — the model declares an owner the live row does not carry:"
    printf '%s' "$mismatch" | head -10
  fi
  return $rc
}

# ── negative proof ───────────────────────────────────────────────────────────
# Runs on EVERY invocation, not only behind a flag: it needs no store, so it is
# the half of this suite that can always be measured.
self_test() {
  tmp=$(mktemp -d); trap 'rm -rf "$tmp"' EXIT
  fail=0

  # (a) GREEN baseline — the scorer can pass, so a RED below means something.
  printf '3645,wren,iri\n3784,wren,iri\n' > "$tmp/live"
  printf '3645,wren\n' > "$tmp/declared"
  if score "$tmp/live" "$tmp/declared" >/dev/null; then
    echo "self-test: GREEN baseline passes ✓"
  else
    echo "self-test FAIL: a fully-owned, fully-matching store scored RED"; fail=1
  fi

  # (b) the state this check exists to catch: a row with no owner.
  printf '3645,wren,iri\n3784,,\n' > "$tmp/live"
  printf '3645,wren\n' > "$tmp/declared"
  if score "$tmp/live" "$tmp/declared" >/dev/null; then
    echo "self-test FAIL: a row with NO owner scored GREEN — the check is hollow"; fail=1
  else
    echo "self-test: unowned row scores RED ✓"
  fi

  # (c) the other half: live disagrees with the model.
  printf '3645,silas,iri\n' > "$tmp/live"
  printf '3645,wren\n' > "$tmp/declared"
  if score "$tmp/live" "$tmp/declared" >/dev/null; then
    echo "self-test FAIL: live owner contradicting the model scored GREEN"; fail=1
  else
    echo "self-test: model/live owner mismatch scores RED ✓"
  fi

  # (d) the literal-owner drift: an owner stored off-shape.
  printf '3645,wren,literal\n' > "$tmp/live"
  printf '3645,wren\n' > "$tmp/declared"
  if score "$tmp/live" "$tmp/declared" >/dev/null; then
    echo "self-test FAIL: a bare-literal owner scored GREEN — off-shape storage is invisible"; fail=1
  else
    echo "self-test: literal owner scores RED ✓"
  fi

  [ "$fail" -eq 0 ] && { echo "self-test PASS"; return 0; } || { echo "self-test FAIL"; return 1; }
}

self_test || exit 1
[ "${1:-}" = "--self-test" ] && exit 0

# ── the real run ─────────────────────────────────────────────────────────────
tmp=$(mktemp -d); trap 'rm -rf "$tmp"' EXIT

# ?own is the role name in either storage form; ?form says WHICH form, because
# CardShape declares sh:class chorus:Role — a bare literal owner is off-shape.
# The door writes the literal form whenever the deployed shape does not declare
# ownedBy as an edge (verified_owner_projection), so this is the drift to catch,
# not a cosmetic difference.
Q="PREFIX chorus: <${NS}> SELECT ?card ?own ?form WHERE { GRAPH <${GRAPH}> { ?c a chorus:Card . BIND(STRAFTER(STR(?c), \"card-\") AS ?card) OPTIONAL { ?c chorus:ownedBy ?o . BIND(IF(isIRI(?o), STRAFTER(STR(?o), \"role-\"), STR(?o)) AS ?own) BIND(IF(isIRI(?o), \"iri\", \"literal\") AS ?form) } } }"
if ! curl -sf -G "$QUERY" --data-urlencode "query=$Q" -H "Accept: text/csv" -o "$tmp/raw"; then
  # A box with no store has not proven the product broken — it has measured
  # nothing. Say so out loud and leave the verdict to the self-test above,
  # rather than reporting a red that means "Fuseki is not running here".
  echo "UNMEASURED — the store at $QUERY did not answer; the live half of this suite did not run"
  exit 0
fi
tail -n +2 "$tmp/raw" | sed 's/\r$//' | tr -d '"' | awk -F, 'NF{printf "%s,%s,%s\n", $1, $2, $3}' | sort -u > "$tmp/live"

# what the ontology files declare: chorus:card-<id> ... chorus:ownedBy chorus:role-<who>
# The declaration spans lines in the .ttl (subject on one, ownedBy on the next),
# so tr the whitespace out first rather than grepping line by line.
cat "$ROOT"/roles/*/ontology/*.ttl 2>/dev/null | tr '\n' ' ' \
  | grep -oE 'chorus:card-[0-9]+ a chorus:Card ;[^.]*chorus:ownedBy chorus:role-[a-z]+' \
  | sed -E 's/chorus:card-([0-9]+).*role-([a-z]+)/\1,\2/' | sort -u > "$tmp/declared"

echo "#4116 card ownership — graph ${GRAPH}"
score "$tmp/live" "$tmp/declared"
