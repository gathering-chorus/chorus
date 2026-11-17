#!/usr/bin/env bats
# @test-type: integration — runs scratch promtails in their own folders (~45 s); reads this repo's promtail config
# @domain: logs
# @card: #4994 · owner: borg-queen
# AC2: promtail never wakes its own folder watcher. Its positions file sits outside every
# folder it tails, and moves there with every offset kept; ~/.chorus is tailed by named file,
# so a new .log there is never shipped; and log_level is warn, so the watcher events promtail
# 2.9.2 still raises on ~/.chorus lock/tmp churn (it watches each tailed file's folder,
# measured 2026-11-17) stay out of promtail.log, which had grown to 1.72 GB.

setup() {
  ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
  CFG="$ROOT/platform/config/promtail/promtail-native.yaml"
  ROUTES="$ROOT/platform/tests/fixtures/4336/promtail-routes.rb"
  WATCH="$ROOT/platform/tests/fixtures/4994/watch-events.py"
  OFFSETS="$ROOT/platform/tests/fixtures/4994/offsets-kept.py"
  PROMTAIL="${PROMTAIL_BIN:-$(plutil -extract ProgramArguments.0 raw "$ROOT/platform/scripts/launchagents-canonical/com.gathering.promtail.plist" 2>/dev/null)}"
  CHORUS_HOME_DIR="$(dirname "$(dirname "$PROMTAIL")")/.chorus"
}

# the __path__ of each target whose folder holds the positions file: the same folder, or any
# folder under a recursive (/**) target (Borg Queen 17:16: File.dirname of ".../dagu/**/*.log"
# is ".../dagu/**", which an exact match never meets). Empty = positions is tailed by nothing.
positions_tailed_by() {
  ruby -ryaml -e '
    c = YAML.load_file(ARGV[0]); pd = File.dirname(c["positions"]["filename"])
    c["scrape_configs"].each { |s| (s["static_configs"] || []).each { |t|
      p = (t["labels"] || {})["__path__"] or next
      hit = p.include?("/**") ? (pd + "/").start_with?(p.split("/**").first + "/") : File.dirname(p) == pd
      puts p if hit } }' "$1"
}

@test "AC2: the positions file is in no folder promtail tails" {
  t="$(positions_tailed_by "$CFG")"
  [ -z "$t" ] || { echo "positions folder is tailed by: $t"; return 1; }
}

@test "AC2: promtail logs at warn, so watcher events stay out of promtail.log" {
  run ruby -ryaml -e 'puts YAML.load_file(ARGV[0])["server"]["log_level"]' "$CFG"
  [ "$output" = "warn" ] || { echo "log_level is '$output'"; return 1; }
}

@test "AC2: a new .log in ~/.chorus is not tailed (named files only)" {
  run ruby "$ROUTES" "$CFG" "$CHORUS_HOME_DIR/rogue.log"
  [ -z "$output" ] || { echo "rogue.log would be tailed by: $output"; return 1; }
}

@test "AC2: positions writes in a subfolder raise no watcher event (scratch promtail, log level info)" {
  [ -x "$PROMTAIL" ] || skip "UNMEASURED: no promtail binary at $PROMTAIL"
  run python3 "$WATCH" "$PROMTAIL" "$BATS_TEST_TMPDIR/sub" subdir
  [ "$output" = "0" ] || { echo "positions events: $output"; return 1; }
}

@test "AC2 NEGATIVE PROOF: positions in the tailed folder raises watcher events; a wildcard tails rogue.log; a tailed positions folder is named, also under /**" {
  [ -x "$PROMTAIL" ] || skip "UNMEASURED: no promtail binary at $PROMTAIL"
  run python3 "$WATCH" "$PROMTAIL" "$BATS_TEST_TMPDIR/watched" watched
  [ "$output" -gt 0 ] || { echo "no events with positions in the tailed folder: $output"; return 1; }
  printf 'scrape_configs:\n  - job_name: g\n    static_configs: [{ labels: { job: g, __path__: %s/*.log } }]\npositions: { filename: %s/positions.yaml }\n' "$CHORUS_HOME_DIR" "$CHORUS_HOME_DIR" > "$BATS_TEST_TMPDIR/w.yaml"
  [ "$(ruby "$ROUTES" "$BATS_TEST_TMPDIR/w.yaml" "$CHORUS_HOME_DIR/rogue.log")" = "g" ]
  [ -n "$(positions_tailed_by "$BATS_TEST_TMPDIR/w.yaml")" ]
  printf 'scrape_configs:\n  - job_name: r\n    static_configs: [{ labels: { job: r, __path__: /x/dagu/**/*.log } }]\npositions: { filename: /x/dagu/run/a/positions.yaml }\n' > "$BATS_TEST_TMPDIR/r.yaml"
  [ "$(positions_tailed_by "$BATS_TEST_TMPDIR/r.yaml")" = "/x/dagu/**/*.log" ]
}

@test "AC2: moving the positions file keeps every offset: only new lines ship (scratch promtail)" {
  [ -x "$PROMTAIL" ] || skip "UNMEASURED: no promtail binary at $PROMTAIL"
  run python3 "$OFFSETS" "$PROMTAIL" "$BATS_TEST_TMPDIR/copy" copy
  [ "$output" = "2" ] || { echo "lines shipped after the move: $output (want the 2 new)"; return 1; }
}

@test "AC2 NEGATIVE PROOF: a move without copying the positions re-ships the history" {
  [ -x "$PROMTAIL" ] || skip "UNMEASURED: no promtail binary at $PROMTAIL"
  run python3 "$OFFSETS" "$PROMTAIL" "$BATS_TEST_TMPDIR/nocopy" nocopy
  [ "$output" = "7" ] || { echo "lines shipped with no copy: $output (want all 7)"; return 1; }
}
