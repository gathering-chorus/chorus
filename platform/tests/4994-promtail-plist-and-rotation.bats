#!/usr/bin/env bats
# @test-type: integration — runs land-4994-rows against a fake launchd, a fake :9080 and scratch repos; runs the rotate script on temp dirs
# @domain: logs
# @card: #4994 · owner: borg-queen
# AC4: com.gathering.promtail runs this repo's config from launchagents-canonical (the proving
# copy is retired), the GB-scale daemon logs rotate by copy-and-truncate in the one rotation
# path (gathering-log-rotate.sh, #4718) and never the spine, and land-4994-rows moves Promtail
# in the order that keeps every offset (bootout, copy, bootstrap), restoring on a red reading.

setup() {
  ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
  PL="$ROOT/platform/scripts/launchagents-canonical/com.gathering.promtail.plist"
  ROT="$ROOT/platform/scripts/gathering-log-rotate.sh"
  LAND="$ROOT/platform/scripts/land-4994-rows"
  HARNESS="$ROOT/platform/tests/fixtures/4994/land-harness.py"
  T="$BATS_TEST_TMPDIR"
}

# why a promtail plist is wrong; empty when it runs this repo's config and logs where Gathering logs live
plist_wrong() {
  local cfg out
  cfg="$(plutil -extract ProgramArguments.1 raw "$1")"; out="$(plutil -extract StandardOutPath raw "$1")"
  [[ "$cfg" == "-config.file="*"/chorus/platform/config/promtail/promtail-native.yaml" ]] || echo "config: $cfg"
  [[ "$out" == *"/Library/Logs/Gathering/promtail.log" ]] || echo "stdout: $out"
  # bounded polling and syncing (AC4): promtail 2.9's default polls every file every 250 ms
  local args; args="$(plutil -extract ProgramArguments json -o - "$1")"
  for fl in -file-watch.min_poll_frequency=1s -file-watch.max_poll_frequency=5s -positions.sync-period=30s -target.sync-period=30s; do
    [[ "$args" == *"\"$fl\""* ]] || echo "missing $fl"
  done
}

@test "AC4: the canonical promtail plist runs this repo's config and logs to Logs/Gathering; the proving copy is gone" {
  w="$(plist_wrong "$PL")"; [ -z "$w" ] || { echo "$w"; return 1; }
  [ ! -e "$ROOT/proving/config/launchagents/com.gathering.promtail.plist" ]
}

@test "AC4: promtail accepts the plist's own flags with the repo config; an unknown flag is refused (exit 2)" {
  bin="$(plutil -extract ProgramArguments.0 raw "$PL")"
  [ -x "$bin" ] || skip "UNMEASURED: no promtail binary at $bin"
  args=(); i=1; while a="$(plutil -extract ProgramArguments.$i raw "$PL" 2>/dev/null)"; do
    case "$a" in -config.file=*) ;; *) args+=("$a");; esac; i=$((i + 1)); done
  run "$bin" "${args[@]}" -check-syntax -config.file "$ROOT/platform/config/promtail/promtail-native.yaml"
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
  run "$bin" -no.such.flag=1 -check-syntax -config.file "$ROOT/platform/config/promtail/promtail-native.yaml"
  [ "$status" -eq 2 ]
}

@test "AC4 NEGATIVE PROOF: a plist on the old shared-observability config, /tmp log and unbounded polling is named" {
  sed -e 's#/chorus/platform/config/promtail/promtail-native.yaml#/shared-observability/config/promtail/promtail-native.yaml#' \
      -e 's#[^<>]*/Library/Logs/Gathering/promtail.log#/tmp/promtail.log#g' "$PL" > "$T/old.plist"
  plutil -remove ProgramArguments.2 "$T/old.plist"
  w="$(plist_wrong "$T/old.plist")"
  [[ "$w" == *"config:"* && "$w" == *"stdout: /tmp/promtail.log"* && "$w" == *"missing -file-watch.min_poll_frequency=1s"* ]] || { echo "$w"; return 1; }
}

@test "AC4: promtail, vikunja, node-exporter-host and chorus-api rotate by copy-and-truncate; a held writer's next line lands in the live file" {
  mkdir -p "$T/g" "$T/c"
  for f in g/promtail.log g/vikunja.log g/node-exporter-host.log c/chorus-api.log; do head -c 2000 /dev/zero | tr '\0' x > "$T/$f"; done
  exec 7>>"$T/g/promtail.log"
  run env LOG_ROTATE_DIR="$T/g" LOG_ROTATE_CHORUS_DIR="$T/c" LOG_ROTATE_SPINE="$T/none" LOG_ROTATE_BYTES=1000 bash "$ROT"
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
  for n in promtail vikunja node-exporter-host chorus-api; do [[ "$output" == *"$n.log rotated"* ]] || { echo "$output"; return 1; }; done
  echo "after" >&7; exec 7>&-
  [ "$(cat "$T/g/promtail.log")" = after ]
  [ "$(gzip -dc "$T"/c/chorus-api.log.*.gz | wc -c | tr -d ' ')" -eq 2000 ]
}

