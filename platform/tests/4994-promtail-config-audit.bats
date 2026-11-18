#!/usr/bin/env bats
# @test-type: integration — reads this repo's promtail config and the log files on disk it would tail
# @domain: logs
# @card: #4994 · owner: borg-queen
# AC1: platform/config/promtail/promtail-native.yaml tails each live log file once and has
# no target whose __path__ matches nothing; promtail itself accepts the config.
# Measured 2026-11-17 17:05 on the shared-observability copy: 44 files tailed twice,
# 13 dead targets (12 jobs).

setup() {
  ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
  CFG="$ROOT/platform/config/promtail/promtail-native.yaml"
  AUDIT="$ROOT/platform/tests/fixtures/4994/promtail-audit.rb"
  # the binary promtail runs from, read from its LaunchAgent in this repo
  PROMTAIL="${PROMTAIL_BIN:-$(plutil -extract ProgramArguments.0 raw "$(cd "$BATS_TEST_DIRNAME/../.." && pwd)/platform/scripts/launchagents-canonical/com.gathering.promtail.plist" 2>/dev/null)}"
}

@test "AC1: no live log file is tailed by two targets" {
  run ruby "$AUDIT" "$CFG"
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
  d="$(printf '%s\n' "$output" | grep '^DOUBLE' || true)"
  [ -z "$d" ] || { echo "$d"; return 1; }
}

@test "AC1: no target tails a path that matches no file" {
  run ruby "$AUDIT" "$CFG"
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
  d="$(printf '%s\n' "$output" | grep '^DEAD' || true)"
  [ -z "$d" ] || { echo "$d"; return 1; }
}

@test "AC1: promtail accepts the config (-check-syntax)" {
  [ -x "$PROMTAIL" ] || skip "UNMEASURED: no promtail binary at $PROMTAIL"
  run "$PROMTAIL" -check-syntax -config.file "$CFG"
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
}

@test "AC1 NEGATIVE PROOF: a file two targets tail, and a target matching nothing, are each named" {
  t="$BATS_TEST_TMPDIR/tree"; mkdir -p "$t/logs"; : > "$t/logs/a.log"; : > "$t/logs/b.log"
  cat > "$BATS_TEST_TMPDIR/p.yaml" <<YAML
scrape_configs:
  - job_name: glob
    static_configs: [{ labels: { job: glob, __path__: /logs/*.log } }]
  - job_name: one
    static_configs: [{ labels: { job: one, __path__: /logs/a.log } }]
  - job_name: gone
    static_configs: [{ labels: { job: gone, __path__: /logs/missing.log } }]
YAML
  run env ROOT_PREFIX="$t" ruby "$AUDIT" "$BATS_TEST_TMPDIR/p.yaml"
  [[ "$output" == *"DOUBLE /logs/a.log glob,one"* ]] || { echo "$output"; return 1; }
  [[ "$output" == *"DEAD gone /logs/missing.log"* ]] || { echo "$output"; return 1; }
  [[ "$output" != *"DOUBLE /logs/b.log"* ]] || { echo "$output"; return 1; }
  # the fix shape: an exclude on the glob clears the double
  sed -i '' 's#__path__: /logs/\*.log }#__path__: /logs/*.log, __path_exclude__: /logs/a.log }#' "$BATS_TEST_TMPDIR/p.yaml"
  run env ROOT_PREFIX="$t" ruby "$AUDIT" "$BATS_TEST_TMPDIR/p.yaml"
  [[ "$output" != *"DOUBLE"* ]] || { echo "$output"; return 1; }
}
