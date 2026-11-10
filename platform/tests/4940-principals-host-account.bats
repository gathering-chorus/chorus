#!/usr/bin/env bats
# @test-type: unit — signal:identity is fixture-data (the model file's own rows)
# @domain: identity
# #4940 — every agent Principal in the model names its own macOS account.
# 2026-11-09 17:29: #4773's model deploy rewrote the principals from this file, which said
# hostAccount "jeffbridwell" for silas, wren and kade, and every Claude role's tools were
# refused ("no principal runs as account chorus-silas") until 18:32. Second time (10-28).
#
# NEGATIVE PROOF (#3734): a fixture with hostAccount "jeffbridwell" on an agent fails the check.

setup() {
  ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
  MODEL="$ROOT/roles/silas/ontology/identity-principals-3613.ttl"
  TMP="$(mktemp -d)"
}

teardown() { rm -rf "$TMP"; }

# Prints "<name> <hostAccount>" for each agent Principal whose hostAccount is not chorus-<name>.
wrong_agents() {
  awk '
    /^chorus:principal-[a-z-]+ a chorus:Principal/ { name=$1; sub(/^chorus:principal-/, "", name); kind=""; host="" }
    /chorus:principalKind/ { match($0, /"[^"]*"/); kind=substr($0, RSTART+1, RLENGTH-2) }
    /chorus:hostAccount/   { match($0, /"[^"]*"/); host=substr($0, RSTART+1, RLENGTH-2) }
    /[.][ \t]*(#.*)?$/ && name != "" {
      if (kind == "agent" && host != "chorus-" name) print name, (host == "" ? "(none)" : host)
      name=""
    }
  ' "$1"
}

@test "4940 AC1: every agent Principal in the model runs as its own chorus-<role> account" {
  run wrong_agents "$MODEL"
  [ "$status" -eq 0 ]
  [ -z "$output" ] || { echo "agents not on their own account: $output"; return 1; }
  # the guard is looking at real rows, never vacuously empty
  for r in silas wren kade abby-normal borg-queen; do
    grep -q "^chorus:principal-$r a chorus:Principal" "$MODEL"
  done
}

@test "4940 AC1 NEGATIVE PROOF: an agent with hostAccount jeffbridwell is caught" {
  cat > "$TMP/bad.ttl" <<'EOF'
chorus:principal-silas a chorus:Principal ;
    chorus:principalKind "agent" ;
    chorus:hostAccount "jeffbridwell" ;   # the 11-09 shape
    chorus:canSignIn "false" .

chorus:principal-jeff a chorus:Principal ;
    chorus:principalKind "person" ;
    chorus:hostAccount "jeffbridwell" .
EOF
  run wrong_agents "$TMP/bad.ttl"
  [ "$output" = "silas jeffbridwell" ]
}

@test "4940 AC2: land-4773-rows is gone; the model deploy seeds the deployer rows" {
  [ ! -e "$ROOT/platform/scripts/land-4773-rows" ]
  grep -q '^chorus:principal-deployer a chorus:Principal' "$MODEL"
}

@test "4940 AC2 NEGATIVE PROOF: the check fails while a land-rows file exists" {
  mkdir -p "$TMP/platform/scripts" && touch "$TMP/platform/scripts/land-4773-rows"
  run test ! -e "$TMP/platform/scripts/land-4773-rows"
  [ "$status" -ne 0 ]
}
