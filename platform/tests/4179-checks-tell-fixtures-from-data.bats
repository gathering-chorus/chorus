#!/usr/bin/env bats
# @test-type: fitness — greps source text in a temp tree; no service, no store.
#
# #4179 — the three checks #4175 tripped now EXCLUDE fixture data, and this
# proves the exclusion narrowed them rather than blinded them.
#
# The trap it closes: the negative-proof discipline (#3734) requires committing
# violation fixtures — rows that break a rule on purpose, so a check can be
# SHOWN to fire. A repo-wide grep cannot tell such a fixture from a real
# declaration. 3838-roles-model tests 8 and 10 read
# platform/tests/fixtures/hats-4175-violations.ttl as production data and went
# red two nightlies running, 2026-09-15 and 2026-09-16.
#
# Exclusion is by DIRECTORY, never by pattern. A pattern exception would also
# hide a real violation that happened to resemble test data — which is the same
# defect wearing the fix's clothes.

setup() {
  T="$BATS_TEST_TMPDIR/tree"
  mkdir -p "$T/platform/tests/fixtures" "$T/roles/wren/ontology" "$T/platform/scripts"
  # a fixture that violates on purpose — must be IGNORED
  cat > "$T/platform/tests/fixtures/violations.ttl" <<'TTL'
@prefix chorus: <https://jeffbridwell.com/chorus#> .
chorus:product-x chorus:ownedBy chorus:wren .
chorus:wren a chorus:Role .
TTL
}

plant_real_violation() {
  cat > "$T/roles/wren/ontology/real.ttl" <<'TTL'
@prefix chorus: <https://jeffbridwell.com/chorus#> .
chorus:product-y chorus:ownedBy chorus:silas .
chorus:silas a chorus:Role .
TTL
}

ownership_hits() {
  ( cd "$T" && grep -rhoE 'chorus:(ownedBy|ownerRole|gatekeeper|assignedTo) chorus:(wren|silas|kade|jeff)\b' \
      --include='*.ttl' --exclude-dir=fixtures . | wc -l | tr -d ' ' )
}

declaration_hits() {
  ( cd "$T" && grep -rh --include='*.ttl' --exclude-dir=fixtures -v '^[[:space:]]*#' . \
      | grep -cE 'chorus:(wren|silas|kade|jeff) a chorus:Role\b' || true )
}

@test "#4179 a violation fixture is NOT read as production data" {
  [ "$(ownership_hits)" -eq 0 ]
  [ "$(declaration_hits)" -eq 0 ]
}

@test "#4179 NEGATIVE PROOF: a real violation outside fixtures is still caught" {
  plant_real_violation
  [ "$(ownership_hits)" -eq 1 ]
  [ "$(declaration_hits)" -eq 1 ]
}

@test "#4179 NEGATIVE PROOF: the path check finds the retired DIRECTORY" {
  # the alias check hunts roles/product-manager, and must not fire on a hat
  # legitimately named "product manager".
  printf '%s\n' 'HATS = ["hat-product-manager", "hat-operations-lead"]' > "$T/platform/scripts/gen.py"
  run bash -c "grep -rnE '/product-manager(/|\"|'\"'\"'|\$)' '$T/platform/scripts'/ || true"
  [ -z "$output" ]
  printf '%s\n' 'ROLE_DIR="$REPO/roles/product-manager/state.md"' > "$T/platform/scripts/stale.sh"
  run bash -c "grep -rnE '/product-manager(/|\"|'\"'\"'|\$)' '$T/platform/scripts'/ || true"
  [ -n "$output" ]
}
