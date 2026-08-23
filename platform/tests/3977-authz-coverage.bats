#!/usr/bin/env bats
# @test-type: integration — pure scoring subcommand of the script under test; no live store, no token
# #3977 — negative proof (#3734) for the authz-coverage scorer. authZ's verdict
# turns on ONE distinction the tool must never blur: a 403 (scope refused) is the
# ONLY covered state. A 200 is OPEN (under-scoped caller mutated), and — the trap —
# a 401 is MISPROBE, NOT covered: authn refused first, so authz was never reached.
# If 401 scored COVERED, an endpoint with NO authz but working authn would read as
# authorized. That is the exact false-pass this proof forbids.

SCRIPT="${BATS_TEST_DIRNAME}/../scripts/authz-coverage.sh"
# Covers: platform/scripts/authz-coverage.sh  (literal path for the #3917 linker)

@test "403 (out-of-scope) is the only COVERED verdict" {
  [ "$(bash "$SCRIPT" score 403)" = COVERED ]
}

@test "NEGATIVE PROOF: 200 is OPEN — an under-scoped caller that mutated is not covered" {
  run bash "$SCRIPT" score 200
  [ "$output" = OPEN ]
  [ "$output" != COVERED ]
}

@test "NEGATIVE PROOF: 401 is MISPROBE, never COVERED — authn refused first, authz unmeasured" {
  run bash "$SCRIPT" score 401
  [ "$output" = MISPROBE ]
  [ "$output" != COVERED ]
}

@test "NEGATIVE PROOF: a 404/000 route miss is MISPROBE, never COVERED" {
  [ "$(bash "$SCRIPT" score 404)" = MISPROBE ]
  [ "$(bash "$SCRIPT" score 000)" = MISPROBE ]
}

@test "a non-403 refusal (500) is OPEN, not COVERED — only a scope 403 counts" {
  [ "$(bash "$SCRIPT" score 500)" = OPEN ]
}

# #3979 — the prefix-aware run classifies a NOT-scope-covered route by its other
# auth mechanism. NEGATIVE PROOF (#3734): the default MUST be GAP — a route that
# is neither Clearing (bridge) nor loopback is an unprotected mutating route. If
# justify() defaulted to anything else, a new unguarded chorus-api route would be
# silently absolved and "REAL GAP: 0" would become a vacuous pass.
@test "justify(): a Clearing route is BRIDGE" {
  [ "$(bash "$SCRIPT" justify /api/message)" = BRIDGE ]
  [ "$(bash "$SCRIPT" justify /api/chat/x/message)" = BRIDGE ]
}

@test "justify(): a loopback route is LOOPBACK" {
  [ "$(bash "$SCRIPT" justify /mcp)" = LOOPBACK ]
  [ "$(bash "$SCRIPT" justify /api/nudge)" = LOOPBACK ]
}

@test "NEGATIVE PROOF: an unknown mutating route defaults to GAP, never justified" {
  run bash "$SCRIPT" justify /api/evil/mutate
  [ "$output" = GAP ]
  [ "$output" != BRIDGE ]
  [ "$output" != LOOPBACK ]
}
