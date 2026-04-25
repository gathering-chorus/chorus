#!/usr/bin/env bats
# Shadow log enforcement — #2005 DEC-114

@test "deep-health checks for /tmp/*.log files" {
  grep -q 'shadow-logs\|/tmp/.*\.log' "$BATS_TEST_DIRNAME/../deep-health.sh"
}

@test "deep-health references DEC-114" {
  grep -q 'DEC-114' "$BATS_TEST_DIRNAME/../deep-health.sh"
}

@test "node-exporter plist does not write to /tmp/" {
  ! grep -q '/tmp/' ~/Library/LaunchAgents/com.prometheus.node-exporter.plist
}

@test "node-exporter writes to ~/Library/Logs/" {
  grep -q 'Library/Logs' ~/Library/LaunchAgents/com.prometheus.node-exporter.plist
}
