#!/usr/bin/env bats
# @test-type: integration — 401 expiration detector and alert delivery
# @domain: alerts
# @card: #4924
# #4924 AC2 — A role session that hits a 401 or expired login raises one alert
# to Jeff naming the role, within 60s.
# Negative proof: a fixture transcript with the 401 line reads red.

setup() {
  REPO="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
  T="$BATS_TEST_TMPDIR"
  export HOME="$T"
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

@test "exact-line matching does not falsely suppress alert when KEY is prefix of stored entry" {
  cat <<'EOF' > "$T/logs/prefix-test.log"
2026-11-07 14:30:00 [error] API Error: 401 OAuth access token has expired. Please run /login
EOF

  STATE_FILE="$T/prefix-test.state"
  mkdir -p "$T"
  # Simulate an existing stored key that has this key as a substring prefix
  LINE_HASH=$(echo -n "2026-11-07 14:30:00 [error] API Error: 401 OAuth access token has expired. Please run /login" | cksum | awk '{print $1}')
  echo "kade:${LINE_HASH}999_longer_suffix" > "${STATE_FILE}.seen"

  run "$ALERT_SCANNER" --scan-file "$T/logs/prefix-test.log" --role kade --state-file "$STATE_FILE"
  [ "$status" -eq 1 ] || return 1
  echo "$output" | grep -q "ALERT: role kade Claude login expired (401)" || return 1
}

@test "rescanning the same pane across different temp files stays quiet" {
  TMP1="$T/pane1.log"
  TMP2="$T/pane2.log"
  cat <<'EOF' > "$TMP1"
2026-11-07 14:35:00 [error] API Error: 401 OAuth access token has expired. Please run /login
EOF
  cp "$TMP1" "$TMP2"

  STATE_FILE="$T/pane-test.state"
  # First scan with TMP1 alerts
  run "$ALERT_SCANNER" --scan-file "$TMP1" --role silas --state-file "$STATE_FILE"
  [ "$status" -eq 1 ] || return 1

  # Second scan with different file path TMP2 (e.g. fresh mktemp) for the same role stays quiet
  run "$ALERT_SCANNER" --scan-file "$TMP2" --role silas --state-file "$STATE_FILE"
  [ "$status" -eq 0 ] || return 1
}
