#!/usr/bin/env bats
# @test-type: integration — 401 expiration detector and alert delivery
# @domain: chorus
# @card: #4924
# #4924 AC2 — A role session that hits a 401 or expired login raises one alert
# to Jeff naming the role, within 60s.
# Negative proof: a fixture transcript with the 401 line reads red.

setup() {
  REPO="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
  T="$BATS_TEST_TMPDIR"
  mkdir -p "$T/logs"
  ALERT_SCANNER="$REPO/platform/scripts/role-401-scan"
}

@test "control: clean session logs produce no alert" {
  cat <<'EOF' > "$T/logs/kade-session.log"
2026-11-07 14:15:00 [info] tool call completed
2026-11-07 14:16:00 [info] response sent
EOF

  run "$ALERT_SCANNER" --scan-file "$T/logs/kade-session.log" --role kade
  [ "$status" -eq 0 ] || return 1
  [ -z "$output" ] || return 1
}

@test "negative proof: fixture transcript with 401 line triggers red / raises alert naming role" {
  cat <<'EOF' > "$T/logs/kade-401.log"
2026-11-07 14:20:00 [error] API Error: 401 OAuth access token has expired. Please run /login
EOF

  run "$ALERT_SCANNER" --scan-file "$T/logs/kade-401.log" --role kade
  [ "$status" -eq 1 ] || return 1
  echo "$output" | grep -q "ALERT: role kade Claude login expired (401)" || return 1
}

@test "401 detector dedupes alerts within 60s window" {
  cat <<'EOF' > "$T/logs/silas-401.log"
2026-11-07 14:22:00 [error] API Error: 401 OAuth access token has expired. Please run /login
2026-11-07 14:22:30 [error] API Error: 401 OAuth access token has expired. Please run /login
EOF

  STATE_FILE="$T/silas-alert.state"
  run "$ALERT_SCANNER" --scan-file "$T/logs/silas-401.log" --role silas --state-file "$STATE_FILE"
  [ "$status" -eq 1 ] || return 1
  # Second invocation within 60s should be deduped (exit 0, no duplicate alert)
  run "$ALERT_SCANNER" --scan-file "$T/logs/silas-401.log" --role silas --state-file "$STATE_FILE"
  [ "$status" -eq 0 ] || return 1
}
