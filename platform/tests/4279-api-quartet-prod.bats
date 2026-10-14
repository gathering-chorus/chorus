#!/usr/bin/env bats
# @test-type: e2e — the API quartet against PRODUCTION, once per owner.
# @domain: knowledge — the product domain this suite guards (#4334)
#
# #4279 — Jeff, 2026-09-23: "i want it as a production test in the 3am nightly,
# not werk-only."
# #4022 — Jeff, 2026-10-13: "i dont see why the api test must be bound to agent
# principals." One walk, as principal-quartet (#3613), not three as kade / wren /
# silas (989s). Its scope is every row graph except security, identity and roles,
# so it cannot grant itself anything; a class there must be REFUSED, and that
# refusal is the pass. Each role's own write scope is 4022-role-write-scope-live.bats.
#
# One case for the walk, one for residue. The walk runs once in setup_file; the
# cases read the saved output.
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
RUNNER="$ROOT/platform/tests/lib/quartet-walk.sh"
API="${QUARTET_API:-http://localhost:3360}"
QUERY="${FUSEKI_QUERY:-http://localhost:3030/pods/query}"
OWNERS="${QUARTET_OWNERS:-quartet}"
PLAN="$ROOT/platform/tests/lib/scope-plan.py"
REFUSED_GRAPHS="${QUARTET_REFUSED_GRAPHS:-urn:chorus:domains:security urn:chorus:domains:identity urn:chorus:domains:roles}"

# shellcheck disable=SC1091
source "$ROOT/platform/tests/lib/prod-gate.sh"
gate_reason() { prod_gate_reason "$ROOT" "$RUNNER" "$API"; }

setup_file() {
  export RUN_ID
  RUN_ID="${NIGHTLY_RUN_ID:-${QUARTET_RUN_ID:-$(date -u +%Y%m%dT%H%M%SZ)}}"
  # #4282 — lowercase too: the DAL slugs what it writes; a capital in the run id
  # (date -u's T and Z) made every read-back 404 and left 126 rows behind.
  RUN_ID="$(printf '%s' "$RUN_ID" | tr 'A-Z' 'a-z' | tr -c 'a-z0-9' '-' | tr -s '-' | sed 's/^-//; s/-$//')"
  export GATE; GATE="$(gate_reason)"
  [ -z "$GATE" ] || return 0
  python3 "$PLAN" graphs "$API" > "$BATS_FILE_TMPDIR/graphs.json"
  # shellcheck disable=SC2086
  python3 "$PLAN" expect "$BATS_FILE_TMPDIR/graphs.json" $REFUSED_GRAPHS > "$BATS_FILE_TMPDIR/expect"
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
  [ -n "$s" ] || { echo "the runner produced no summary: $(tail -c 300 "$BATS_FILE_TMPDIR/$1.out")"; return 1; }
  # #4022 — every class answers: written end to end, or refused where the scope
  # says it must be. A refusal anywhere else, a FAIL, or an unmeasured class is red.
  run python3 "$PLAN" verdict "$BATS_FILE_TMPDIR/expect" "$BATS_FILE_TMPDIR/$1.out"
  echo "$output"
  grep -E ' (FAIL|UNMEASURED) ' "$BATS_FILE_TMPDIR/$1.out" | sed "s/^/    /" || true
  [ "$status" -eq 0 ] || return 1
  ! grep -qE ' (FAIL|UNMEASURED) ' "$BATS_FILE_TMPDIR/$1.out" || return 1
}

@test "quartet: quartet against production, one walk of every generated API — written end to end, or refused where its scope says" { owner_case quartet; }

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
