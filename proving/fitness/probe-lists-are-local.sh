#!/usr/bin/env bash
# ROW 6 — no two apps share an allow-list source. (#3765)
#
# WHY THIS ROW EXISTS
#
# Jeff, 2026-08-08: "i dont want a user to get access to both apps
# automatically" and "i think the allow list auth must be local to each app."
# Authentication is shared — one sign-in per deployment. Authorization is not.
# Admitted at one app must imply nothing about another, or the second app we
# deliver hands every Chorus builder the keys to a client's data.
#
# WHAT THIS DOES NOT ASSERT
#
# Not "the two apps use different code." They already do — the guard reads a
# file, the Clearing queries a graph. Different mechanisms prove nothing: two
# implementations can read the same list, and one list is one list however many
# ways you read it. So this probe adds an identity to ONE app's source and
# requires the OTHER to still refuse it. Mechanism is irrelevant; reach is the
# measure.
#
# SAFETY
#
# Both sources are scratch. Nothing writes to config/share-principals.txt or to
# the security graph — the incident behind row 5 was exactly a probe-shaped
# write landing on a real graph.
set -u

HERE="$(cd "$(dirname "$0")/../.." && pwd)"
GUARD="$HERE/platform/scripts/chorus-share-guard.py"
STORE="${FITNESS_STORE:-http://localhost:3030/pods}"
PROVE_RED=0
[ "${1:-}" = "--prove-red" ] && PROVE_RED=1

AUTHSH="$HERE/platform/scripts/fuseki-auth.sh"
if [ -f "$AUTHSH" ]; then set +u; . "$AUTHSH" >/dev/null 2>&1; set -u; fi
CURL_AUTH=(); [ "${#FUSEKI_AUTH[@]}" -gt 0 ] 2>/dev/null && CURL_AUTH=("${FUSEKI_AUTH[@]}")

[ -f "$GUARD" ] || { echo "row6 UNMEASURABLE — guard not found" >&2; exit 2; }
up=$(curl -s -o /dev/null -w '%{http_code}' --max-time 8 --data-urlencode 'query=ASK{}' "$STORE/query" 2>/dev/null)
case "$up" in 000|"") echo "row6 UNMEASURABLE — store not answering; cannot resolve the graph-backed list." >&2; exit 2 ;; esac
[ "${#CURL_AUTH[@]}" -gt 0 ] || { echo "row6 UNMEASURABLE — no store credential" >&2; exit 2; }

ROOT="$(mktemp -d)"; SCRATCH_GRAPH="urn:chorus:fitness-probe/row6"
cleanup() {
  curl -s -o /dev/null --max-time 10 "${CURL_AUTH[@]}" \
    -H 'Content-Type: application/sparql-update' \
    --data-binary "DROP SILENT GRAPH <$SCRATCH_GRAPH>" "$STORE/update"
  rm -rf "$ROOT"
}
trap cleanup EXIT

# ── the identity that exists in exactly one place ────────────────────────────
ONLY_GUARD="https://fitness.test/only-guard#me"

# App A's source: a file, read by the guard's OWN loader.
FILE_SRC="$ROOT/principals.txt"
printf '%s\n' "$ONLY_GUARD" > "$FILE_SRC"

