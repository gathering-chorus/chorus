#!/usr/bin/env bats
# @test-type: integration
# #4125 — a subject deleted from a source .ttl is NAMED, not silently kept.
#
# The deploy's merge is per-subject additive: it only ever touches subjects that are
# IN staging. Remove a subject from source and nothing removes it from the store —
# it lives forever. Wren lost three attempts to this on one merge on 2026-09-18.
#
# The whole point of this suite is the NEGATIVE direction, so it is built as a real
# git fixture rather than a stub: a throwaway repo with one tracked .ttl, deployed
# once (which stamps deployedFromCommit), then edited to remove a subject. The guard
# compares the working tree against the commit the STORE says it came from, so it
# cannot be exercised by a fixture that is not a git repo with history.
#
# Asserts use simple commands, never `[[ ]]` — on bash 3.2 a failing `[[` that is not
# the test's last line passes the test silently (91 of 223 suites had this shape).

SCRIPT="$(cd "$BATS_TEST_DIRNAME/../scripts" && pwd)/athena-deploy-model.sh"
Q="http://localhost:3030/pods/query"
GSP="http://localhost:3030/pods/data"
GRAPH="urn:chorus:ontology-test-bats-4125"

# shellcheck source=/dev/null
. "$(cd "$BATS_TEST_DIRNAME/../scripts" && pwd)/fuseki-auth.sh" 2>/dev/null || true

_drop_graph() {
  curl -s "${FUSEKI_AUTH[@]+"${FUSEKI_AUTH[@]}"}" -X DELETE "$GSP?graph=$1" -o /dev/null 2>/dev/null || true
}

# A minimal tracked model file. One subject we will delete, one we keep so the deploy
# still has something to merge (an empty staging is refused by other guards).
_write_ttl() {
  local path="$1" with_a="$2"
  {
    echo '@prefix chorus: <https://jeffbridwell.com/chorus#> .'
    echo '@prefix rdfs:   <http://www.w3.org/2000/01/rdf-schema#> .'
    echo '@prefix owl:    <http://www.w3.org/2002/07/owl#> .'
    echo ''
    echo 'chorus:fixtureKeep a owl:Class ; rdfs:label "fixture keep" .'
    if [ "$with_a" = "yes" ]; then
      echo 'chorus:fixtureDropMe a owl:Class ; rdfs:label "fixture drop me" .'
    fi
  } > "$path"
}

setup() {
  REPO="$BATS_TEST_TMPDIR/fixture-repo"
  mkdir -p "$REPO/roles/silas/ontology" "$REPO/platform/scripts"
  # The script sources $CHORUS_ROOT/platform/scripts/fuseki-auth.sh, and CHORUS_ROOT is
  # the fixture repo here (the guard needs git history at that root). Without this the
  # staging load 401s and every test fails for the wrong reason.
  cp "$(cd "$BATS_TEST_DIRNAME/../scripts" && pwd)/fuseki-auth.sh" "$REPO/platform/scripts/"
  TTL_PATH="$REPO/roles/silas/ontology/fixture-4125.ttl"
  git -C "$REPO" init -q 2>/dev/null
  git -C "$REPO" config user.email "silas@chorus.local"
  git -C "$REPO" config user.name "silas"
  _write_ttl "$TTL_PATH" yes
  git -C "$REPO" add -A >/dev/null 2>&1
  git -C "$REPO" commit -q -m "fixture: both subjects" >/dev/null 2>&1
  RF="$BATS_TEST_TMPDIR/retirements.jsonl"
  : > "$RF"
  _drop_graph "$GRAPH"
}

teardown() {
  _drop_graph "$GRAPH"
}

_deploy() {
  env ONTOLOGY_GRAPH="$GRAPH" TTL="$TTL_PATH" CHORUS_ROOT="$REPO" \
      RETIREMENTS_FILE="$RF" DEPLOY_ROLE=silas "$@" bash "$SCRIPT"
}

_live_count() {
  curl -s "$Q" -H 'Accept: text/csv' \
    --data-urlencode "query=SELECT (COUNT(*) AS ?n) WHERE { GRAPH <$GRAPH> { <https://jeffbridwell.com/chorus#$1> ?p ?o } }" \
    2>/dev/null | tail -1 | tr -dc '0-9'
}

@test "#4125 first deploy into an unstamped store says so and proceeds" {
  run _deploy
  test "$status" -eq 0
  echo "$output" | grep -q "source-delete check SKIPPED"
  test "$(_live_count fixtureDropMe)" -gt 0
}

