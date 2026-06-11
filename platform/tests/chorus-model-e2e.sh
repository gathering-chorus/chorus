#!/bin/bash
# chorus-model e2e (#3257 AC5) — prove the DAL against the LIVE store.
#
# Three proofs:
#   1. CONVENTION MATCH (the #3242 slice, zero hand-edits): the mint re-derives
#      the canonical tree.json IRIs byte-identical — the entities the #3242
#      slice hand-typed wrong come out right through (kind, name) alone.
#   2. GOVERNED WRITE, live: a sacrificial role is written through the full
#      path (shape check → referential integrity → idempotent UPDATE), verified
#      by ASK, written TWICE to prove idempotence, then cleaned up.
#   3. FAIL-CLOSED, live: an edge to a nonexistent target is refused and
#      writes nothing.
#
# Read-only against canonical model data except the sacrificial subject
# chorus:role-dal-e2e-sacrificial (created + deleted here).
set -uo pipefail

BIN="${CHORUS_MODEL_BIN:-$(command -v chorus-model || echo "$(dirname "$0")/../services/chorus-model/target/debug/chorus-model")}"
FUSEKI="${CHORUS_FUSEKI:-http://localhost:3030/pods}"
NS="https://jeffbridwell.com/chorus#"
PASS=0; FAIL=0

ok()   { PASS=$((PASS+1)); echo "  ok  - $1"; }
bad()  { FAIL=$((FAIL+1)); echo "  FAIL - $1"; }

check_mint() { # kind name expected-localname
  local got; got=$("$BIN" mint --kind "$1" --name "$2" 2>&1)
  if [ "$got" = "${NS}$3" ]; then ok "mint $1/$2 → $3"; else bad "mint $1/$2: got '$got' want '${NS}$3'"; fi
}

echo "— proof 1: the mint re-derives canonical IRIs (the #3242 slice, zero hand-edits)"
# the five value-stream steps (the exact class that was hand-typed as chorus:Proving)
for s in shaping designing directing building proving; do
  check_mint value-stream-step "$s" "value-stream-step-$s"
done
# roles + a service + bare-grain spine entities, all live tree.json canon
check_mint role wren  "role-wren"
check_mint role jeff  "role-jeff"
check_mint service crawler "service-crawler"
check_mint product loom "loom"
check_mint domain principles "principles"
# the mismatch input itself: 'Proving' as authored in #3242 — comes out canonical
check_mint value-stream-step "Proving" "value-stream-step-proving"

echo "— proof 2: governed write, live (shape → integrity → idempotent), then cleanup"
SUBJ="${NS}role-dal-e2e-sacrificial"
OUT1=$("$BIN" add --kind role --name dal-e2e-sacrificial \
  --field label="DAL e2e sacrificial" --field comment="created+deleted by chorus-model-e2e.sh (#3257)" 2>&1)
case "$OUT1" in
  written:*) ok "live write accepted: $OUT1" ;;
  *) bad "live write refused: $OUT1" ;;
esac
ASK=$(curl -sf -H "Accept: application/sparql-results+json" \
  --data-urlencode "query=ASK { GRAPH <urn:chorus:instances> { <$SUBJ> ?p ?o } }" "$FUSEKI/query")
echo "$ASK" | grep -q '"boolean" *: *true' && ok "subject exists in instances graph (ASK)" || bad "subject missing after write"
# idempotence: write again, count triples — same count both times
COUNT_Q="SELECT (COUNT(*) as ?v) WHERE { GRAPH <urn:chorus:instances> { <$SUBJ> ?p ?o } }"
C1=$(curl -sf -H "Accept: application/sparql-results+json" --data-urlencode "query=$COUNT_Q" "$FUSEKI/query" | grep -o '"value" *: *"[0-9]*"' | grep -o '[0-9]*')
"$BIN" add --kind role --name dal-e2e-sacrificial \
  --field label="DAL e2e sacrificial" --field comment="created+deleted by chorus-model-e2e.sh (#3257)" >/dev/null 2>&1
C2=$(curl -sf -H "Accept: application/sparql-results+json" --data-urlencode "query=$COUNT_Q" "$FUSEKI/query" | grep -o '"value" *: *"[0-9]*"' | grep -o '[0-9]*')
[ -n "$C1" ] && [ "$C1" = "$C2" ] && ok "idempotent: $C1 triples both writes" || bad "idempotence broke: $C1 vs $C2"

echo "— proof 2b: edge with referential integrity (sacrificial target — NOTE: the live"
echo "  store does NOT yet hold v2 instances like value-stream-step-proving; the DAL's"
echo "  integrity check surfaced that honestly = the model-3 reconcile gap, on the card)"
"$BIN" add --kind role --name dal-e2e-target --field label="DAL e2e target" >/dev/null 2>&1
OUT2=$("$BIN" add --kind role --name dal-e2e-sacrificial \
  --field label="DAL e2e sacrificial" \
  --edge ownedBy=role:dal-e2e-target 2>&1)
case "$OUT2" in
  written:*) ok "edge to existing (sacrificial) target accepted" ;;
  *) bad "edge to existing target refused: $OUT2" ;;
esac

echo "— proof 3: fail-closed on unknown target, live"
OUT3=$("$BIN" add --kind role --name dal-e2e-sacrificial-2 \
  --edge atStep=value-stream-step:does-not-exist-xyz 2>&1)
case "$OUT3" in
  *unknown-target*) ok "unknown edge target refused: fail-closed" ;;
  *) bad "expected unknown-target refusal, got: $OUT3" ;;
esac
ASK3=$(curl -sf -H "Accept: application/sparql-results+json" \
  --data-urlencode "query=ASK { GRAPH <urn:chorus:instances> { <${NS}role-dal-e2e-sacrificial-2> ?p ?o } }" "$FUSEKI/query")
echo "$ASK3" | grep -q '"boolean" *: *true' && bad "refused write leaked triples" || ok "refusal wrote nothing"

echo "— cleanup"
for s_iri in "$SUBJ" "${NS}role-dal-e2e-target"; do
  curl -sf --data-urlencode "update=DELETE WHERE { GRAPH <urn:chorus:instances> { <$s_iri> ?p ?o } }" "$FUSEKI/update" >/dev/null \
    && ok "removed $s_iri" || bad "cleanup failed — remove <$s_iri> by hand"
done

echo "════ $PASS passed, $FAIL failed ════"
[ "$FAIL" -eq 0 ]
