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
