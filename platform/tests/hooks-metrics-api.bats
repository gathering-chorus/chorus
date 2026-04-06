#!/usr/bin/env bats
# hooks-metrics-api.bats — Tests for /api/chorus/hooks/metrics endpoint (#2277)
# What Jeff sees: gate enforcement data available as a queryable API, not awk on a log file.

API="http://localhost:3340/api/chorus/hooks/metrics"

@test "hooks metrics endpoint exists and returns 200" {
  run curl -sf --max-time 5 -o /dev/null -w "%{http_code}" "$API"
  [ "$output" = "200" ]
}

@test "response is valid JSON" {
  result=$(curl -sf --max-time 5 "$API")
  echo "$result" | python3 -c "import json,sys; json.load(sys.stdin)" 2>/dev/null
}

@test "response includes total decisions count" {
  result=$(curl -sf --max-time 5 "$API")
  echo "$result" | python3 -c "import json,sys; d=json.load(sys.stdin); assert 'totalDecisions' in d, 'missing totalDecisions'"
}

@test "response includes per-module breakdown" {
  result=$(curl -sf --max-time 5 "$API")
  echo "$result" | python3 -c "import json,sys; d=json.load(sys.stdin); assert 'modules' in d, 'missing modules'"
}

@test "response includes enforcement percentage" {
  result=$(curl -sf --max-time 5 "$API")
  echo "$result" | python3 -c "import json,sys; d=json.load(sys.stdin); assert 'enforcementPercent' in d, 'missing enforcementPercent'"
}

@test "response includes enforced module count" {
  result=$(curl -sf --max-time 5 "$API")
  echo "$result" | python3 -c "import json,sys; d=json.load(sys.stdin); assert 'enforcedModules' in d, 'missing enforcedModules'"
}

@test "endpoint responds in under 500ms" {
  time_ms=$(curl -sf --max-time 5 -o /dev/null -w "%{time_total}" "$API" | awk '{printf "%.0f", $1 * 1000}')
  [ "$time_ms" -lt 500 ]
}

@test "module entries have deny and allow counts" {
  result=$(curl -sf --max-time 5 "$API")
  echo "$result" | python3 -c "
import json,sys
d = json.load(sys.stdin)
modules = d['modules']
assert len(modules) > 0, 'no modules'
first = list(modules.values())[0]
assert 'deny' in first, 'missing deny count'
assert 'allow' in first, 'missing allow count'
"
}
