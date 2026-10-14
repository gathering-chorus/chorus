#!/usr/bin/env bats
# @test-type: unit
# @domain: security — the security ledger measures its own tree (#3716)
# #4515 — the ledger's TS-minter leg read platform/mcp-server/src, which #4515
# deletes; a grep on a missing dir exits 2 and the ledger would report the item
# unknown forever. This runs the ledger's own test (it pins the measured state and
# the unknown-not-zero rule) so werk-test exercises the script.

ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"

@test "the security ledger's own test passes against this tree" {
  run bash "$ROOT/platform/scripts/test-security-distance-to-done.sh"
  echo "$output" | tail -5
  [ "$status" -eq 0 ]
  echo "$output" | grep -q '^PASS$' || return 1
}

@test "NEGATIVE PROOF: pointing the minter leg at a missing dir reports unknown, never 0" {
  bad="$BATS_TEST_TMPDIR/ledger.sh"
  sed 's|"\$C/platform/chorus-sdk/src" 2>/dev/null|"$C/platform/chorus-sdk/src" "$C/platform/no-such-dir" 2>/dev/null|' \
    "$ROOT/platform/scripts/security-distance-to-done.sh" > "$bad"
  ! cmp -s "$bad" "$ROOT/platform/scripts/security-distance-to-done.sh" || return 1
  run env CHORUS_ROOT="$ROOT" SECURITY_LEDGER_PROM="$BATS_TEST_TMPDIR/x.prom" bash "$bad"
  echo "$output" | grep -E 'hs256_minters' 
  echo "$output" | grep -qE 'hs256_minters +unknown' || return 1
}
