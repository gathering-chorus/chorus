#!/usr/bin/env bats
# @test-type: unit — a throwaway git origin + fake scope and MCP doors, no live service
# @domain: pipelines — a werk v2 land deploys the model its commit carried (#4494)
# What Jeff sees: a card that lands a model change has that model in the store
# afterwards (Wren, 2026-10-10 22:48: #4496's PolicyShape never arrived), and a
# card with no model change never starts a model run.

setup() {
  ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
  AL="$ROOT/platform/scripts/athena-land"
  T="$BATS_TEST_TMPDIR"
  git init -q --bare "$T/origin.git"
  git clone -q "$T/origin.git" "$T/home" 2>/dev/null
  git -C "$T/home" -c user.email=t@t -c user.name=t commit -q --allow-empty -m "base"
  git -C "$T/home" -c user.email=t@t -c user.name=t commit -q --allow-empty -m "#77 (kade) (#9)"
  git -C "$T/home" -c user.email=t@t -c user.name=t commit -q --allow-empty -m "#770 (kade) (#10)"
  git -C "$T/home" push -q origin HEAD:main
  LANDED="$(git -C "$T/home" rev-parse HEAD~1)"
  # the fake MCP door records what it was asked
  printf '#!/usr/bin/env bash\necho "$@" >> "%s/calls"\n' "$T" > "$T/mcp-call"; chmod +x "$T/mcp-call"
  export CHORUS_HOME="$T/home" ATHENA_LAND_MCP_CALL="$T/mcp-call"
}

scope() { printf '#!/usr/bin/env bash\necho "$@" >> "%s/scoped"\n%s\n' "$T" "$1" > "$T/scope"; chmod +x "$T/scope"; export ATHENA_LAND_SCOPE_BIN="$T/scope"; }

@test "a land that carried model sources runs chorus_athena canonical on its landed commit" {
  scope 'echo designing/model/policy.ttl'
  run "$AL" 77 kade
  [ "$status" -eq 0 ]
  [[ "$output" == *"1 model file(s)"* ]] || false
  grep -q "scope $T/home ${LANDED}^..${LANDED}" "$T/scoped"
  grep -q "kade chorus_athena {\"role\":\"kade\",\"card_id\":77,\"target\":\"canonical\",\"landed_commit\":\"${LANDED}\"}" "$T/calls"
}

@test "NEGATIVE PROOF: a land with no model source never starts a model run" {
  scope 'true'
  run "$AL" 77 kade
  [ "$status" -eq 0 ]
  [[ "$output" == *"carries no model source"* ]] || false
  [ ! -e "$T/calls" ]
}

@test "card 77 is not card 770: the landed commit is the card's own" {
  scope 'echo designing/model/x.ttl'
  run "$AL" 77 kade
  run grep -q "$(git -C "$T/home" rev-parse HEAD)" "$T/calls"
  [ "$status" -ne 0 ]
}

@test "a land subject in the summary form is found too" {
  git -C "$T/home" -c user.email=t@t -c user.name=t commit -q --allow-empty -m "kade: #79 — a summary (#11)"
  git -C "$T/home" push -q origin HEAD:main
  scope 'echo designing/model/x.ttl'
  run "$AL" 79 kade
  grep -q "\"card_id\":79,.*\"landed_commit\":\"$(git -C "$T/home" rev-parse HEAD)\"" "$T/calls"
}

@test "no landed commit says so and starts nothing" {
  scope 'echo designing/model/x.ttl'
  run "$AL" 78 kade
  [ "$status" -eq 0 ]
  [[ "$output" == *"no landed commit for #78"* ]] || false
  [ ! -e "$T/calls" ]
}

@test "a failed scope never reads as no model, and never fails the land" {
  scope 'echo boom >&2; exit 3'
  run "$AL" 77 kade
  [ "$status" -eq 0 ]
  [[ "$output" == *"scope failed"* ]] || false
  [[ "$output" != *"no model source"* ]] || false
  [ ! -e "$T/calls" ]
}
