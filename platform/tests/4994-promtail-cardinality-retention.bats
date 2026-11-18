#!/usr/bin/env bats
# @test-type: integration — runs platform/scripts/log-archive on fixture folders; never touches live logs
# @domain: logs
# @card: #4994 · owner: borg-queen
# AC3: dated ops/logs files older than the window leave promtail's reach (archived, each
# proven by size and sha256, never deleted); undated live files stay. Measured 2026-11-17:
# 158 of ops/logs' 175 files (3.08 GB) are dated and older than 2 days.
# Not asserted here: live :9080 (werk-test runs before the land; the land's verifier checks it).

setup() {
  ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
  LA="$ROOT/platform/scripts/log-archive"
  export LOG_ARCHIVE_OPS_LOGS="$BATS_TEST_TMPDIR/ops/logs" LOG_ARCHIVE_ROOT="$BATS_TEST_TMPDIR/archive"
  mkdir -p "$LOG_ARCHIVE_OPS_LOGS"
  old() { printf '%s\n' "$2" > "$LOG_ARCHIVE_OPS_LOGS/$1"; touch -t 202611010000 "$LOG_ARCHIVE_OPS_LOGS/$1"; }
  old athena-make-20261101.jsonl a; old owl-api-20260820.jsonl b
  printf 'today\n' > "$LOG_ARCHIVE_OPS_LOGS/athena-make-$(date +%Y%m%d).jsonl"
  old werk-build.jsonl live   # undated: a live log, however old its mtime
}

@test "AC3: ops-logs dry run names the old dated files and moves nothing" {
  run python3 "$LA" ops-logs
  [ "$status" -eq 0 ] && [[ "$output" == *"2 dated file(s)"* ]] || { echo "$output"; return 1; }
  [ "$(ls "$LOG_ARCHIVE_OPS_LOGS" | wc -l | tr -d ' ')" = 4 ]
}

@test "AC3: ops-logs --apply archives only old dated files, each proven, and keeps the rest" {
  run python3 "$LA" ops-logs --apply
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
  [ "$(ls "$LOG_ARCHIVE_OPS_LOGS" | sort | paste -sd, -)" = "athena-make-$(date +%Y%m%d).jsonl,werk-build.jsonl" ]
  [ "$(cat "$LOG_ARCHIVE_ROOT/ops-logs/athena-make-20261101.jsonl")" = a ]
  [ "$(cat "$LOG_ARCHIVE_ROOT/ops-logs/owl-api-20260820.jsonl")" = b ]
  ls "$LOG_ARCHIVE_ROOT"/manifests/ops-logs-*.json >/dev/null
}

@test "AC3 NEGATIVE PROOF: an archived file whose bytes changed fails the run, loudly" {
  mkdir -p "$LOG_ARCHIVE_ROOT/ops-logs"
  # the verifier reads the archive: plant a different file where the move will land, then
  # make the move write over it with a shutil.move stand-in that corrupts one byte
  run python3 - "$LA" <<'PY'
import importlib.util, importlib.machinery, os, shutil, sys
loader = importlib.machinery.SourceFileLoader("la", sys.argv[1])
spec = importlib.util.spec_from_loader("la", loader); la = importlib.util.module_from_spec(spec); loader.exec_module(la)
real = shutil.move
def bad_move(src, dst):
    real(src, dst); open(dst, "a").write("x")
la.shutil.move = bad_move
sys.exit(la.ops_logs(2, True))
PY
  [ "$status" -eq 1 ] && [[ "$output" == *"LOG-ARCHIVE FAILED"*"not proven"* ]] || { echo "$output"; return 1; }
}

# Jeff 17:25 (via Wren): dagu's run count must not drive promtail's load, so its step logs are
# never tailed; dagu's own process log still is.
promtail_home() { dirname "$(dirname "$(plutil -extract ProgramArguments.0 raw "$ROOT/platform/scripts/launchagents-canonical/com.gathering.promtail.plist")")"; }
ROUTES_4336() { ruby "$ROOT/platform/tests/fixtures/4336/promtail-routes.rb" "$@"; }

@test "AC3: no promtail target tails a dagu run or step log; dagu.log itself is still tailed" {
  h="$(promtail_home)"; cfg="$ROOT/platform/config/promtail/promtail-native.yaml"
  for f in "$h/Library/Logs/Chorus/dagu/cicd/dag-run_20261117_215954Z_x/step.log" "$h/Library/Logs/Chorus/dagu/run_20261114_080409Z_991413/a.out"; do
    out="$(ROUTES_4336 "$cfg" "$f")"
    [ -z "$out" ] || { echo "$f tailed by: $out"; return 1; }
  done
  [ -n "$(ROUTES_4336 "$cfg" "$h/Library/Logs/Chorus/dagu.log")" ] || { echo "dagu.log is no longer tailed"; return 1; }
}

@test "AC3 NEGATIVE PROOF: the old recursive dagu target is named as tailing a step log" {
  h="$(promtail_home)"
  printf 'scrape_configs:\n  - job_name: chorus-dagu\n    static_configs: [{ labels: { job: dagu, __path__: "%s/Library/Logs/Chorus/dagu/**/*.{log,out,err}" } }]\n' "$h" > "$BATS_TEST_TMPDIR/old.yaml"
  [ "$(ROUTES_4336 "$BATS_TEST_TMPDIR/old.yaml" "$h/Library/Logs/Chorus/dagu/cicd/dag-run_x/step.log")" = "dagu" ]
}

@test "AC3: ops-logs with no ops/logs folder says so and exits 0 (never a traceback)" {
  run env LOG_ARCHIVE_OPS_LOGS="$BATS_TEST_TMPDIR/none" python3 "$LA" ops-logs --apply
  [ "$status" -eq 0 ] && [[ "$output" == *"no folder"* ]] || { echo "$output"; return 1; }
}

@test "AC3: an ops-logs file already archived with the same bytes leaves no copy behind (the shared move path)" {
  mkdir -p "$LOG_ARCHIVE_ROOT/ops-logs"; printf 'a\n' > "$LOG_ARCHIVE_ROOT/ops-logs/athena-make-20261101.jsonl"
  run python3 "$LA" ops-logs --apply
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
  [ ! -e "$LOG_ARCHIVE_OPS_LOGS/athena-make-20261101.jsonl" ] && [ ! -e "$LOG_ARCHIVE_ROOT"/ops-logs/athena-make-20261101.jsonl.2* ] || { ls "$LOG_ARCHIVE_ROOT/ops-logs"; return 1; }
}
