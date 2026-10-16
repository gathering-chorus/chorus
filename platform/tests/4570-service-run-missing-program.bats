#!/usr/bin/env bats
# @test-type: unit — runs service-run directly; needs no launchd and no built binary.
# @domain: services
#
# #4570 — Silas's review: com.chorus.clearing-v2 pins nvm node v20.8.0 (the node the
# firewall allows). If nvm drops it, service-run must name the missing path and exit 127,
# not respawn an unnamed failure under KeepAlive.

bats_require_minimum_version 1.5.0

setup() { ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"; }

@test "NEGATIVE PROOF: a missing program path is named and exits 127" {
  run -127 "$ROOT/platform/scripts/service-run" com.chorus.zz-missing daemon /nonexistent/nvm/v0.0.0/bin/node x.js
  [ "$status" -eq 127 ]
  [[ "$output" == *"program not found or not executable: /nonexistent/nvm/v0.0.0/bin/node"* ]] || return 1
}

@test "a present program still runs and exits with its own code" {
  run -3 "$ROOT/platform/scripts/service-run" com.chorus.zz-present job /bin/sh -c 'exit 3'
  [ "$status" -eq 3 ]
  [[ "$output" != *"program not found"* ]] || return 1
}
