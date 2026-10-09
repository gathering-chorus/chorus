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
  [ "$(plutil -extract ProgramArguments.1 raw "$PLIST")" = "start-all" ]
  [ "$(basename "$(plutil -extract ProgramArguments.3 raw "$PLIST")")" = "dagu.yaml" ]
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
