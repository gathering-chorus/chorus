#!/usr/bin/env bats
# #2927 AC1 — Unit resolution.
# Default = git diff origin/main introspection. --units <list> overrides.
# Tests cover the pure resolution functions (no shell-out to git in this file;
# introspect is tested with a fixture werk in ac1-deploy-units-introspect.bats).

SCRIPT="$BATS_TEST_DIRNAME/../scripts/deploy-daemon-card.sh"

setup() {
  # Source the script to expose internal functions. The script's main flow
  # is guarded by a BASH_SOURCE check so sourcing doesn't execute deploy.
  # shellcheck disable=SC1090
  source "$SCRIPT"
}

@test "KNOWN_UNITS lists the three deploy units" {
  [ "$KNOWN_UNITS" = "chorus-api chorus-hooks cards-sdk" ]
}

@test "unit_pattern: chorus-api → ^platform/api/" {
  [ "$(unit_pattern chorus-api)" = '^platform/api/' ]
}

@test "unit_pattern: chorus-hooks → ^platform/services/chorus-hooks/" {
  [ "$(unit_pattern chorus-hooks)" = '^platform/services/chorus-hooks/' ]
}

@test "unit_pattern: cards-sdk → ^directing/products/cards/" {
  [ "$(unit_pattern cards-sdk)" = '^directing/products/cards/' ]
}

@test "unit_pattern: unknown unit returns non-zero" {
  run unit_pattern bogus
  [ "$status" -ne 0 ]
}

@test "match_units_against_paths: chorus-api path match" {
  result=$(echo "platform/api/src/server.ts" | match_units_against_paths)
  [ "$result" = "chorus-api" ]
}

@test "match_units_against_paths: chorus-hooks path match" {
  result=$(echo "platform/services/chorus-hooks/src/hooks/foo.rs" | match_units_against_paths)
  [ "$result" = "chorus-hooks" ]
}

@test "match_units_against_paths: cards-sdk path match" {
  result=$(echo "directing/products/cards/src/sdk.ts" | match_units_against_paths)
  [ "$result" = "cards-sdk" ]
}

@test "match_units_against_paths: multi-unit diff returns all matched" {
  paths="platform/api/src/x.ts
platform/services/chorus-hooks/src/y.rs
directing/products/cards/src/z.ts"
  result=$(echo "$paths" | match_units_against_paths | sort -u)
  [ "$result" = "cards-sdk
chorus-api
chorus-hooks" ]
}

@test "match_units_against_paths: unrelated path returns empty" {
  result=$(echo "docs/readme.md" | match_units_against_paths)
  [ -z "$result" ]
}

@test "match_units_against_paths: TEAM_PROTOCOL.md not a deploy unit" {
  result=$(echo "TEAM_PROTOCOL.md" | match_units_against_paths)
  [ -z "$result" ]
}

@test "resolve_units_explicit: single unit" {
  result=$(resolve_units_explicit "chorus-hooks")
  [ "$result" = "chorus-hooks" ]
}

@test "resolve_units_explicit: comma list" {
  result=$(resolve_units_explicit "chorus-hooks,cards-sdk")
  [ "$result" = "chorus-hooks
cards-sdk" ]
}

@test "resolve_units_explicit: whitespace-tolerant in list" {
  result=$(resolve_units_explicit " chorus-hooks , cards-sdk ")
  [ "$result" = "chorus-hooks
cards-sdk" ]
}

@test "resolve_units_explicit: unknown unit returns REJECT:<name>" {
  result=$(resolve_units_explicit "bogus")
  [ "$result" = "REJECT:bogus" ]
}

@test "resolve_units_explicit: mixed known/unknown emits both" {
  result=$(resolve_units_explicit "chorus-hooks,bogus")
  echo "$result" | grep -q "^chorus-hooks$"
  echo "$result" | grep -q "^REJECT:bogus$"
}

@test "resolve_units_explicit: empty string yields no output" {
  result=$(resolve_units_explicit "")
  [ -z "$result" ]
}

@test "resolve_units dispatches to explicit when --units given" {
  result=$(resolve_units "chorus-api" "/nonexistent-werk")
  [ "$result" = "chorus-api" ]
}
