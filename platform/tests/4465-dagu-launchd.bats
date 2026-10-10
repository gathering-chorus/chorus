#!/usr/bin/env bats
# @test-type: fitness — static config check, no live service
# @domain: deploys — dagu, the orchestration layer, runs under launchd (#4465)
# What Jeff sees: the pipelines keep running after a reboot, and nothing off this
# machine can drive them while dagu has no login of its own.

setup() {
  ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
  PLIST="$ROOT/platform/launchd/com.chorus.dagu.plist"
  CONF="$ROOT/platform/config/dagu.yaml"
}

# host + auth.mode from a dagu config file (plain text: no YAML library on this box)
cfg() {
  local host mode
  host=$(awk '/^host:/{print $2}' "$1")
  mode=$(awk '/^auth:/{a=1;next} a&&/^[^ ]/{a=0} a&&/^ +mode:/{print $2}' "$1")
  echo "${host:-MISSING} ${mode:-MISSING}"
}

# loopback-only rule: auth none is allowed only on 127.0.0.1; a missing value fails
ok_bind() {
  set -- $(cfg "$1")
  [ "$1" != MISSING ] && [ "$2" != MISSING ] || return 1
  [ "$2" != "none" ] || [ "$1" = "127.0.0.1" ]
}

@test "the LaunchAgent starts dagu start-all with the repo config" {
  plutil -lint "$PLIST"
  [ "$(plutil -extract Label raw "$PLIST")" = "com.chorus.dagu" ]
  # #4474: run through service-run so starts, stops and failures reach Loki (#4446)
  [ "$(basename "$(plutil -extract ProgramArguments.0 raw "$PLIST")")" = "service-run" ]
  [ "$(plutil -extract ProgramArguments.2 raw "$PLIST")" = "daemon" ]
  [ "$(basename "$(plutil -extract ProgramArguments.3 raw "$PLIST")")" = "dagu" ]
  [ "$(plutil -extract ProgramArguments.4 raw "$PLIST")" = "start-all" ]
  [ "$(basename "$(plutil -extract ProgramArguments.6 raw "$PLIST")")" = "dagu.yaml" ]
}

@test "auth is set explicitly and dagu binds loopback only" {
  set -- $(cfg "$CONF")
  [ "$2" = "none" ] || [ "$2" = "builtin" ] || [ "$2" = "basic" ]
  ok_bind "$CONF"
}

@test "negative proof: auth none on a non-loopback host is refused" {
  bad="$BATS_TEST_TMPDIR/dagu.yaml"
  sed 's/^host: 127.0.0.1/host: 0.0.0.0/' "$CONF" > "$bad"
  run ok_bind "$bad"
  [ "$status" -ne 0 ]
  # and a config with no auth block at all is refused, not passed
  grep -v -e '^auth:' -e '^  mode:' "$CONF" > "$bad"
  run ok_bind "$bad"
  [ "$status" -ne 0 ]
}

# #4474: the generated workflow passes the machine's paths through from dagu's
# own env (dagu hands a step only a short list). Each one the workflow names
# must be set by the LaunchAgent, or every verb dies "<VAR> not set".
passthrough_unset() {
  local wf="$ROOT/platform/pipelines/cicd.yaml" missing="" v
  for v in $(sed -n '/^env:/,/^steps:/p' "$wf" | grep -oE '\$\{[A-Z_]+\}' | tr -d '${}' | sort -u); do
    [ "$v" = DAG_RUN_ID ] && continue  # dagu sets its own run id
    plutil -extract "EnvironmentVariables.$v" raw "$1" >/dev/null 2>&1 || missing="$missing $v"
  done
  echo "$missing"
}

@test "every variable the generated workflow passes through is set by the LaunchAgent" {
  run passthrough_unset "$PLIST"
  [ "$status" -eq 0 ]
  [ -z "$output" ] || { echo "unset in plist:$output"; return 1; }
}

@test "negative proof: a LaunchAgent without CHORUS_WERK_BASE is caught" {
  bad="$BATS_TEST_TMPDIR/dagu.plist"
  grep -v CHORUS_WERK_BASE "$PLIST" > "$bad"
  run passthrough_unset "$bad"
  [[ "$output" == *CHORUS_WERK_BASE* ]] || return 1
}

# #4474 / ADR-023: dagu's run and step logs must land where Promtail reads
# (~/Library/Logs/Chorus), and its own lines are JSON. Step output in
# ~/.chorus/dagu/logs was invisible to Loki.
logs_ok() {
  local dir fmt
  dir=$(awk '/^log_dir:/{print $2}' "$1")
  fmt=$(awk '/^log_format:/{print $2}' "$1")
  [[ "$dir" == */Library/Logs/Chorus/* ]] || { echo "log_dir ${dir:-MISSING} is outside ~/Library/Logs/Chorus"; return 1; }
  [ "$fmt" = json ] || { echo "log_format ${fmt:-MISSING} is not json"; return 1; }
}

@test "dagu logs to ~/Library/Logs/Chorus in JSON" {
  run logs_ok "$CONF"
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
}

@test "negative proof: a log_dir outside ~/Library/Logs/Chorus or no JSON is caught" {
  bad="$BATS_TEST_TMPDIR/dagu.yaml"
  sed 's|^log_dir: .*|log_dir: $HOME/.chorus/dagu/logs|' "$CONF" > "$bad"
  run logs_ok "$bad"
  [ "$status" -ne 0 ] || return 1
  grep -v '^log_format:' "$CONF" > "$bad"
  run logs_ok "$bad"
  [ "$status" -ne 0 ] || return 1
}
