#!/usr/bin/env bats
# @test-type: unit — the real policy-validate over a fixture dataset, then the real demo gate over the same; no store, no door, no network.
# @domain: policies
# @card: #4959
# @owner: silas
#
# #4959 AC5 — negative proof (#3734): one fixture, both states. A synthetic violation
# that puts the count at baseline + 1 fails policy-validate (exit 1) AND the demo gate
# that runs it; the same data at its baseline passes both. A check that could only go
# one way would fail one of the two.

ROOT="$BATS_TEST_DIRNAME/../.."
PV="$ROOT/platform/scripts/policy-validate"
CRATE="$ROOT/platform/services/werk-demo"

setup() {
  T="$BATS_TEST_TMPDIR"
  cat > "$T/data.trig" <<'EOF'
@prefix c: <https://jeffbridwell.com/chorus#> .
<urn:chorus:ontology> { c:pulse a c:Product . c:werk a c:Product . }
<urn:chorus:instances> { c:pulse a c:Product . c:werk a c:Product . }
EOF
  export POLICY_DATA="$T/data.trig"
}

policy_at() {
  printf '{"data":[{"name":"one-home","gate":"data","status":"ratchet","baseline":"%s","checkKind":"sparql","checkSpec":"SELECT DISTINCT ?s WHERE { GRAPH <urn:chorus:ontology> { ?s a ?t1 } GRAPH <urn:chorus:instances> { ?s a ?t2 } }"}]}' "$1" > "$T/p.json"
  export POLICIES_JSON="$T/p.json"
}

@test "negative proof: count == baseline + 1 makes policy-validate exit 1" {
  policy_at 1
  run "$PV" --gate data
  [ "$status" -eq 1 ]
  [[ "$output" == *"FAIL one-home 2 > 1"* ]] || return 1
}

@test "control: the same data at its baseline exits 0" {
  policy_at 2
  run "$PV" --gate data
  [ "$status" -eq 0 ]
}

@test "negative proof: the demo gate running the real checker fails at baseline + 1 and passes at the baseline" {
  CARGO_TARGET_DIR="$CRATE/target/$(id -un)" run cargo test --manifest-path "$CRATE/Cargo.toml" --test policy_gates_4959 \
    negative_proof_ac5_baseline_plus_one_fails_the_real_gate_and_the_baseline_passes -- --test-threads=1
  [ "$status" -eq 0 ] || { echo "$output" | tail -20; return 1; }
  [[ "$output" == *"test negative_proof_ac5_baseline_plus_one_fails_the_real_gate_and_the_baseline_passes ... ok"* ]] || return 1
}
