#!/usr/bin/env bats
# @test-type: contract — runs the policies' own checks over committed fixtures and land-4960-rows against a fake door; no store, no network.
# @domain: policies
# @card: #4960
# @owner: silas
#
# #4960 AC1 — the rows land through the door: 57 policies — 46 ADR/DEC rules (42 plain, to
# read; 4 made one-line commands) and the 11 GovernanceChecks folded in as runnable checks, each with a
# measured baseline; policy-no-self-acp says Jeff's go only. (The 34 GovernanceCheck rows
# leave with the class's retirement: AC2.) Every runnable check proves it can go red on
# its own violation fixture and stays green on clean data (#3734).

ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
CHECKS="$ROOT/designing/data/policy-checks-4960.json"
RULES="$ROOT/designing/data/policies-4960.tsv"
PV="$ROOT/platform/scripts/policy-validate"
LAND="$ROOT/platform/scripts/land-4960-rows"

setup() {
  T="$BATS_TEST_TMPDIR"
  printf '@prefix c: <https://jeffbridwell.com/chorus#> .\n' > "$T/empty.trig"
}

# one check from the plan as a door answer, baseline forced to 0
one_policy() {
  python3 - "$CHECKS" "$1" "$T/p.json" <<'PY'
import json, sys
rows = [r for r in json.load(open(sys.argv[1])) if r["name"] == sys.argv[2]]
assert rows, sys.argv[2]
r = dict(rows[0]); r["baseline"] = "0"
json.dump({"data": [r]}, open(sys.argv[3], "w"))
PY
  export POLICIES_JSON="$T/p.json"
}

@test "the plan holds 46 rules and 16 runnable checks, every check with a fixture on disk" {
  [ "$(grep -vc '^#' "$RULES")" -eq 46 ] || return 1
  run python3 -c "import json,os,sys; r=json.load(open('$CHECKS')); print(len(r)); [sys.exit('no fixture: '+x['violationFixture']) for x in r if not os.path.isfile(os.path.join('$ROOT', x['violationFixture']))]"
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
  [ "$output" = "16" ] || return 1
}

@test "negative proof: every SPARQL check counts its fixture's violation and is clean on empty data" {
  for name in $(python3 -c "import json; print(' '.join(r['name'] for r in json.load(open('$CHECKS')) if r['checkKind']=='sparql'))"); do
    one_policy "$name"
    fx=$(python3 -c "import json; print([r['violationFixture'] for r in json.load(open('$CHECKS')) if r['name']=='$name'][0])")
    POLICY_DATA="$ROOT/$fx" run "$PV"
    [ "$status" -eq 1 ] || { echo "$name did not go red on $fx: $output"; return 1; }
    POLICY_DATA="$T/empty.trig" run "$PV"
    [ "$status" -eq 0 ] || { echo "$name is not clean on empty data: $output"; return 1; }
  done
}

# The command checks' fixtures, named so a change to one runs this suite (werk-test ties a
# script to the tests that name it); the plan must name exactly these.
COMMAND_FIXTURES="platform/tests/fixtures/4960/arch-css-sole-identity-issuer
platform/tests/fixtures/4960/arch-localhost-internal-binds
platform/tests/fixtures/4960/code-domain-api-over-raw-fuseki
platform/tests/fixtures/4960/qual-no-compiled-artifacts-in-src
platform/tests/fixtures/4960/qual-stdout-to-loki-no-log-files"

@test "the plan's command fixtures are exactly the ones named here" {
  run python3 -c "import json; print('\\n'.join(sorted(r['violationFixture'].removesuffix('.sh') for r in json.load(open('$CHECKS')) if r['checkKind']=='command')))"
  [ "$output" = "$COMMAND_FIXTURES" ] || { echo "$output"; return 1; }
}

@test "negative proof: every command check counts its fixture's violation and is clean in an empty tree" {
  for name in $(python3 -c "import json; print(' '.join(r['name'] for r in json.load(open('$CHECKS')) if r['checkKind']=='command'))"); do
    one_policy "$name"
    fx=$(python3 -c "import json; print([r['violationFixture'] for r in json.load(open('$CHECKS')) if r['name']=='$name'][0])")
    red="$T/red-$name"; clean="$T/clean-$name"; mkdir -p "$red" "$clean"
    sh "$ROOT/$fx" "$red" >/dev/null
    (cd "$clean" && git init -q .)
    run sh -c "cd '$red' && '$PV'"
    [ "$status" -eq 1 ] || { echo "$name did not go red: $output"; return 1; }
    run sh -c "cd '$clean' && '$PV'"
    [ "$status" -eq 0 ] || { echo "$name is not clean: $output"; return 1; }
  done
}

