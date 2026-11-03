#!/usr/bin/env bats
# @test-type: unit — hermetic: plists, logs and `launchctl list` are fixtures in $BATS_TEST_TMPDIR
# @domain: monitors
# @card: #4879 · owner: silas
# What Jeff sees: deep-health says what happened. On 2026-11-03 it called three
# batch jobs "crashed" all day; each ran to the end and reported findings and
# exited 1. A job that exited reads "exited", with the last line it wrote; only
# a job killed by a signal, or one that never started, reads "crashed".

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
  <key>StandardErrorPath</key><string>$log</string>
</dict>
</plist>
EOF
}

@test "a job that ran and exited 1 reads exited, with the last line it wrote, never crashed" {
  write_plist "com.chorus.harvest" "$W/logs/harvest.log"
  printf 'scanning\nservices: RED \xe2\x80\x94 24 finding(s)\n\n' > "$W/logs/harvest.log"
  echo "- 1 com.chorus.harvest" > "$W/loaded.txt"
  run bash "$SCRIPT"
  grep -q "crashed: com.chorus.harvest" "$W/health.json" && { cat "$W/health.json"; return 1; }
  grep -q "exited: com.chorus.harvest (exit 1) — services: RED — 24 finding(s)" "$W/health.json" || { cat "$W/health.json"; return 1; }
}

@test "negative proof: a job killed by a signal still reads crashed" {
  write_plist "com.chorus.killed" "$W/logs/killed.log"
  touch "$W/logs/killed.log"
  echo "- -9 com.chorus.killed" > "$W/loaded.txt"
  run bash "$SCRIPT"
  grep -q "crashed: com.chorus.killed (signal 9)" "$W/health.json" || { cat "$W/health.json"; return 1; }
}

@test "negative proof: a job that never started (exit 127, 126, 78) still reads crashed" {
  for ec in 127 126 78; do
    write_plist "com.chorus.nostart$ec" "$W/logs/nostart$ec.log"
    touch "$W/logs/nostart$ec.log"
    echo "- $ec com.chorus.nostart$ec" >> "$W/loaded.txt"
  done
  run bash "$SCRIPT"
  for ec in 127 126 78; do
    grep -q "crashed: com.chorus.nostart$ec (exit $ec)" "$W/health.json" || { cat "$W/health.json"; return 1; }
  done
}

@test "the alert does not re-fire when only the job's last log line changes (dedup on the failure, not the detail)" {
  write_plist "com.chorus.harvest" "$W/logs/harvest.log"
  echo "- 1 com.chorus.harvest" > "$W/loaded.txt"
  printf 'run 1 red: 24 finding(s) \\ "quoted"\n' > "$W/logs/harvest.log"
  run bash "$SCRIPT"
  python3 -c 'import json,sys; json.load(open(sys.argv[1]))' "$W/health.json" || { cat "$W/health.json"; return 1; }
  printf 'run 2 red: 24 finding(s)\n' > "$W/logs/harvest.log"
  run bash "$SCRIPT"
  [[ "$output" == *"failure set unchanged"* ]] || { echo "$output"; return 1; }
}
