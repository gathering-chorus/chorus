#!/usr/bin/env bats
# @test-type: unit — rsync-backup.sh with a fake rsync and a fake ssh in a temp dir; nothing leaves the machine
# @domain: backups
# #4630 — a copy that fails twice must report FAIL, never OK. The old retry read
# `rc=$?` after an if with no else (always 0), so two failed rsyncs logged
# "RETRY (exit 0)" then "OK" and the spine copy on Bedroom froze at 10-13.

setup() {
  ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
  T="$(mktemp -d)"
  mkdir -p "$T/bin" "$T/home/.chorus" "$T/home/.claude" "$T/home/.ssh" "$T/home/CascadeProjects" "$T/home/Library/LaunchAgents"
  # fake Bedroom: answers the reachability probe, and reports each copy's size
  # as REMOTE_SIZE (default: as big as the source, i.e. a good copy)
  printf '#!/bin/sh\ncase "$*" in *stat*) echo "${REMOTE_SIZE:-999999999999}";; esac\nexit 0\n' > "$T/bin/ssh"
  chmod +x "$T/bin/ssh"
  head -c 5000 /dev/zero > "$T/home/.chorus/chorus.log"
}

teardown() { rm -rf "$T"; }

fake_rsync() { # fake_rsync <exit>
  printf '#!/bin/sh\necho "fake rsync $*" >&2\nexit %s\n' "$1" > "$T/bin/rsync"
  chmod +x "$T/bin/rsync"
}

run_backup() {
  run env HOME="$T/home" PATH="$T/bin:$PATH" BACKUP_RSYNC="$T/bin/rsync" BACKUP_SSH="$T/bin/ssh" REMOTE_SIZE="${REMOTE_SIZE:-}" CHORUS_ROOT="$T/none" \
    bash -c 'sleep() { :; }; export -f sleep; bash "$0"' "$ROOT/platform/scripts/rsync-backup.sh"
}

@test "a copy that fails twice reports FAIL with rsync's own exit code, and the run exits red" {
  fake_rsync 30
  run_backup
  [ "$status" -eq 1 ] || return 1
  [[ "$output" == *"RETRY: dot-chorus (rsync exit 30"* ]] || return 1
  [[ "$output" == *"FAIL: dot-chorus (rsync exit 30 after 2 attempts)"* ]] || return 1
  [[ "$output" != *"OK: dot-chorus"* ]] || return 1
}

@test "a copy that works reports OK and the run is green" {
  fake_rsync 0
  run_backup
  [ "$status" -eq 0 ] || return 1
  [[ "$output" == *"OK: dot-chorus"* ]] || return 1
  [[ "$output" == *"5 synced, 0 failed"* ]] || return 1
}

@test "negative proof: rsync says OK but the spine copy stayed behind its source (10-13), so the backup is red" {
  fake_rsync 0
  REMOTE_SIZE=4000 run_backup
  [ "$status" -eq 1 ] || return 1
  [[ "$output" == *"STALE: dot-chorus/.chorus/chorus.log copy is 4000 bytes, source was 5000"* ]] || return 1
  [[ "$output" == *"FAIL: dot-chorus (copy is behind its source)"* ]] || return 1
  [[ "$output" != *"OK: dot-chorus"* ]] || return 1
}
