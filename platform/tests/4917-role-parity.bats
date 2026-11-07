#!/usr/bin/env bats
# @test-type: unit
# @domain: chorus
# @card: #4917 · owner: borg-queen

ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"

@test "#4917 AC1: role parity inventory exists and audits key account capabilities" {
  local inv="$ROOT/platform/config/role-parity-matrix.json"
  [ -f "$inv" ]

  run python3 -c "
import json
data = json.load(open('$inv'))
assert 'capabilities' in data
for role in ['borg-queen', 'abby-normal', 'kade']:
    assert role in data['roles']
print('OK')
"
  [ "$status" -eq 0 ]
  [ "$output" = "OK" ]
}
