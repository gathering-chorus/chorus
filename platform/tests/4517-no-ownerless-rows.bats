#!/usr/bin/env bats
# @test-type: unit — the door's create stamp and claim branch, and the DAL's claim, through
# their own test targets; no live services
# @domain: security
# @card: #4517
#
# #4517 AC3 (narrowed by Jeff 11-18) — a new row gets its owner from the verified login, never
# the body; an ownerless row can be claimed only by a Write holder, with exactly {ownedBy:
# itself}, checked atomically at the DAL. The live backfill (14 shapes, 23 rows, 577
# unreadable, zz-probe) is #5002. Negative proofs in each target: a body ownedBy naming
# someone else is ignored; a claim from a caller with no Write, a claim naming another owner,
# a claim on an owned row and a claim on a missing row are all refused.
#
# Covers: platform/services/athena-make/src/lib.rs
# Covers: platform/services/athena-make/src/claims.rs
# Covers: platform/services/athena-model/src/lib.rs

setup() { ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"; }

one_ok() { # $1 crate dir, rest: cargo test args — exactly the named test ran and passed
  local dir="$1"; shift
  run bash -c "cd '$ROOT/platform/services/$dir' && cargo test --release --quiet $* 2>&1"
  echo "$output" | tail -3
  [ "$status" -eq 0 ] || return 1
  echo "$output" | grep -qE "test result: ok\. [1-9][0-9]* passed" || return 1
}

@test "AC3 a new row's owner is the verified caller; a body ownedBy is ignored" {
  one_ok athena-make --lib required_owner_is_satisfied_by_the_verified_caller_not_the_body
}

@test "AC3 the door's claim: only {ownedBy: itself}, only on an ownerless row, only with Write" {
  one_ok athena-make --test coverage_3701_hermetic a_claim_on_an_ownerless_row_reaches_the_dal_and_nothing_else_does_4517
}

@test "AC3 the DAL's claim: the Write holder claims for itself; owned, unwritable and missing rows refused" {
  one_ok athena-model --lib the_write_holder_claims_an_ownerless_row_for_itself
  one_ok athena-model --lib negative_proofs_owned_unwritable_and_missing_rows_are_refused
}
