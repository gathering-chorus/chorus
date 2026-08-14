#!/usr/bin/env bats
# @test-type: unit — signal is fixture-data: arq/generated files only; no live store, no $HOME, no network
# #3870 — service-drift check: negative proofs FIRST (Wren's pair directive:
# the fixtures exist before the harvester, so the check cannot be shaped to
# match whatever the harvester produces). Hermetic: every case runs arq over
# a fixture file — no Fuseki, no live store, no $HOME state (#3528 contract).

setup() {
  DIR="$( cd "$( dirname "$BATS_TEST_FILENAME" )" && pwd )"
  CHECK="$DIR/../scripts/service-drift-check.sh"
  FIX="$DIR/fixtures/service-drift"
}

@test "RED: an unmapped instance fails the check and is NAMED" {
  run bash "$CHECK" --data "$FIX/unmapped-instance.ttl"
  [ "$status" -eq 1 ]
  [[ "$output" == *"com.example.mystery"* ]]
  [[ "$output" == *"unmapped"* ]]
}

@test "GREEN: a mapped, running, fresh instance passes" {
  run bash "$CHECK" --data "$FIX/mapped-fresh.ttl"
  [ "$status" -eq 0 ]
  [[ "$output" == *"green"* ]]
}

@test "RED: a vanished (absent) instance fails even though it is mapped" {
  run bash "$CHECK" --data "$FIX/vanished-instance.ttl"
  [ "$status" -eq 1 ]
  [[ "$output" == *"com.example.gone"* ]]
  [[ "$output" == *"vanished"* ]]
}

@test "RED: a stale instance (lastObserved beyond cutoff) fails" {
  run bash "$CHECK" --data "$FIX/stale-instance.ttl"
  [ "$status" -eq 1 ]
  [[ "$output" == *"com.example.stale"* ]]
  [[ "$output" == *"stale"* ]]
}

@test "GREEN: the empty graph passes — check works before any harvest exists" {
  run bash "$CHECK" --data "$FIX/empty.ttl"
  [ "$status" -eq 0 ]
}

@test "REFUSE: missing fixture is exit 2, never a quiet green" {
  run bash "$CHECK" --data "$FIX/does-not-exist.ttl"
  [ "$status" -eq 2 ]
}

@test "ONE QUERY: fixture and live paths run the same .rq file" {
  # Wren's pin: if the live path ever gets its own query (or an inline one),
  # the fixtures keep proving a query nobody runs. Assert the script derives
  # BOTH paths from a single QUERY_SRC and contains no second query source.
  # Negative proof: adding another '.rq' reference or an inline SELECT to the
  # script flips these counts and this test goes red.
  # count CODE references only — a comment mentioning .rq is the #3725 trap
  # (the marker string living in a comment) in the other direction.
  refs="$(grep -v '^\s*#' "$CHECK" | grep -c '\.rq')"
  [ "$refs" -eq 1 ]                       # exactly one .rq reference: QUERY_SRC
  inline="$(grep -cE '^\s*(SELECT|PREFIX)\b' "$CHECK" || true)"
  [ "$inline" -eq 0 ]                     # no inline SPARQL smuggled in
  # both execution branches consume the same injected copy "$Q"
  [ "$(grep -c 'query "\$Q"\|query@\$Q' "$CHECK")" -eq 2 ]
}

@test "GREEN: an EXTERNAL unmapped instance is not a finding (separate count)" {
  # paired with test 1: identical shape minus chorus:external → RED. Both
  # directions proven — external must not blind the check to OUR unmapped.
  run bash "$CHECK" --data "$FIX/external-unmapped.ttl"
  [ "$status" -eq 0 ]
}

@test "GREEN: evidence-unavailable (unknown) is not an unmapped finding" {
  # paired with test 1 (same shape, no evidenceState → RED): could-not-look
  # is an ops gap with its own count, never "ours, unexplained".
  run bash "$CHECK" --data "$FIX/unknown-evidence.ttl"
  [ "$status" -eq 0 ]
}

@test "RED: a stale mapping row is a named finding" {
  run bash "$CHECK" --data "$FIX/stale-mapping.ttl"
  [ "$status" -eq 1 ]
  [[ "$output" == *"com.test.ghost"* ]]
  [[ "$output" == *"stale-mapping"* ]]
}
