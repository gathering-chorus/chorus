#!/usr/bin/env bats
# @test-type: unit — the door's supersede, through athena-make's logins tests; no live services
# @domain: identity — the product domain this suite guards (#4334)
#
# #4445 — Jeff, 2026-10-08 08:03: "i now see 2 sessions for abby" / "i used our
# chorus-principal commands 2x". A role has one session, login to logout (#4406): a
# new pane login closes the role's other open pane sessions on the same account.
# Since #4517 the door does it, after the new row is written (chorus-principal writes no
# Session row), so this suite runs the door's tests, and the old harness cases retired.
#
# Covers: platform/services/athena-make/src/logins.rs

setup() { ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"; }

one_ok() { # cargo test args — the named test ran and passed
  run bash -c "cd '$ROOT/platform/services/athena-make' && cargo test --release --quiet --test logins_4517 $* 2>&1"
  echo "$output" | tail -3
  [ "$status" -eq 0 ] || return 1
  echo "$output" | grep -qE "test result: ok\. 1 passed" || return 1
}

@test "a new pane login closes the role's older open pane session, only after its own row is written" {
  one_ok a_new_pane_login_supersedes_only_after_its_row_is_written
}

@test "NEGATIVE PROOF: another role's session, a closed row and another channel are never closed" {
  one_ok the_supersede_query_names_only_this_principals_open_pane_rows_on_this_account
  one_ok negative_proofs_a_failed_create_or_a_browser_login_supersedes_nothing
}
