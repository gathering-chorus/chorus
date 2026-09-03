#!/usr/bin/env bats
# @test-type: unit — runs commitment-check.sh over the tests service's real Commitment rows and over fixtures; no store, no service, no network.
# #4089 AC3: the #4064 check runs on the tests service. The real file is read as
# it is committed; the negative proof removes a card from an open row and shows
# the check go RED for exactly that row (#3734).
setup() {
  ROOT="$BATS_TEST_DIRNAME/../.."
  CHECK="$ROOT/platform/scripts/commitment-check.sh"
  TTL="$ROOT/roles/kade/ontology/tests-commitments-4089.ttl"
  export COMMITMENT_PROBE_ROOT="$ROOT"
  export COMMITMENT_PROBES=0   # hermetic: probes are named, never run, here
  command -v sparql >/dev/null || skip "sparql (jena) not on PATH"
}

@test "the tests service rows parse and the check reads every one of them" {
  n=$(grep -c "a chorus:Commitment" "$TTL")
  run env COMMITMENT_TTL="$TTL" "$CHECK"
  echo "$output" | grep -q "commitments=$n " || { echo "$output"; false; }
  # every open row without a card is named — the reason the rows exist
  open_no_card=$(echo "$output" | grep -c "^FAIL open-no-card")
  [ "$open_no_card" -gt 0 ] || { echo "no unowned open commitment reported — either every promise is carded (update this test) or the check went blind"; echo "$output"; false; }
  [ "$status" -eq 1 ]
}

@test "every probe a row names is a file in the tree (the check runs it by kind: .bats via bats, .sh via bash)" {
  missing=""
  for p in $(grep -o 'chorus:probe "[^"]*"' "$TTL" | cut -d'"' -f2); do
    [ -f "$ROOT/$p" ] || missing="$missing $p"
  done
  [ -z "$missing" ] || { echo "probes not in tree:$missing"; false; }
}

@test "NEGATIVE PROOF: removing the card from a carded open row turns that row red" {
  sed 's/chorus:card chorus:card-4017 \./ ./' "$TTL" > "$BATS_TEST_TMPDIR/one-uncarded.ttl"
  run env COMMITMENT_TTL="$BATS_TEST_TMPDIR/one-uncarded.ttl" "$CHECK"
  echo "$output" | grep -q "^FAIL open-no-card commitment-one-binary$" || { echo "$output"; false; }
  # and the control: the committed file does NOT fail that row
  run env COMMITMENT_TTL="$TTL" "$CHECK"
  ! echo "$output" | grep -q "^FAIL open-no-card commitment-one-binary$"
}

@test "NEGATIVE PROOF: a closed row whose probe goes red is reported red" {
  printf '#!/bin/bash\nexit 1\n' > "$BATS_TEST_TMPDIR/red.sh"; chmod +x "$BATS_TEST_TMPDIR/red.sh"
  # only the row under test keeps a probe; every other probe is a real test and stays unrun
  sed -e 's#chorus:probe "platform/[^"]*"#chorus:probe-removed "x"#' \
      -e "s#chorus:probe-removed \"x\" \.\(.*nightly-least-privilege\)#IGNORE#" "$TTL" \
    | sed "/commitment-nightly-least-privilege-principal a chorus:Commitment/,/ \./ s#chorus:probe-removed \"x\"#chorus:probe \"$BATS_TEST_TMPDIR/red.sh\"#" \
    > "$BATS_TEST_TMPDIR/red-probe.ttl"
  grep -q "red.sh" "$BATS_TEST_TMPDIR/red-probe.ttl"
  run env COMMITMENT_PROBES=1 COMMITMENT_TTL="$BATS_TEST_TMPDIR/red-probe.ttl" "$CHECK"
  echo "$output" | grep -q "^FAIL closed-probe-red commitment-nightly-least-privilege-principal" || { echo "$output"; false; }
}
