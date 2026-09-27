#!/usr/bin/env bats
# @test-type: e2e — the API quartet against PRODUCTION, once per owner.
# @domain: knowledge — the product domain this suite guards (#4334)
#
# #4279 — Jeff, 2026-09-23: "i want it as a production test in the 3am nightly,
# not werk-only." Ruling B (10:48): run once per owner, as each principal
# (kade / wren / silas), no widened probe scope.
#
# One bats case per owner, one for the cross-owner check, one for residue — so
# the nightly report shows five rows, not one. The walks themselves run once in
# setup_file; the cases read the saved output.
#
#   gate      refuses (skips, named) unless it is the nightly (WERK_TEST_NIGHTLY=1)
#             or a person says QUARTET_PROD_CONFIRM=yes — a hand run inside a werk
#             must never hit prod by accident (the #4022 nightly-in-werk class)
#   label     QUARTET_PROD=1 CHORUS_CONTEXT=prod — the #3615 membrane escape
#             hatch, declared on purpose; the runner refuses :3360 without it
#   subjects  zz-probe-<runId>-<class>, one disposable row per class per owner
#   residue   every graph is asked for the rows the door named; the delete
#             reply is never trusted (2026-09-23: "deleted", and the row stayed)
#
# Negative proofs live in 4279-api-quartet-prod-proofs.bats (no prod write).

ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
RUNNER="$ROOT/platform/tests/4267-all-generated-apis.test.sh"
API="${QUARTET_API:-http://localhost:3360}"
QUERY="${FUSEKI_QUERY:-http://localhost:3030/pods/query}"
OWNERS="${QUARTET_OWNERS:-kade wren silas}"

gate_reason() {
  if [ "${WERK_TEST_NIGHTLY:-}" != "1" ] && [ "${QUARTET_PROD_CONFIRM:-}" != "yes" ]; then
    echo "not the nightly and no QUARTET_PROD_CONFIRM=yes — this suite writes PRODUCTION on purpose (#4279)"; return
  fi
  [ -x "$RUNNER" ] || { echo "runner missing: $RUNNER"; return; }
  curl -sf --max-time 5 "$API/health" >/dev/null 2>&1 || echo "$API not answering"
}

setup_file() {
  export RUN_ID
  RUN_ID="${NIGHTLY_RUN_ID:-${QUARTET_RUN_ID:-$(date -u +%Y%m%dT%H%M%SZ)}}"
  # #4282 — lowercase too: the DAL slugs what it writes; a capital in the run id
  # (date -u's T and Z) made every read-back 404 and left 126 rows behind.
  RUN_ID="$(printf '%s' "$RUN_ID" | tr 'A-Z' 'a-z' | tr -c 'a-z0-9' '-' | tr -s '-' | sed 's/^-//; s/-$//')"
  export GATE; GATE="$(gate_reason)"
  [ -z "$GATE" ] || return 0
  for owner in $OWNERS; do
    # #4282 — each owner walks under its own run id (<runId>-<pid>-<owner>), so
    # three walks never share a subject name: a delete the store has not shown
    # yet (the 14:34 nightly, 2026-09-23) can no longer read as "already exists".
    API_BASE="$API" CHORUS_CONTEXT=prod QUARTET_PROD=1 QUARTET_RUN_ID="$RUN_ID-$$-$owner" CHORUS_ROLE="$owner" \
      bash "$RUNNER" >"$BATS_FILE_TMPDIR/$owner.out" 2>&1 || true
  done
}

summary_of() { grep -E '^[0-9]+ pass · ' "$BATS_FILE_TMPDIR/$1.out" | tail -1; }

owner_case() { # $1 owner
  [ -z "$GATE" ] || skip "$GATE"
  local s; s="$(summary_of "$1")"
  echo "$1: $s"
  grep -E ' (FAIL|NOT-PERM|UNMEASURED) ' "$BATS_FILE_TMPDIR/$1.out" | sed "s/^/    /" || true
  [ -n "$s" ] || { echo "the runner produced no summary: $(tail -c 300 "$BATS_FILE_TMPDIR/$1.out")"; return 1; }
  local f; f="$(printf '%s' "$s" | awk '{print $4}')"
  [ "${f:-1}" -eq 0 ]
}

@test "kade: quartet against production — create, read, update, delete, no residue" { owner_case kade; }
@test "wren: quartet against production — create, read, update, delete, no residue" { owner_case wren; }
@test "silas: quartet against production — create, read, update, delete, no residue" { owner_case silas; }

@test "cross-owner: no class fails under every owner (unmeasured ones are named, never red or green)" {
  [ -z "$GATE" ] || skip "$GATE"
  passed="$(cat "$BATS_FILE_TMPDIR"/*.out | grep -E ' PASS ' | awk '{print $1}' | sort -u)"
  failed="$(cat "$BATS_FILE_TMPDIR"/*.out | grep -E ' FAIL ' | awk '{print $1}' | sort -u)"
  unmeasured="$(cat "$BATS_FILE_TMPDIR"/*.out | grep -E ' (NOT-PERM|UNMEASURED) ' | awk '{print $1}' | sort -u)"
  broken="$(comm -23 <(printf '%s\n' "$failed") <(printf '%s\n' "$passed") | grep -v '^$' | tr '\n' ' ' | sed 's/ *$//')"
  nowhere="$(comm -23 <(printf '%s\n' "$unmeasured") <(printf '%s\n' "$passed") | comm -23 - <(printf '%s\n' "$failed") | grep -v '^$' | tr '\n' ' ' | sed 's/ *$//')"
  echo "UNMEASURED under every owner ($(printf '%s' "$nowhere" | wc -w | tr -d ' ')): $nowhere"
  [ -z "$broken" ] || { echo "broken for every owner: $broken"; return 1; }
}

@test "residue: no row named after this run remains in any graph" {
  [ -z "$GATE" ] || skip "$GATE"
  # shellcheck disable=SC1091
  [ -r "$ROOT/platform/scripts/fuseki-auth.sh" ] && source "$ROOT/platform/scripts/fuseki-auth.sh" 2>/dev/null
  # the rows as the DOOR names them: <kind-slug>-zz-probe-<runId>-<pid>-<owner>-<class>
  vals="$(curl -sf --max-time 20 "$API/" | python3 -c 'import json,sys,re
rid,pid,owners=sys.argv[1],sys.argv[2],sys.argv[3].split()
for p in json.load(sys.stdin)["primitives"]:
    k=p["kind"]; slug=re.sub(r"(?<!^)(?=[A-Z])","-",k).lower()
    for o in owners:
        print("<https://jeffbridwell.com/chorus#%s>" % (("%s-zz-probe-%s-%s-%s-%s" % (slug, rid, pid, o, k.lower()))[:140]))' "$RUN_ID" "$$" "$OWNERS" | tr '\n' ' ')"
  [ -n "$vals" ] || { echo "no class list from $API/ — residue not measurable"; return 1; }
  left="$(curl -s --max-time 60 "${FUSEKI_AUTH[@]+"${FUSEKI_AUTH[@]}"}" -G "$QUERY" \
    --data-urlencode "query=SELECT DISTINCT ?g ?s WHERE { VALUES ?s { $vals } GRAPH ?g { ?s ?p ?o } }" \
    -H 'Accept: text/csv' 2>/dev/null | tail -n +2 | tr -d '\r' | sed 's|https://jeffbridwell.com/chorus#||' | tr ',' ' ')"
  echo "run $RUN_ID: ${left:-clean}"
  [ -z "$left" ]
}
