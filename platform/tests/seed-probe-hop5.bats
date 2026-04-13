#!/usr/bin/env bats
# seed-probe-hop5.bats — Tests for seed probe hop 5 fix (#2004)
# What Jeff sees: seed probe reports FAIL on hop 5 every run because
# the probe checks Fuseki for a seed that the handler guard intentionally blocks.
# After the fix: probe checks for the 'Test seed detected' log line instead.

PROBE="/Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/seed-probe.sh"
HANDLER="/Users/jeffbridwell/CascadeProjects/jeff-bridwell-personal-site/src/handlers/seed.handler.ts"

# --- AC 1: Hop 5 checks log line instead of Fuseki persistence ---

@test "hop 5 checks for 'Test seed detected' log line" {
  # Extract hop 5 section
  hop5=$(sed -n '/HOP 5/,/─── \(CLEANUP\|PERMUTATION\)/p' "$PROBE")
  echo "$hop5" | grep -q "Test seed detected"
}

@test "hop 5 does not poll Fuseki SPARQL for probe seed" {
  hop5=$(sed -n '/HOP 5/,/─── \(CLEANUP\|PERMUTATION\)/p' "$PROBE")
  # Should NOT contain the old SPARQL query looking for the probe SID in Fuseki
  ! echo "$hop5" | grep -q 'SPARQL_QUERY'
}

@test "probe defines LOKI_URL for log queries" {
  grep -q 'LOKI_URL=' "$PROBE"
}

@test "probe queries Loki API for log verification" {
  grep -q 'loki/api/v1' "$PROBE"
}

# --- AC 2: Guard stays in place ---

@test "handler guard still blocks SM_PROBE_ seeds" {
  grep -q "SM_PROBE_" "$HANDLER"
}

@test "handler guard still blocks SEED-PROBE content" {
  grep -q "SEED-PROBE" "$HANDLER"
}

@test "handler guard logs 'Test seed detected' before skipping" {
  grep -q "Test seed detected" "$HANDLER"
}

# --- AC 3: Permutations use log check ---

@test "P1 webhook does not use fuseki_has_sid" {
  perms=$(sed -n '/P1:.*Content.*hashtag/,/P2:/p' "$PROBE")
  ! echo "$perms" | grep -q 'fuseki_has_sid'
}

@test "P2 webhook does not use fuseki_has_sid" {
  perms=$(sed -n '/P2:.*Link.*hashtag/,/P3:/p' "$PROBE")
  ! echo "$perms" | grep -q 'fuseki_has_sid'
}

@test "P3 webhook does not use fuseki_has_sid" {
  perms=$(sed -n '/P3:.*Content without/,/P4:/p' "$PROBE")
  ! echo "$perms" | grep -q 'fuseki_has_sid'
}

@test "Fuseki safety purge retained as defense in depth" {
  grep -q 'Purging.*SEED-PROBE' "$PROBE"
}
