#!/usr/bin/env bash
# ROW 5 — everyone can still write. (#3765)
#
# WHY THIS ROW EXISTS
#
# On 2026-08-02 I emptied the security graph believing it was a duplicate. That
# locked Jeff out of the Clearing and locked the data-access layer against me —
# the person who would have to fix it. Recovery worked only because the deploy
# path authenticates to the STORE rather than through the identity system: it
# holds the store's credentials, not a door key. If that ever stops being true,
# a bad identity deploy becomes unrecoverable from inside.
#
# WHAT THIS REPLACES, AND WHY
#
# platform/tests/recovery-path-ungated-3785.bats asserts this by grepping the
# recovery script for the absence of a token check. That proves the file SAYS
# something. It passes against a script that has been renamed out of the deploy
# path, against a store that refuses every write, and against a role whose
# credentials were revoked this morning. Wren's sweep found the same shape
# across three repos; two of the offenders are mine, and this is one of them.
#
# So this probe does not read the recovery path. It WRITES.
#
# SAFETY
#
# Every write goes to a scratch graph that exists for this probe and is deleted
# at the end. Nothing in the security graph, the instance graph, or any model
# graph is touched — the incident this row commemorates was caused by a write to
# a real graph, and a probe for it must not be able to repeat it.
set -u

STORE="${FITNESS_STORE:-http://localhost:3030/pods}"
GRAPH="urn:chorus:fitness-probe/row5"
PROVE_RED=0
[ "${1:-}" = "--prove-red" ] && PROVE_RED=1

# The store's own credentials. Sourced, never echoed — running the allow-set
# gate under `bash -x` on 2026-08-07 put the admin password into a transcript.
AUTHSH="$(cd "$(dirname "$0")/../.." && pwd)/platform/scripts/fuseki-auth.sh"
if [ -f "$AUTHSH" ]; then
  # shellcheck disable=SC1090
  set +u; . "$AUTHSH" >/dev/null 2>&1; set -u
fi
# FUSEKI_AUTH is an ARRAY of curl arguments — (-u user:pass) — not a string.
# Treating it as a string expanded to a bare "-u", curl read that as the
# username and prompted for a password on stdin, and every write came back 401.
# The first run of this probe went red against a healthy store for that reason,
# AND its negative proof reported OK — because a probe that cannot write at all
# is red no matter which credentials it is holding. Same wrong-reason pass as
# the empty fixture in row 1: the proof agreed with itself, not with reality.
CURL_AUTH=()
[ "${#FUSEKI_AUTH[@]}" -gt 0 ] 2>/dev/null && CURL_AUTH=("${FUSEKI_AUTH[@]}")

# Holding no credential at all is NOT the same as being refused. Without this,
# a missing credential file would make the negative proof pass every time while
# proving nothing — the guarded condition would be unreachable.
if [ "$PROVE_RED" -eq 0 ] && [ "${#CURL_AUTH[@]}" -eq 0 ]; then
  echo "row5 UNMEASURABLE — no store credential available; this probe cannot ask." >&2
  exit 2
fi

if [ "$PROVE_RED" -eq 1 ]; then
  # The state this row exists to catch: the store is there and will not take
  # our writes. Credentials that are WRONG rather than absent — absent ones fail
  # at a different layer and would prove the wrong thing.
  CURL_AUTH=(-u "fitness-probe-nobody:definitely-not-the-password")
fi

req() {  # req <endpoint> <content-type> <body>  -> prints http code
  curl -s -o /dev/null -w '%{http_code}' --max-time 15 \
    "${CURL_AUTH[@]}" -H "Content-Type: $2" --data-binary "$3" "$STORE/$1"
}
ask() {  # ask <sparql> -> prints body
  curl -s --max-time 15 "${CURL_AUTH[@]}" \
    -H 'Accept: text/csv' --data-urlencode "query=$1" "$STORE/query"
}

# Is the store even there? A probe that reports failure when it could not ask
# has told you nothing, and teaches people to discount it when it is right.
up=$(curl -s -o /dev/null -w '%{http_code}' --max-time 8 \
     --data-urlencode 'query=ASK{}' "$STORE/query" 2>/dev/null)
case "$up" in
  000|"") echo "row5 UNMEASURABLE — no answer from $STORE. Could not ask." >&2; exit 2 ;;
esac

fail=""
ROLES="jeff silas wren kade"

for r in $ROLES; do
  s="urn:chorus:fitness-probe/$r"
  code=$(req update "application/sparql-update" \
    "INSERT DATA { GRAPH <$GRAPH> { <$s> <urn:chorus:fitness#wrote> \"$r\" } }")
  case "$code" in
    2*) ;;
    *)  fail="$fail $r cannot write to the store (HTTP $code);"; continue ;;
  esac
  # Accepted is not the same as stored. The store answering 200 and holding
  # nothing is precisely the gap between "the deploy succeeded" and "the thing
  # is live" that has bitten us five times in two days.
  # SPARQL CSV returns the bare lexical form — `v\njeff` — not a quoted literal.
  # Matching on quotes found nothing and read as "accepted but not stored" for
  # writes that were, in fact, stored.
  if ! grep -qx "$r" <<<"$(ask "SELECT ?v WHERE { GRAPH <$GRAPH> { <$s> <urn:chorus:fitness#wrote> ?v } }" | tr -d '\r')"; then
    fail="$fail $r's write was accepted but is not in the store;"
  fi
done

# Clean up with the same credentials that wrote — under --prove-red these are
# wrong, so nothing was written and nothing needs removing.
req update "application/sparql-update" "DROP SILENT GRAPH <$GRAPH>" >/dev/null

if [ -n "$fail" ]; then
  echo "row5 RED — the store will not take writes we depend on being able to make:$fail"
  [ "$PROVE_RED" -eq 1 ] && { echo "NEGATIVE PROOF OK — with credentials the store rejects, the probe goes red."; exit 0; }
  exit 1
fi

if [ "$PROVE_RED" -eq 1 ]; then
  echo "NEGATIVE PROOF FAILED — writes succeeded with credentials the store should reject." >&2
  echo "Either the store is not checking them, or this probe is not really writing." >&2
  exit 1
fi

echo "row5 GREEN — all of $ROLES wrote to the store and the writes were read back"
