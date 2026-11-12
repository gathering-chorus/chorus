#!/usr/bin/env bats
# @test-type: contract — runs policy-validate over fixture policies and a fixture dataset
# @domain: policies — the product domain this suite guards (#4334)
# in BATS_TEST_TMPDIR; no store, no door, no network.
# #4959 AC3 — policy-validate reads the policies, runs each check, and fails only
# when a check finds MORE violations than its baseline. It never passes on a check
# it could not run: an unreadable door or a broken query is exit 2, not green (#3734).

setup() {
  REPO="${CHORUS_ROOT:-$(cd "$BATS_TEST_DIRNAME/../.." && pwd)}"
  PV="$REPO/platform/scripts/policy-validate"
  T="$BATS_TEST_TMPDIR"
  # two subjects typed in two graphs: the one-home rule finds 2 violations here
  cat > "$T/data.trig" <<'EOF'
@prefix c: <https://jeffbridwell.com/chorus#> .
<urn:chorus:ontology> { c:pulse a c:Product . c:werk a c:Product . }
<urn:chorus:instances> { c:pulse a c:Product . c:werk a c:Product . }
EOF
  Q='SELECT DISTINCT ?s WHERE { GRAPH <urn:chorus:ontology> { ?s a ?t1 } GRAPH <urn:chorus:instances> { ?s a ?t2 } }'
  export POLICY_DATA="$T/data.trig"
}

# policies.json in the door's GET shape: {"data": [ ... ]}
policies() {
  python3 - "$T/policies.json" "$@" <<'PY'
import json, sys
out, rows = sys.argv[1], []
for spec in sys.argv[2:]:
    name, gate, status, base, kind, check = spec.split("|", 5)
    r = {"name": name, "label": name, "comment": "c"}
    if gate: r["gate"] = gate
    if status: r["status"] = status
    if base: r["baseline"] = int(base)
    if kind: r["checkKind"] = kind; r["checkSpec"] = check
    rows.append(r)
json.dump({"data": rows}, open(out, "w"))
PY
  export POLICIES_JSON="$T/policies.json"
}

@test "a check at its baseline PASSES (exit 0) and says so" {
  policies "one-home|data|ratchet|2|sparql|$Q"
  run "$PV"
  [ "$status" -eq 0 ]
  [[ "$output" == *"PASS one-home 2 <= 2"* ]] || return 1
}

@test "negative: one violation over the baseline FAILS (exit 1) and names the policy" {
  policies "one-home|data|ratchet|1|sparql|$Q"
  run "$PV"
  [ "$status" -eq 1 ]
  [[ "$output" == *"FAIL one-home 2 > 1"* ]] || return 1
}

@test "--gate runs only that gate's policies" {
  policies "one-home|data|active|0|sparql|$Q" "other|arch|active|5|sparql|$Q"
  run "$PV" --gate arch
  [ "$status" -eq 0 ]
  [[ "$output" == *"PASS other"* ]] || return 1
  [[ "$output" != *"one-home"* ]] || return 1
}

@test "a retired policy does not run" {
  policies "one-home|data|retired|0|sparql|$Q"
  run "$PV"
  [ "$status" -eq 0 ]
  [[ "$output" != *"FAIL"* ]] || return 1
}

@test "a plain policy with no check is not counted as passing or failing" {
  policies "english-only|||||"
  run "$PV"
  [ "$status" -eq 0 ]
  [[ "$output" == *"0 checks"* ]] || return 1
}

@test "negative: a broken query is exit 2, never a pass" {
  policies "broken|data|active|99|sparql|SELECT ?s WHERE { this is not sparql"
  run "$PV"
  [ "$status" -eq 2 ]
  [[ "$output" == *"broken"* ]] || return 1
}

@test "negative: an unreadable door is exit 2, never a pass" {
  export POLICIES_JSON="$T/missing.json"
  run "$PV"
  [ "$status" -eq 2 ]
}

@test "a command check counts the lines it prints as violations" {
  policies "two-lines|code|active|1|command|printf 'a\nb\n'"
  run "$PV"
  [ "$status" -eq 1 ]
  [[ "$output" == *"FAIL two-lines 2 > 1"* ]] || return 1
}