@test "#4125 a subject removed from source with no staged retirement REFUSES, naming the subject and the fix" {
  _deploy >/dev/null 2>&1
  test "$(_live_count fixtureDropMe)" -gt 0          # it is live, from the stamped commit

  _write_ttl "$TTL_PATH" no                           # the deletion
  git -C "$REPO" commit -aqm "fixture: drop fixtureDropMe" >/dev/null 2>&1

  run _deploy
  test "$status" -eq 1
  echo "$output" | grep -q "REFUSED"
  echo "$output" | grep -q "fixtureDropMe"
  echo "$output" | grep -q "fixture-4125.ttl"
  # the refusal names the FIX, not only the fact (Wren, 2026-09-18 13:27)
  echo "$output" | grep -q "retire-subject"
  # and nothing was written: the subject is untouched, not half-removed
  test "$(_live_count fixtureDropMe)" -gt 0
}

@test "#4125 NEGATIVE PROOF: without the guard the same deletion deploys clean and the subject SURVIVES" {
  _deploy >/dev/null 2>&1
  _write_ttl "$TTL_PATH" no
  git -C "$REPO" commit -aqm "fixture: drop fixtureDropMe" >/dev/null 2>&1

  run _deploy DEPLOY_SOURCE_DELETE_CHECK=0
  test "$status" -eq 0
  # This is the defect the guard exists to catch: green deploy, row still live.
  test "$(_live_count fixtureDropMe)" -gt 0
}

@test "#4125 a staged retirement lets the same deletion through, and the subject is gone" {
  _deploy >/dev/null 2>&1
  _write_ttl "$TTL_PATH" no
  git -C "$REPO" commit -aqm "fixture: drop fixtureDropMe" >/dev/null 2>&1

  printf '%s\n' "{\"retire_subject\": \"https://jeffbridwell.com/chorus#fixtureDropMe\", \"graph\": \"$GRAPH\", \"reason\": \"#4125 fixture\", \"by\": \"silas\", \"card\": \"4125\", \"status\": \"staged\"}" > "$RF"

  run _deploy
  test "$status" -eq 0
  echo "$output" | grep -qv "REFUSED"
  test "$(_live_count fixtureDropMe)" -eq 0
  test "$(_live_count fixtureKeep)" -gt 0
}

@test "#4125 a subject already absent from the store is not reported" {
  # deleted from source AND never live: a clean removal, nothing to say
  _write_ttl "$TTL_PATH" no
  git -C "$REPO" commit -aqm "fixture: drop before first deploy" >/dev/null 2>&1
  _deploy >/dev/null 2>&1                              # stamps, with only fixtureKeep live
  run _deploy
  test "$status" -eq 0
  echo "$output" | grep -qv "fixtureDropMe"
}

# --- the second leg of #4125: the source may not author a Role as an owner ---

@test "#4125 a MODEL_SET file authoring ownedBy chorus:role-* REFUSES the deploy, naming file and fix" {
  printf '%s\n' 'chorus:fixtureOwned a owl:Class ; chorus:ownedBy chorus:role-kade .' >> "$TTL_PATH"
  git -C "$REPO" commit -aqm "fixture: author a role owner" >/dev/null 2>&1
  run _deploy
  test "$status" -eq 1
  echo "$output" | grep -q "REFUSED"
  echo "$output" | grep -q "authors a Role as an owner"
  echo "$output" | grep -q "fixture-4125.ttl"
  echo "$output" | grep -q "chorus:principal-<name>"
}

@test "#4125 NEGATIVE PROOF: the check distinguishes an ownedBy role from every other role reference" {
  # holdsRole / appointedHat name a Role legitimately — a guard that cannot tell these
  # from an owner would refuse every real file and be deleted within a day.
  printf '%s\n' 'chorus:fixtureHolder a owl:Class ; chorus:holdsRole chorus:role-kade ; chorus:appointedHat chorus:role-wren .' >> "$TTL_PATH"
  git -C "$REPO" commit -aqm "fixture: legitimate role references" >/dev/null 2>&1
  run _deploy
  test "$status" -eq 0
  echo "$output" | grep -qv "authors a Role as an owner"
}

@test "#4125 the shipped MODEL_SET authors zero role owners" {
  run grep -rlE 'ownedBy[[:space:]]+chorus:role-' --include='*.ttl' "$(cd "$BATS_TEST_DIRNAME/../.." && pwd)/roles"
  test "$status" -ne 0
}
