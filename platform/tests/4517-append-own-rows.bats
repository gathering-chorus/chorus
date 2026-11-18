#!/usr/bin/env bats
# @test-type: unit — athena-make's Append rules through its own unit target; no live services
# @domain: security
# @card: #4517
#
# #4517 AC4 — acl:Append lets a principal create rows and change only its own; acl:Write still
# opens others'. Built 10-13/10-14 (reopen 1); this suite makes it AC4's check. The live half
# (the quartet principal holds Append and its delete legs pass) is measured after the land (AC7).
#
# Covers: platform/services/athena-make/src/lib.rs

setup() {
  ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
}

@test "AC4 Append opens a create, never another owner's row, only on its own graph (negative proofs)" {
  run bash -c "cd '$ROOT/platform/services/athena-make' && cargo test --release --quiet --test create_needs_write_row_4517 2>&1"
  echo "$output" | tail -3
  [ "$status" -eq 0 ]
  echo "$output" | grep -qE "test result: ok\. 8 passed" || return 1
}
