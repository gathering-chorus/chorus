#!/usr/bin/env bats
# @test-type: contract — reads the tree and runs policy-validate's usage; no store, no network.
# @domain: policies
# @card: #5020 · owner: kade
# What Jeff sees: one policy runner. The domain lint is a flag on policy-validate (#4959),
# not a second tool.
#
# AC4: policy-validate takes --domain; the tree has no domain-lint tool. NEGATIVE: an unknown
# flag is refused (exit 2), so the flag is really parsed, not ignored.

setup() {
  REPO="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
  PV="$REPO/platform/scripts/policy-validate"
}

@test "AC4: no new runner: the lint is policy-validate's --domain, and the tree has no domain-lint tool" {
  grep -q -- '--domain <name>' "$PV"
  run bash -c "cd '$REPO' && git ls-files platform/scripts | grep -iE 'domain-?lint'"
  [ -z "$output" ] || { echo "$output"; return 1; }
}

@test "AC4 NEGATIVE: policy-validate refuses a flag it does not know, and a --domain with no name" {
  run "$PV" --lint principles
  [ "$status" -eq 2 ] || { echo "$output"; return 1; }
  run "$PV" --domain
  [ "$status" -eq 2 ] || { echo "$output"; return 1; }
}