@test "AC4 NEGATIVE PROOF: the spine is refused and untouched; a test's LOG_ROTATE_DIR never reaches a live Chorus log" {
  mkdir -p "$T/g" "$T/home/Library/Logs/Chorus" "$T/home/.chorus"
  head -c 2000 /dev/zero | tr '\0' x > "$T/home/.chorus/chorus.log"
  head -c 2000 /dev/zero | tr '\0' x > "$T/home/Library/Logs/Chorus/chorus-api.log"
  run env HOME="$T/home" LOG_ROTATE_DIR="$T/g" LOG_ROTATE_EXTRA="$T/home/.chorus/chorus.log" LOG_ROTATE_BYTES=1000 bash "$ROT"
  [ "$status" -eq 1 ] || { echo "a spine touch exited $status"; return 1; }
  [[ "$output" == *"refused $T/home/.chorus/chorus.log: it is the spine"* ]] || { echo "$output"; return 1; }
  [ "$(wc -c < "$T/home/.chorus/chorus.log" | tr -d ' ')" -eq 2000 ]
  [ "$(wc -c < "$T/home/Library/Logs/Chorus/chorus-api.log" | tr -d ' ')" -eq 2000 ] || { echo "a live-path chorus-api.log was rotated"; return 1; }
}

@test "AC4: the land boots out promtail before copying positions, bootstraps the new plist, rotates, reloads log-archive, then retires the old configs" {
  printf 'promtail_files_active_total 152\npromtail_read_bytes_total{path="/x/Library/Logs/Chorus/chorus-api.log"} 9\n' > "$T/green.prom"
  run python3 "$HARNESS" "$LAND" "$T/run" "$T/green.prom" 3.1
  [[ "${lines[0]}" == "exit 0" ]] || { echo "$output"; return 1; }
  [[ "${lines[1]}" == "launchctl bootout com.gathering.promtail [new positions: none]" ]] || { echo "$output"; return 1; }
  [[ "$output" == *"launchctl bootstrap"*"[new positions: 2]"*"chorus-log land.promtail.spine-probe"*"log-rotate"*"bootout com.chorus.log-archive"* ]] || { echo "$output"; return 1; }
  [[ "$output" == *"installed: new com.gathering.promtail"* && "$output" == *"obs left: promtail-bedroom.yaml"* ]] || { echo "$output"; return 1; }
  [[ "$output" == *"old positions kept: True"* ]] || { echo "$output"; return 1; }
}

@test "AC4 NEGATIVE PROOF: today's reading (9,214 files) restores the old plist and keeps the old configs" {
  printf 'promtail_files_active_total 9214\n' > "$T/red.prom"
  run python3 "$HARNESS" "$LAND" "$T/run" "$T/red.prom" 54.7
  [[ "${lines[0]}" == "exit 1" ]] || { echo "$output"; return 1; }
  [[ "$output" == *"installed: old com.gathering.promtail"* && "$output" == *"obs left: promtail-bedroom.yaml,promtail-config.yaml,promtail-native.yaml"* ]] || { echo "$output"; return 1; }
  [[ "$output" == *"9214 files (want under 500)"* && "$output" != *"log-rotate"* ]] || { echo "$output"; return 1; }
}

@test "AC4 NEGATIVE PROOF: a spine event that never reaches Loki restores the old plist before any rotate or retire" {
  printf 'promtail_files_active_total 152\n' > "$T/green.prom"
  run python3 "$HARNESS" "$LAND" "$T/run" "$T/green.prom" 3.1 nospine
  [[ "${lines[0]}" == "exit 1" ]] || { echo "$output"; return 1; }
  [[ "$output" == *"did not reach Loki"* && "$output" == *"installed: old com.gathering.promtail"* && "$output" != *"log-rotate"* ]] || { echo "$output"; return 1; }
  [[ "$output" == *"obs left: promtail-bedroom.yaml,promtail-config.yaml,promtail-native.yaml"* ]] || { echo "$output"; return 1; }
}

@test "AC4: a rerun keeps the newer positions; the verdict refuses dagu series, CPU over the line and a missing count" {
  printf 'promtail_files_active_total 152\n' > "$T/green.prom"
  run python3 "$HARNESS" "$LAND" "$T/run" "$T/green.prom" 3.1 rerun
  [[ "$output" == *"new positions: positions:   a.log: '99'   b.log: '7'"* ]] || { echo "$output"; return 1; }
  run python3 - "$LAND" <<'PY'
import importlib.machinery, importlib.util, sys
l = importlib.machinery.SourceFileLoader("land", sys.argv[1]); s = importlib.util.spec_from_loader("land", l)
m = importlib.util.module_from_spec(s); l.exec_module(m)
files, dagu = m.reading('promtail_files_active_total 152\npromtail_read_bytes_total{path="/u/Library/Logs/Chorus/dagu/cicd/x/a.log"} 3\n')
print(m.verdict(files, dagu, [2.0]))
print(m.verdict(152, 0, [54.7, 50.0]))
print(m.verdict(None, 0, [1.0]))
print(m.verdict(152, 0, [2.0, 3.0]) or "green")
PY
  [[ "${lines[0]}" == "1 series on :9080 name a dagu step log (want 0)" ]] || { echo "$output"; return 1; }
  [[ "${lines[1]}" == "promtail averages 52.4 % CPU over 2 samples (want under 10 %)" ]] || { echo "$output"; return 1; }
  [[ "${lines[2]}" == "promtail_files_active_total is missing from :9080/metrics" && "${lines[3]}" == green ]] || { echo "$output"; return 1; }
}