@test "every planned row conforms to the live PolicyShape" {
  run python3 "$LAND" --ttl "$T/rows.ttl"
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
  printf '@prefix c: <https://jeffbridwell.com/chorus#> .\nc:principal-silas a c:Principal .\n' >> "$T/rows.ttl"
  run shacl validate --shapes "$ROOT/roles/wren/ontology/policies-4077.ttl" --data "$T/rows.ttl"
  echo "$output" | grep -q "sh:conforms *true" || { echo "$output" | grep resultMessage | head; return 1; }
}

@test "the dry run plans the grant, the adds and the Jeff-only fix, deletes nothing, and writes nothing" {
  python3 - "$T/live.json" <<'PY'
import json, sys
live = {"policies": [{"name": "no-self-acp", "comment": "Only Wren or Jeff can accept."}],
        "governancechecks": [{"name": "gc-x"}]}  # not this script's to delete (AC2)
json.dump(live, open(sys.argv[1], "w"))
PY
  printf '#!/bin/sh\necho "$*" >> %s/runner.log\n' "$T" > "$T/runner"; chmod +x "$T/runner"
  LAND_LIVE_JSON="$T/live.json" CHORUS_RUNNER="$T/runner" run python3 "$LAND"
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
<<<<<<< HEAD
<<<<<<< HEAD
<<<<<<< HEAD
  [[ "$output" == *"62 add, 1 change, 0 delete"* ]] || { echo "$output" | tail -3; return 1; }
=======
  [[ "$output" == *"59 add, 1 change, 34 delete"* ]] || { echo "$output" | tail -3; return 1; }
>>>>>>> 262c8739a (silas: #4960 — AC1: the land writes as Jeff's go, who could not write policies or domains. permission-jeff-policies and permission-jeff-domains in permissions-4183.ttl (count 51, reason in 4183's test), and land-4960-rows adds them first through the security door; a write refused inside the door's 300 s scope cache waits once and retries. No store write before the go. 10/10; live dry run 59 add, 1 change, 34 delete.)
=======
  [[ "$output" == *"58 add, 1 change, 0 delete"* ]] || { echo "$output" | tail -3; return 1; }
>>>>>>> 26bd012d4 (silas: #4960 — AC1 (BQ: the land runs as the card's role, not Jeff): one grant, permission-silas-policies (count 50), added first by land-4960-rows; jeff grants dropped. The GovernanceCheck deletes leave the script: the class retirement removes its 34 rows (AC2). Live dry run: 58 add, 1 change, 0 delete. 10/10.)
=======
  [[ "$output" == *"62 add, 1 change, 0 delete"* ]] || { echo "$output" | tail -3; return 1; }
>>>>>>> 3b7af7cc8 (silas: #4960 — AC3: the policy sensor — platform/pipelines/policy-sensor.yaml generated by chorus-make from its rows (hourly at :07, queue policy-sensor one at a time, continue on failure); policy-sensor runs policy-validate --prom in canonical main; land-4960-rows writes the same four graph rows (repo == store); policy-sensor-stale alert (file older than 3 h or never written). 4960-policy-schedule 7/7 with drift, can't-measure and stale negatives; 4959 rule test reads both rule forms.)
  [ ! -f "$T/runner.log" ] || return 1
}

@test "negative proof: a write the door refuses makes the apply exit 1" {
  printf '{"policies": [], "governancechecks": []}' > "$T/live.json"
  printf '#!/bin/sh\necho "403 refused"\nexit 1\n' > "$T/runner"; chmod +x "$T/runner"
  LAND_LIVE_JSON="$T/live.json" CHORUS_RUNNER="$T/runner" LAND_SCOPE_WAIT_CMD=true run python3 "$LAND" --apply
  [ "$status" -eq 1 ] || return 1
  [[ "$output" == *"REFUSED"* ]] || return 1
}

@test "negative proof: an unreadable door is exit 2 and no plan" {
  LAND_LIVE_JSON="$T/nope.json" run python3 "$LAND"
  [ "$status" -eq 2 ] || return 1
}

@test "negative proof: rows carry the door's PolicyCreate types (string baseline, one full-IRI derivedFrom)" {
  # the door answers 422 to an int baseline or a list of slugs (Borg Queen, 11:04)
  run python3 -c "
import json, re, sys
bad = [r['name'] for r in json.load(open('$CHECKS'))
       if not isinstance(r['baseline'], str) or not r['baseline'].isdigit()
       or ('derivedFrom' in r and not (isinstance(r['derivedFrom'], str) and r['derivedFrom'].startswith('https://jeffbridwell.com/chorus#')))]
print(bad); sys.exit(1 if bad else 0)"
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
}
<<<<<<< HEAD
<<<<<<< HEAD

@test "the land grants the card's role Write on policies first, through the security door" {
=======

<<<<<<< HEAD
@test "the land grants Jeff's go Write on policies and domains first, through the security door" {
>>>>>>> 262c8739a (silas: #4960 — AC1: the land writes as Jeff's go, who could not write policies or domains. permission-jeff-policies and permission-jeff-domains in permissions-4183.ttl (count 51, reason in 4183's test), and land-4960-rows adds them first through the security door; a write refused inside the door's 300 s scope cache waits once and retries. No store write before the go. 10/10; live dry run 59 add, 1 change, 34 delete.)
=======
@test "the land grants the card's role Write on policies first, through the security door" {
>>>>>>> 26bd012d4 (silas: #4960 — AC1 (BQ: the land runs as the card's role, not Jeff): one grant, permission-silas-policies (count 50), added first by land-4960-rows; jeff grants dropped. The GovernanceCheck deletes leave the script: the class retirement removes its 34 rows (AC2). Live dry run: 58 add, 1 change, 0 delete. 10/10.)
  printf '{"policies": [], "governancechecks": [], "permissions": []}' > "$T/live.json"
  printf '#!/bin/sh\necho "$*" >> %s/runner.log\necho " → 201"\n' "$T" > "$T/runner"; chmod +x "$T/runner"
  LAND_LIVE_JSON="$T/live.json" CHORUS_RUNNER="$T/runner" run python3 "$LAND" --apply
  [ "$status" -eq 0 ] || { echo "$output" | tail -3; return 1; }
<<<<<<< HEAD
<<<<<<< HEAD
  first=$(head -1 "$T/runner.log")
  [[ "$first" == *"domain=security class=permissions"*"silas-policies"* ]] || { echo "$first"; return 1; }
  grep -q 'chorus:permission-silas-policies a chorus:Permission' "$ROOT/roles/silas/ontology/permissions-4183.ttl" || return 1
=======
  first=$(head -2 "$T/runner.log")
  [[ "$first" == *"domain=security class=permissions"*"jeff-policies"* ]] || { echo "$first"; return 1; }
  [[ "$(sed -n 2p "$T/runner.log")" == *"jeff-domains"* ]] || return 1
  grep -q 'chorus:permission-jeff-policies a chorus:Permission' "$ROOT/roles/silas/ontology/permissions-4183.ttl" || return 1
  grep -q 'chorus:permission-jeff-domains a chorus:Permission' "$ROOT/roles/silas/ontology/permissions-4183.ttl" || return 1
>>>>>>> 262c8739a (silas: #4960 — AC1: the land writes as Jeff's go, who could not write policies or domains. permission-jeff-policies and permission-jeff-domains in permissions-4183.ttl (count 51, reason in 4183's test), and land-4960-rows adds them first through the security door; a write refused inside the door's 300 s scope cache waits once and retries. No store write before the go. 10/10; live dry run 59 add, 1 change, 34 delete.)
=======
  first=$(head -1 "$T/runner.log")
  [[ "$first" == *"domain=security class=permissions"*"silas-policies"* ]] || { echo "$first"; return 1; }
  grep -q 'chorus:permission-silas-policies a chorus:Permission' "$ROOT/roles/silas/ontology/permissions-4183.ttl" || return 1
>>>>>>> 26bd012d4 (silas: #4960 — AC1 (BQ: the land runs as the card's role, not Jeff): one grant, permission-silas-policies (count 50), added first by land-4960-rows; jeff grants dropped. The GovernanceCheck deletes leave the script: the class retirement removes its 34 rows (AC2). Live dry run: 58 add, 1 change, 0 delete. 10/10.)
}

@test "a write refused while the door still holds the old scopes waits the cache out once and retries" {
  printf '{"policies": [], "governancechecks": [], "permissions": []}' > "$T/live.json"
  # refuses every policies write until a marker exists; the wait creates it (LAND_SCOPE_WAIT_CMD)
  printf '#!/bin/sh\ncase "$*" in *class=policies*) [ -f %s/scopes-fresh ] || { echo "→ 403"; exit 1; } ;; esac\necho " → 201"\n' "$T" > "$T/runner"; chmod +x "$T/runner"
  LAND_LIVE_JSON="$T/live.json" CHORUS_RUNNER="$T/runner" LAND_SCOPE_WAIT_CMD="touch $T/scopes-fresh" run python3 "$LAND" --apply
  [ "$status" -eq 0 ] || { echo "$output" | tail -3; return 1; }
  [[ "$output" == *"waited for the door's scopes"* ]] || return 1
}
<<<<<<< HEAD
=======
>>>>>>> b6319870c (silas: #4960 — AC1 review (BQ): baseline as string and derivedFrom as one full IRI (PolicyCreate contract, with a test); the cross-graph rule row re-read whole (a | in its text had split it); a 16th runnable check, arch-css-sole-identity-issuer (passkey/sudoers in source, baseline 11, the #4710 class), with its fixture. 8/8.)
=======
>>>>>>> 262c8739a (silas: #4960 — AC1: the land writes as Jeff's go, who could not write policies or domains. permission-jeff-policies and permission-jeff-domains in permissions-4183.ttl (count 51, reason in 4183's test), and land-4960-rows adds them first through the security door; a write refused inside the door's 300 s scope cache waits once and retries. No store write before the go. 10/10; live dry run 59 add, 1 change, 34 delete.)
