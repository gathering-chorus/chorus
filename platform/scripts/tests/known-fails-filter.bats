#!/usr/bin/env bats

# #2497 — known-fails-filter.sh: pre-commit honors carded test failures.
#
# Contract: takes test framework + name on stdin or args, consults
# platform/state/known-fails.json, returns 0 if all failures are allowlisted,
# 1 otherwise. CI does NOT use this — pre-commit only.

setup() {
  FILTER="$BATS_TEST_DIRNAME/../known-fails-filter.sh"
  ALLOWLIST="$BATS_TEST_TMPDIR/known-fails.json"
  [ -f "$FILTER" ] || skip "known-fails-filter.sh not yet present"

  cat > "$ALLOWLIST" <<'JSON'
{
  "schema_version": 1,
  "entries": [
    {
      "test_id": "shared::protocol_contract::tests::matches_python_test_vectors",
      "framework": "cargo",
      "card_id": 2644,
      "reason": "fixture stale, fix in card #2644",
      "filed_at": "2026-05-01T19:30:00Z"
    },
    {
      "test_id": "session.test.ts > foo bar baz",
      "framework": "jest",
      "card_id": 2493,
      "reason": "session blocking pattern",
      "filed_at": "2026-04-25T18:24:42Z"
    }
  ]
}
JSON
}

@test "prints help when invoked with no args" {
  run bash "$FILTER"
  [ "$status" -ne 0 ]
  [[ "$output" == *"Usage"* ]] || [[ "$output" == *"usage"* ]]
}

@test "is-allowed returns 0 for an allowlisted cargo test" {
  KNOWN_FAILS_FILE="$ALLOWLIST" run bash "$FILTER" is-allowed cargo "shared::protocol_contract::tests::matches_python_test_vectors"
  [ "$status" -eq 0 ]
}

@test "is-allowed returns 1 for a non-allowlisted cargo test" {
  KNOWN_FAILS_FILE="$ALLOWLIST" run bash "$FILTER" is-allowed cargo "some::other::test"
  [ "$status" -ne 0 ]
}

@test "is-allowed respects framework boundary (cargo entry not allowed for jest framework)" {
  KNOWN_FAILS_FILE="$ALLOWLIST" run bash "$FILTER" is-allowed jest "shared::protocol_contract::tests::matches_python_test_vectors"
  [ "$status" -ne 0 ]
}

@test "is-allowed returns 0 for an allowlisted jest test" {
  KNOWN_FAILS_FILE="$ALLOWLIST" run bash "$FILTER" is-allowed jest "session.test.ts > foo bar baz"
  [ "$status" -eq 0 ]
}

@test "missing allowlist file: nothing is allowed (fail-closed)" {
  KNOWN_FAILS_FILE="/tmp/nonexistent-known-fails.json" run bash "$FILTER" is-allowed cargo "anything"
  [ "$status" -ne 0 ]
}

@test "list-cards prints unique card_ids referenced by allowlist" {
  KNOWN_FAILS_FILE="$ALLOWLIST" run bash "$FILTER" list-cards
  [ "$status" -eq 0 ]
  [[ "$output" == *"2644"* ]]
  [[ "$output" == *"2493"* ]]
}

@test "count returns total entry count" {
  KNOWN_FAILS_FILE="$ALLOWLIST" run bash "$FILTER" count
  [ "$status" -eq 0 ]
  [ "$output" = "2" ]
}

@test "filter-cargo: subtracts allowlisted failures from cargo test output, returns 0 if all allowlisted" {
  cat > "$BATS_TEST_TMPDIR/cargo-output.txt" <<'EOF'
running 392 tests
test shared::protocol_contract::tests::matches_python_test_vectors ... FAILED
test result: FAILED. 391 passed; 1 failed; 0 ignored
EOF
  KNOWN_FAILS_FILE="$ALLOWLIST" run bash "$FILTER" filter-cargo "$BATS_TEST_TMPDIR/cargo-output.txt"
  [ "$status" -eq 0 ]
  [[ "$output" == *"known-fails-filter: 1 allowlisted"* ]] || [[ "$output" == *"all failures allowlisted"* ]]
}

@test "filter-cargo: returns 1 if any non-allowlisted failure remains" {
  cat > "$BATS_TEST_TMPDIR/cargo-output.txt" <<'EOF'
test shared::protocol_contract::tests::matches_python_test_vectors ... FAILED
test some::other::test ... FAILED
test result: FAILED. 390 passed; 2 failed
EOF
  KNOWN_FAILS_FILE="$ALLOWLIST" run bash "$FILTER" filter-cargo "$BATS_TEST_TMPDIR/cargo-output.txt"
  [ "$status" -ne 0 ]
  [[ "$output" == *"some::other::test"* ]]
}
