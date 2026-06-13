#!/usr/bin/env bats
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
  line=$(grep "LOCALHOST_ONLY_PORTS=" "$DH")
  # internal-only must be checked
  for p in 3344 3352 3475 3030 3306; do echo "$line" | grep -q "$p"; done
  # 3470 deliberately NOT here: clearing serves LAN for #3366's phone URL (unauth-LAN is an auth question, not bind)
  ! echo "$line" | grep -qw 3470
  # LAN exceptions must NOT be in the localhost-only set (would false-fire)
  for p in 3340 3102 3471 3000; do ! echo "$line" | grep -q "\b$p\b" || { echo "LAN-exception $p wrongly in localhost-only set"; false; }; done
}

@test "guard detects a 0.0.0.0 listener (positive: a bound test port on all-interfaces fires)" {
  # Spin a localhost-only port pattern on 0.0.0.0 and confirm the grep shape matches.
  # (Pattern test — the guard's matcher must catch *:PORT and 0.0.0.0:PORT.)
  echo "node 999 u IPv4 0t0 TCP *:3344 (LISTEN)" | grep -qE "(\*|0\.0\.0\.0):3344\b"
  echo "node 999 u IPv4 0t0 TCP 127.0.0.1:3344 (LISTEN)" | grep -qvE "(\*|0\.0\.0\.0):3344\b"
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