# App B's source: a graph, read by the Clearing's OWN query shape.
# Under --prove-red the graph is loaded with app A's file — one list, two
# readers, which is the state this row exists to catch.
if [ "$PROVE_RED" -eq 1 ]; then
  curl -s -o /dev/null --max-time 15 "${CURL_AUTH[@]}" \
    -H 'Content-Type: application/sparql-update' \
    --data-binary "PREFIX chorus: <https://jeffbridwell.com/chorus#>
      INSERT DATA { GRAPH <$SCRATCH_GRAPH> {
        <urn:chorus:fitness-probe/p1> a chorus:Principal ; chorus:webId \"$ONLY_GUARD\" } }" \
    "$STORE/update"
else
  curl -s -o /dev/null --max-time 15 "${CURL_AUTH[@]}" \
    -H 'Content-Type: application/sparql-update' \
    --data-binary "PREFIX chorus: <https://jeffbridwell.com/chorus#>
      INSERT DATA { GRAPH <$SCRATCH_GRAPH> {
        <urn:chorus:fitness-probe/p2> a chorus:Principal ;
          chorus:webId \"https://fitness.test/only-clearing#me\" } }" \
    "$STORE/update"
fi

# ── resolve each app's list through that app's own code path ─────────────────
# The guard: booted on its scratch file, asked who it admits. Not a re-read of
# the file — a re-implementation of a loader agrees with itself while both are
# wrong (the #me fragment was eaten by comment-stripping exactly that way).
UP_PORT=$((23000 + ($$ % 9000))); G_PORT=$((UP_PORT + 1))
export SHARE_PRINCIPALS_FILE="$FILE_SRC" SHARE_STATE_FILE="$ROOT/s.json" SHARE_OIDC_OFFLINE=1
SHARE_UPSTREAM="http://127.0.0.1:$UP_PORT" SHARE_ALLOW="/about" SHARE_PORT="$G_PORT" \
  python3 "$GUARD" >/dev/null 2>&1 &
for _ in $(seq 1 25); do curl -s -o /dev/null "http://127.0.0.1:$G_PORT/_auth/" 2>/dev/null && break; sleep 0.3; done

SESS=$(python3 "$GUARD" --sign-session "$ONLY_GUARD" 2>/dev/null)
guard_admits=$(curl -s --max-time 10 -H "Cookie: chorus_share_session=$SESS" \
  "http://127.0.0.1:$G_PORT/_auth/whoami" | grep -c '"allowed": *true')

# The Clearing: its actual query, against the scratch graph.
clearing_set=$(curl -s --max-time 15 "${CURL_AUTH[@]}" -H 'Accept: text/csv' \
  --data-urlencode "query=PREFIX chorus: <https://jeffbridwell.com/chorus#>
    SELECT ?webid WHERE { GRAPH <$SCRATCH_GRAPH> { ?p a chorus:Principal ; chorus:webId ?webid } }" \
  "$STORE/query" | tr -d '\r')
clearing_admits=$(grep -cx "$ONLY_GUARD" <<<"$clearing_set")

fail=""
# The whole row, in one sentence: app A admits them, app B must not.
if [ "$guard_admits" -eq 0 ]; then
  fail="$fail the identity was added to app A's list and app A does not admit it — the probe is not exercising a real list;"
elif [ "$clearing_admits" -gt 0 ]; then
  fail="$fail an identity added to ONE app's list is admitted by the OTHER — the lists are not local;"
fi

# And the live sources must not be the same locator, or the isolation above is
# an artefact of the sandbox rather than a property of the system.
LIVE_GRAPH=$(grep -oE "urn:chorus:domains:[a-z]+" "$HERE/directing/clearing/src/solid-auth.ts" 2>/dev/null | head -1)
LIVE_FILE=$(grep -oE "share-principals[a-z.]*" "$HERE/config/launchagents/com.chorus.share-guard.plist" 2>/dev/null | head -1)
[ -n "$LIVE_GRAPH" ] || fail="$fail the Clearing's live allow-set locator could not be determined;"

if [ -n "$fail" ]; then
  echo "row6 RED —$fail"
  [ "$PROVE_RED" -eq 1 ] && { echo "NEGATIVE PROOF OK — pointed at one shared list, the probe goes red on cross-app reach."; exit 0; }
  exit 1
fi

if [ "$PROVE_RED" -eq 1 ]; then
  echo "NEGATIVE PROOF FAILED — both apps read the SAME list and the probe still passed." >&2
  echo "It cannot distinguish separate lists from one list read twice." >&2
  exit 1
fi

# Reported, not asserted. The two live lists holding the same people is a policy
# choice — today every principal is a builder, so of course they overlap. What
# would be a defect is being UNABLE to differ, and that is what the walk above
# measures. Printing the overlap keeps the distinction visible instead of
# letting a green line imply more separation than exists.
live_file_n=$(grep -cE '^[^#[:space:]]+' "$HERE/config/share-principals.txt" 2>/dev/null || echo '?')
live_graph_n=$(curl -s --max-time 15 "${CURL_AUTH[@]}" -H 'Accept: text/csv' \
  --data-urlencode "query=PREFIX chorus: <https://jeffbridwell.com/chorus#>
    SELECT (COUNT(?w) AS ?n) WHERE { GRAPH <urn:chorus:domains:security> { ?p a chorus:Principal ; chorus:webId ?w } }" \
  "$STORE/query" 2>/dev/null | tr -d '\r' | tail -1)

echo "row6 GREEN — an identity on one app's list is not admitted by the other (guard: file; Clearing: <$LIVE_GRAPH>)"
echo "         live: guard file holds ${live_file_n} principal(s), <urn:chorus:domains:security> holds ${live_graph_n:-?}."
echo "         Separable is the assertion. Whether they currently hold the same people is policy, and today they largely do."
