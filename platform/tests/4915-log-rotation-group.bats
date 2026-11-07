#!/usr/bin/env bats
# @test-type: unit — log-archive chorus-log --apply in a temp dir; no live log touched
# @domain: logs
# @card: #4915 · owner: kade
# The 04:30 rotation on 11-06 left the next write to create chorus.log, which made it
# jeffbridwell:staff 0644, and every role account was refused the spine from then on
# (the #4915 land's borg-record). log-archive now creates it group chorus, 0664.
ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"

rotate() {  # <script> → rotates a planted 2-line chorus.log in the test dir
  D="$BATS_TEST_TMPDIR/r"; rm -rf "$D"; mkdir -p "$D/archive"
  printf 'one\ntwo\n' > "$D/chorus.log"
  run env LOG_ARCHIVE_CHORUS_LOG="$D/chorus.log" LOG_ARCHIVE_ROOT="$D/archive" LOG_ARCHIVE_SETTLE_SECS=0 \
    python3 "$1" chorus-log --apply --over 0
}
mode_group() { stat -f '%Lp %Sg' "$1"; }

@test "after a rotation the fresh spine log is 664 and group chorus" {
  rotate "$ROOT/platform/scripts/log-archive"
  echo "$output"
  [ "$status" -eq 0 ] || return 1
  [ -f "$D/chorus.log" ] || { echo "no fresh chorus.log"; return 1; }
  [ "$(mode_group "$D/chorus.log")" = "664 chorus" ] || { mode_group "$D/chorus.log"; return 1; }
}

@test "negative proof: without the fresh-log lines the rotation leaves no group-writable log (the 11-06 state)" {
  sed '/^    fresh_log(CHORUS_LOG)$/d' "$ROOT/platform/scripts/log-archive" > "$BATS_TEST_TMPDIR/old-log-archive"
  grep -q 'fresh_log(CHORUS_LOG)$' "$BATS_TEST_TMPDIR/old-log-archive" && { echo "the call was not removed"; return 1; }
  rotate "$BATS_TEST_TMPDIR/old-log-archive"
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
  [ ! -f "$D/chorus.log" ] || [ "$(mode_group "$D/chorus.log")" != "664 chorus" ] || return 1
}

@test "a writer that won the race: an existing fresh log is set to 664 group chorus too" {
  D="$BATS_TEST_TMPDIR/race"; mkdir -p "$D"; ( umask 022; : > "$D/chorus.log" )
  run python3 -c 'import importlib.machinery,sys; m=importlib.machinery.SourceFileLoader("la", sys.argv[1]).load_module(); m.fresh_log(sys.argv[2])' "$ROOT/platform/scripts/log-archive" "$D/chorus.log"
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
  [ "$(mode_group "$D/chorus.log")" = "664 chorus" ] || { mode_group "$D/chorus.log"; return 1; }
}

@test "a fresh log another account owns (chown refused) leaves it as is, and the rotation still archives" {
  D="$BATS_TEST_TMPDIR/foreign"; rm -rf "$D"; mkdir -p "$D/archive"; printf 'one\ntwo\n' > "$D/chorus.log"
  run env LOG_ARCHIVE_CHORUS_LOG="$D/chorus.log" LOG_ARCHIVE_ROOT="$D/archive" LOG_ARCHIVE_SETTLE_SECS=0 python3 -c '
import importlib.machinery, os, sys
m = importlib.machinery.SourceFileLoader("la", sys.argv[1]).load_module()
real_rename = os.rename
def rename_then_foreign_writer(a, b):
    real_rename(a, b)
    open(a, "w").close()          # a writer won the race
def refused(*_a, **_k):
    raise PermissionError(1, "Operation not permitted")
m.os.rename, m.os.chown = rename_then_foreign_writer, refused
sys.exit(m.chorus_log(True, 0))' "$ROOT/platform/scripts/log-archive"
  echo "$output"
  [ "$status" -eq 0 ] || return 1
  [[ "$output" == *"owned by another account; left as is"* ]] || return 1
  ls "$D/archive/chorus-log/"chorus.log.*.gz >/dev/null 2>&1 || { echo "nothing archived"; return 1; }
}
