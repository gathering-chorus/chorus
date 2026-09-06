#!/usr/bin/env bash
# card-ownership-backfill-4116.sh — #4116, one-time and idempotent.
#
# Every live chorus:Card row was minted with a label and nothing else (the board
# projection never passed an owner, board-3654-project.sh CARD branch). The write
# door is fail-closed on ownership, so all 229 of them were unwritable by anyone —
# which is what stopped #4102 mid-land with a 403 on its own model statement.
#
# This gives each row the owner it already has somewhere authoritative:
#   1. the ontology declaration, when the model states one (14 rows)
#   2. otherwise the board, which is the system of record for who owns a card
#   3. otherwise NOTHING — the row is named and counted, never guessed at
#
# The write goes through the DAL (athena-model link), the same governed path the
# projection uses. The HTTP door is not loosened and authz_allows is untouched.
set -u

ROOT="${CHORUS_ROOT:-/Users/jeffbridwell/CascadeProjects/chorus}"
QUERY="${FUSEKI_QUERY:-http://localhost:3030/pods/query}"
GRAPH="${CARD_GRAPH:-urn:chorus:instances}"
CARDS="${CARDS_CLI:-$ROOT/platform/scripts/cards}"
AM="${CHORUS_MODEL_BIN:-$(command -v athena-model 2>/dev/null || echo target/release/athena-model)}"
DRY="${DRY_RUN:-0}"
NS="https://jeffbridwell.com/chorus#"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/fuseki-auth.sh"
_ROLE="${DEPLOY_ROLE:-${CHORUS_ROLE:-system}}"
export CHORUS_IDENTITY_TOKEN="$("$SCRIPT_DIR/chorus-identity-token" "$_ROLE" 2>/dev/null || true)"

tmp=$(mktemp -d); trap 'rm -rf "$tmp"' EXIT

# live rows with no owner
Q="PREFIX chorus: <${NS}> SELECT ?card WHERE { GRAPH <${GRAPH}> { ?c a chorus:Card . BIND(STRAFTER(STR(?c), \"card-\") AS ?card) FILTER NOT EXISTS { ?c chorus:ownedBy ?o } } }"
curl -sf -G "$QUERY" --data-urlencode "query=$Q" -H "Accept: text/csv" \
  | tail -n +2 | sed 's/\r$//' | grep -E '^[0-9]+$' | sort -n > "$tmp/unowned" \
  || { echo "FATAL: store did not answer" >&2; exit 2; }

# what the model declares
cat "$ROOT"/roles/*/ontology/*.ttl 2>/dev/null | tr '\n' ' ' \
  | grep -oE 'chorus:card-[0-9]+ a chorus:Card ;[^.]*chorus:ownedBy chorus:role-[a-z]+' \
  | sed -E 's/chorus:card-([0-9]+).*role-([a-z]+)/\1 \2/' | sort -u > "$tmp/declared"

W=0; R=0; U=0
while read -r cid; do
  [ -n "$cid" ] || continue
  owner="$(awk -v c="$cid" '$1==c {print $2}' "$tmp/declared" | head -1)"
  src="model"
  if [ -z "$owner" ]; then
    owner="$("$CARDS" view "$cid" 2>/dev/null | awk '/^  Owner:/ {print tolower($2); exit}')"
    src="board"
  fi
  case "$owner" in
    wren|silas|kade|jeff) ;;
    *)
      # No authority states an owner. Name it — do not invent one.
      echo "UNRESOLVED card-$cid — neither the model nor the board names an owner"
      U=$((U+1)); continue ;;
  esac
  if [ "$DRY" = "1" ]; then
    echo "DRY: card-$cid -> role-$owner ($src)"; W=$((W+1)); continue
  fi
  if out="$("$AM" link --kind card --name "$cid" --edge "ownedBy=role:$owner" 2>&1)"; then
    W=$((W+1))
  else
    echo "REFUSED card-$cid -> role-$owner ($src): $out" >&2
    R=$((R+1))
  fi
done < "$tmp/unowned"

echo "backfill: $W written, $R refused, $U unresolved (of $(wc -l < "$tmp/unowned" | tr -d ' ') unowned rows)"
[ "$R" -eq 0 ] && [ "$U" -eq 0 ]
