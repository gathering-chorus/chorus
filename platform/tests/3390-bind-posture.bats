#!/usr/bin/env bats
# @test-type: fitness — signal is fixture-data: the guard scripts and ADR text are read,
# and the one live-looking line is a constructed lsof string, not a real socket.
# 3390-bind-posture.bats — internal services bind localhost, not 0.0.0.0 (ADR-012 intent / ADR-042 §8)
# What Jeff sees: nothing on the LAN can reach an internal service that was only
# meant for localhost, and the decision can't be silently lost in a migration again.

REPO="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
DH="$REPO/platform/scripts/deep-health.sh"

@test "deep-health carries the bind-posture guard (check 17)" {
  grep -q "bind-posture" "$DH"
  grep -q "LOCALHOST_ONLY_PORTS" "$DH"
}

@test "guard's localhost-only set covers the known internal services, excludes the LAN exceptions" {
  # #4187 — strip the trailing comment before asserting. The comment names the
  # ports that are deliberately NOT in the set, so matching against the raw line
  # finds 3470 in the prose and calls it a member. The old `! grep -qw` assert was
  # hollow, so this never fired; with a real assert it fails, and the extraction is
  # what is wrong, not the rule.
  line=$(grep "LOCALHOST_ONLY_PORTS=" "$DH" | sed 's/#.*//')
  # internal-only must be checked
  for p in 3344 3352 3475 3030 3306; do echo "$line" | grep -q "$p"; done
  # 3470 deliberately NOT here: clearing serves LAN for #3366's phone URL (unauth-LAN is an auth question, not bind)
  test -z "$(printf '%s' "$line" | grep -ow 3470 || true)"
  # LAN exceptions must NOT be in the localhost-only set (would false-fire)
  for p in 3340 3102 3471 3000; do ! echo "$line" | grep -q "\b$p\b" || { echo "LAN-exception $p wrongly in localhost-only set"; false; }; done
}

@test "guard detects a 0.0.0.0 listener (positive: a bound test port on all-interfaces fires)" {
  # Spin a localhost-only port pattern on 0.0.0.0 and confirm the grep shape matches.
  # (Pattern test — the guard's matcher must catch *:PORT and 0.0.0.0:PORT.)
  echo "node 999 u IPv4 0t0 TCP *:3344 (LISTEN)" | grep -qE "(\*|0\.0\.0\.0):3344\b"
  # #4187 — NOT `! ... | grep -q`: under set -e bash ignores a failure inverted
  # by `!`, so that form asserts nothing unless it is the last line of the test.
  test -z "$(printf '%s' "node 999 u IPv4 0t0 TCP 127.0.0.1:3344 (LISTEN)" | grep -E "(\*|0\.0\.0\.0):3344" || true)"
}

@test "ADR-042 §8 restates the binding rule with the LAN exception list" {
  ADR="$REPO/roles/silas/adr/ADR-042-generator-layer-security-gathering-realm.md"
  grep -q "Network binding" "$ADR"
  grep -q "CHORUS_BIND=127.0.0.1" "$ADR"
  grep -q "3471" "$ADR"   # the mic exception is named
}

@test "deep-health is valid bash" {
  bash -n "$DH"
}
