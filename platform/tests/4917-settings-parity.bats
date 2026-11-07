#!/usr/bin/env bats
# @test-type: unit
# @domain: chorus
# @card: #4917 · owner: borg-queen

ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"

@test "#4917 AC3: borg-queen has a tracked settings file matching abby-normal's shape" {
  local bq_settings="${ROOT}/roles/borg-queen/.gemini/settings.json"
  local abby_settings="${ROOT}/roles/abby-normal/.gemini/settings.json"

  [ -f "$bq_settings" ]
  [ -f "$abby_settings" ]

  run python3 -c "
import json
bq = json.load(open('$bq_settings'))
abby = json.load(open('$abby_settings'))
assert set(bq.keys()) == set(abby.keys()), 'keys mismatch'
assert bq['mcpServers']['chorus-api']['env']['CHORUS_ROLE'] == 'borg-queen'
print('OK')
"
  [ "$status" -eq 0 ]
  [ "$output" = "OK" ]
}
