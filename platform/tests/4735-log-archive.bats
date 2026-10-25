#!/usr/bin/env bats
# @test-type: integration — real files in a tempdir; never touches ~/Library/Logs or ~/.chorus
# @domain: observability
# @card: #4735
# @owner: silas
#
# #4735 — platform/scripts/log-archive. What Jeff sees: the Library is no longer
# pegged by promtail tailing 20,000 old pipeline logs, and not one log line is
# lost on the way ("the log data is highly valuable be careful", 10-25 08:06).

ROOT="$BATS_TEST_DIRNAME/../.."
LA="$ROOT/platform/scripts/log-archive"

old() { touch -t 202610010000 "$@"; }

setup() {
  T="$(mktemp -d)"
  export LOG_ARCHIVE_DAGU_LOGS="$T/logs" LOG_ARCHIVE_DAGU_DATA="$T/data" LOG_ARCHIVE_ROOT="$T/archive" LOG_ARCHIVE_CHORUS_LOG="$T/chorus.log"
  mkdir -p "$T/logs/cicd/dag-run_20261001_010101Z_OLDrun1/run_x" "$T/logs/cicd/dag-run_20261025_010101Z_NEWrun2/run_y" \
           "$T/logs/cicd/dag-run_20261001_020202Z_WAITrun3/run_z" "$T/logs/run_20261001_030303Z_abc123" \
           "$T/data/cicd/dag-runs/2026/10/01/dag-run_20261001_020202Z_WAITrun3"
  echo "old step" > "$T/logs/cicd/dag-run_20261001_010101Z_OLDrun1/run_x/a.out"
  echo "old err" > "$T/logs/cicd/dag-run_20261001_010101Z_OLDrun1/run_x/a.err"
  echo "new step" > "$T/logs/cicd/dag-run_20261025_010101Z_NEWrun2/run_y/b.out"
  echo "waiting" > "$T/logs/cicd/dag-run_20261001_020202Z_WAITrun3/run_z/c.out"
  echo "legacy" > "$T/logs/run_20261001_030303Z_abc123/d.log"
  old "$T/logs/cicd/dag-run_20261001_010101Z_OLDrun1/run_x/"* "$T/logs/cicd/dag-run_20261001_020202Z_WAITrun3/run_z/c.out" "$T/logs/run_20261001_030303Z_abc123/d.log"
  # the waiting run's status was written today: it is still live
  echo '{"status":"waiting"}' > "$T/data/cicd/dag-runs/2026/10/01/dag-run_20261001_020202Z_WAITrun3/status.jsonl"
}

teardown() { rm -rf "$T"; }

@test "a dry run counts what would move and moves nothing" {
  run "$LA" dagu
  [ "$status" -eq 0 ]
  [[ "$output" == *"2 run(s), 3 file(s)"* ]] || return 1
  [[ "$output" == *"dry run: nothing moved"* ]] || return 1
  [ -f "$T/logs/cicd/dag-run_20261001_010101Z_OLDrun1/run_x/a.out" ]
  [ ! -d "$T/archive" ]
}

@test "old runs move with every byte; today's run and a run still waiting stay" {
  run "$LA" dagu --apply
  [ "$status" -eq 0 ]
  [[ "$output" == *"moved and verified 3 file(s)"* ]] || return 1
  [ "$(cat "$T/archive/dagu/cicd/dag-run_20261001_010101Z_OLDrun1/run_x/a.out")" = "old step" ]
  [ -f "$T/archive/dagu/run_20261001_030303Z_abc123/d.log" ]
  [ ! -e "$T/logs/cicd/dag-run_20261001_010101Z_OLDrun1" ]
  [ -f "$T/logs/cicd/dag-run_20261025_010101Z_NEWrun2/run_y/b.out" ]
  [ -f "$T/logs/cicd/dag-run_20261001_020202Z_WAITrun3/run_z/c.out" ]
  ls "$T/archive/manifests/"dagu-*.json
}

@test "a second run finds nothing and changes nothing" {
  "$LA" dagu --apply
  run "$LA" dagu --apply
  [ "$status" -eq 0 ]
  [[ "$output" == *"0 run(s)"* ]] || return 1
}

# NEGATIVE PROOF: the verify step separates a true copy from a lost or changed
# file — a manifest checked against an archive missing one file fails loudly.
@test "negative proof: a file missing or changed in the archive fails the check" {
  "$LA" dagu --apply
  m=$(ls "$T/archive/manifests/"dagu-*.json)
  run python3 -I -c "
import importlib.util, importlib.machinery, json, sys
l = importlib.machinery.SourceFileLoader('la', '$LA'); s = importlib.util.spec_from_loader('la', l); la = importlib.util.module_from_spec(s); l.exec_module(la)
man = json.load(open('$m'))['files']
print(len(la.verify(man, '$T/archive/dagu')))
open('$T/archive/dagu/cicd/dag-run_20261001_010101Z_OLDrun1/run_x/a.out', 'w').write('tampered')
import os; os.remove('$T/archive/dagu/run_20261001_030303Z_abc123/d.log')
print(sorted(b.split()[0] for b in la.verify(man, '$T/archive/dagu')))
"
  [ "$status" -eq 0 ]
  [ "${lines[0]}" = "0" ]
  [ "${lines[1]}" = "['changed', 'missing']" ]
}

@test "chorus.log is copied, verified and truncated in place; its inode is kept" {
  printf 'line one\nline two\n' > "$T/chorus.log"
  ino=$(stat -f %i "$T/chorus.log")
  run "$LA" chorus-log --apply
  [ "$status" -eq 0 ]
  [[ "$output" == *"copied and verified 18 bytes"* ]] || return 1
  [ "$(stat -f %i "$T/chorus.log")" = "$ino" ]
  [ ! -s "$T/chorus.log" ]
  [ "$(gunzip -c "$T/archive/chorus-log/"chorus.log.*.gz)" = "$(printf 'line one\nline two')" ]
}

@test "chorus.log under the --over size is left as is" {
  printf 'small\n' > "$T/chorus.log"
  run "$LA" chorus-log --over 1000 --apply
  [ "$status" -eq 0 ]
  [[ "$output" == *"left as is"* ]] || return 1
  [ -s "$T/chorus.log" ]
  [ ! -d "$T/archive/chorus-log" ]
}

@test "the daily job is a canonical LaunchAgent that applies both and keeps a failure's exit" {
  P="$ROOT/platform/scripts/launchagents-canonical/com.chorus.log-archive.plist"
  plutil -lint "$P"
  run plutil -extract ProgramArguments.2 raw "$P"
  [[ "$output" == *"log-archive dagu --apply; r=\$?"* ]] || return 1
  [[ "$output" == *"exit \$r"* ]] || return 1
}
