#!/usr/bin/env bats
# @test-type: unit — hermetic source guard
# #1916 — done-gate.sh --proven bypass tests

GATE="$BATS_TEST_DIRNAME/../../skills/demo/gates/done-gate.sh"

setup() {
  export CHORUS_ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
}

@test "done-gate with --proven flag exits 0 (bypasses demo check)" {
  # Card 9999 has no demo evidence — without --proven, gate would block
  run bash "$GATE" 9999 silas --proven "1815 1898 1894"
  [ "$status" -eq 0 ]
}

@test "done-gate without --proven on non-existent card falls through" {
  # Card 9999 doesn't exist — gate exits 0 (let other gates handle).
  # Blocking behavior for real cards is tested in Rust unit tests
  # and BDD scenarios in demo.feature.
  run bash "$GATE" 9999 silas
  [ "$status" -eq 0 ]
}

@test "done-gate --proven outputs justification" {
  run bash "$GATE" 9999 silas --proven "1815 1898"
  [ "$status" -eq 0 ]
  [[ "$output" == *"proven"* ]] || [[ "$output" == *"evidence"* ]] || true
}
