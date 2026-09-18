#!/usr/bin/env bats
# @test-type: unit — fixture-data, not a suite: this file exists to be READ by the #4201 placement proofs, never run.
# #4201 — fixture, not a suite. A card number in the header and nothing else:
# no route, no binary, no class, no import. The retired folder rule read a file
# like this as `services` because of where it sat. It must come out unplaced.

@test "a bare card number is not a domain" {
  run true
  [ "$status" -eq 0 ]
}
