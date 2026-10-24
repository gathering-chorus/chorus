#!/usr/bin/env bats
# @test-type: unit
# @domain: services
# #4719 — subtractive sweep: prune dead log monitors & retire ghost plists

setup() {
  SCRIPT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)/platform/scripts/deep-health.sh"
  [ -f "$SCRIPT" ] || skip "deep-health.sh not found"
  W="$BATS_TEST_TMPDIR"
  mkdir -p "$W/logs" "$W/agents" "$W/bin" "$W/Library/LaunchAgents"
  printf '#!/bin/sh\nexit 0\n' > "$W/bin/noop"; chmod +x "$W/bin/noop"
  export HEALTH_LOG_DIR="$W/logs"
  export HEALTH_PLIST_DIR="$W/agents"
  export HEALTH_AGENT_LIST="$W/loaded.txt"
  export HEALTH_JSON_OUT="$W/health.json"
  export HEALTH_STATE_FILE="$W/state.txt"
  export HEALTH_OPS_NUDGE="$W/bin/noop"
  export HEALTH_CHORUS_LOG="$W/bin/noop"
  : > "$W/endpoints.txt"; export HEALTH_ENDPOINTS_FILE="$W/endpoints.txt"
  export HEALTH_LOG_FILE="$W/logs/deep-health.log"
  : > "$W/loaded.txt"
}

write_plist() {
  local label="$1" log="$2"
  cat <<EOF > "$W/agents/$label.plist"
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key><string>$label</string>
  <key>StandardOutPath</key><string>$log</string>
</dict>
</plist>
EOF
}

@test "4719: pruned log monitors in SKIP_LOGS do not fail" {
  write_plist "com.chorus.roles-up" "$W/logs/roles-up.log"
  touch -t 202601010000 "$W/logs/roles-up.log"
  echo "1234 0 com.chorus.roles-up" > "$W/loaded.txt"

  run bash "$SCRIPT"
  grep -q "roles-up.log is" "$W/health.json" && return 1
  return 0
}

@test "4719: retired worktree plists in RETIRED_PLISTS do not fail" {
  touch "$W/Library/LaunchAgents/com.chorus.api.werk.abby-normal.plist"
  : > "$W/loaded.txt"

  HOME="$W" run bash "$SCRIPT"
  grep -q "com.chorus.api.werk.abby-normal: plist on disk but NOT loaded" "$W/health.json" && return 1
  return 0
}

@test "4719: negative proof - un-skipped agent with stale log fails loudly" {
  write_plist "com.chorus.real-agent" "$W/logs/real-agent.log"
  echo "sample log line" > "$W/logs/real-agent.log"
  touch -t 202601010000 "$W/logs/real-agent.log"
  echo "5678 0 com.chorus.real-agent" > "$W/loaded.txt"

  run bash "$SCRIPT"
  grep -q "log-freshness: real-agent.log is" "$W/health.json" || return 1
}

@test "4719: running agent with prior non-zero exit code does not alert as crashed" {
  write_plist "com.chorus.active-svc" "$W/logs/active.log"
  touch "$W/logs/active.log"
  echo "9999 143 com.chorus.active-svc" > "$W/loaded.txt"

  run bash "$SCRIPT"
  grep -q "crashed: com.chorus.active-svc" "$W/health.json" && return 1
  return 0
}

@test "4719: stopped agent with non-zero exit code alerts as crashed" {
  write_plist "com.chorus.dead-svc" "$W/logs/dead.log"
  touch "$W/logs/dead.log"
  echo "- 1 com.chorus.dead-svc" > "$W/loaded.txt"

  run bash "$SCRIPT"
  grep -q "crashed: com.chorus.dead-svc (exit 1)" "$W/health.json" || return 1
}
