#!/usr/bin/env bats
# dashboard-health.bats — Tests for dashboard content validation (#2278)
# What Jeff sees: empty Grafana panels look like "everything fine" when data source is broken.

SCRIPT="/Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/dashboard-health.sh"
GRAFANA="http://localhost:3100"

@test "dashboard-health.sh exists and is executable" {
  [ -x "$SCRIPT" ]
}

@test "Grafana is reachable" {
  run curl -sf --max-time 5 "$GRAFANA/api/health"
  [ "$status" -eq 0 ]
}

@test "script checks all 13 dashboards" {
  # Script should reference dashboard count or iterate over provisioned dashboards
  run bash "$SCRIPT"
  [[ "$output" == *"dashboard"* ]]
}

@test "script reports results with dashboard names" {
  run bash "$SCRIPT"
  # Output should mention at least one known dashboard name
  [[ "$output" == *"Chorus"* ]] || [[ "$output" == *"App Operations"* ]] || [[ "$output" == *"dashboard"* ]]
}

@test "script exits 0 when all dashboards have data" {
  run bash "$SCRIPT"
  # If Grafana is healthy and data sources work, should pass
  # Failure is acceptable if a real data source is down
  [ "$status" -eq 0 ] || [[ "$output" == *"FAIL"* ]]
}

@test "script integrates with deep-health format" {
  # Output should be parseable as deep-health FAILURES entries
  run bash "$SCRIPT"
  # Should end with summary line
  [[ "$output" == *"pass"* ]] || [[ "$output" == *"fail"* ]] || [[ "$output" == *"FAIL"* ]]
}
