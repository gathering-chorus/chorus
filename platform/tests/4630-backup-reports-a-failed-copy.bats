#!/usr/bin/env bats
# @test-type: unit — rsync-backup.sh with a fake rsync and a fake ssh in a temp dir; nothing leaves the machine
# @domain: persistence — backups are persistence; there is no backups domain (#4670)
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

@test "#4705: sockets are skipped (--no-specials), so a werk's live .sock cannot fail the copy" {
  printf '#!/bin/sh\necho "$*" >> "%s/rsync-args"\nexit 0\n' "$T" > "$T/bin/rsync"; chmod +x "$T/bin/rsync"
  run_backup
  [ "$status" -eq 0 ] || return 1
  [ "$(grep -c -- '--no-specials' "$T/rsync-args")" -eq "$(wc -l < "$T/rsync-args")" ] || return 1
}

@test "#4705 negative proof: with --no-specials on, a real unreadable file (exit 23) still reads FAIL, never OK" {
  # rsync exits 23 only when it was given --no-specials: the socket skip cannot hide a real failure
  printf '#!/bin/sh\ncase "$*" in *--no-specials*) echo "rsync: open \"secret\" failed: Permission denied (13)" >&2; exit 23;; esac\nexit 0\n' > "$T/bin/rsync"; chmod +x "$T/bin/rsync"
  run_backup
  [ "$status" -eq 1 ] || return 1
  [[ "$output" == *"FAIL: dot-chorus (rsync exit 23 after 2 attempts)"* ]] || return 1
  [[ "$output" != *"OK: dot-chorus"* ]] || return 1
}

@test "#4705: openrsync's exit 23 for files that vanished mid-copy (ENOENT only) reads as vanished, not FAIL" {
  printf '#!/bin/sh\necho "rsync(1): error: /h/.chorus/sessions/kade.turn.json: open (2) in /x: No such file or directory" >&2\nexit 23\n' > "$T/bin/rsync"; chmod +x "$T/bin/rsync"
  run_backup
  [ "$status" -eq 0 ] || return 1
  [[ "$output" == *"OK, 1 vanished: dot-chorus (rsync exit 24)"* ]] || return 1
}

@test "#4705 negative proof: one non-ENOENT error among the vanished ones keeps exit 23 and reads FAIL" {
  printf '#!/bin/sh\necho "rsync(1): error: /h/a: open (2) in /x: No such file or directory" >&2\necho "rsync(1): error: /h/b: open (13) in /x: Permission denied" >&2\nexit 23\n' > "$T/bin/rsync"; chmod +x "$T/bin/rsync"
  run_backup
  [ "$status" -eq 1 ] || return 1
  [[ "$output" == *"FAIL: dot-chorus (rsync exit 23 after 2 attempts)"* ]] || return 1
}

@test "a copy that works reports OK and the run is green" {
  fake_rsync 0
  run_backup
  [ "$status" -eq 0 ] || return 1
  [[ "$output" == *"OK: dot-chorus"* ]] || return 1
  [[ "$output" == *"5 synced, 0 failed"* ]] || return 1
}

@test "the live spine is not mirrored, and vanished live files are named, never a plain OK" {
  printf '#!/bin/sh\necho "$*" >> "%s/rsync-args"\necho "file has vanished: \"/x/a\""\necho "file has vanished: \"/x/b\""\nexit 24\n' "$T" > "$T/bin/rsync"; chmod +x "$T/bin/rsync"
  run_backup
  [ "$status" -eq 0 ] || return 1
  grep -q -- "--exclude=/.chorus/chorus.log" "$T/rsync-args" || return 1
  [[ "$output" == *"OK, 2 vanished: dot-chorus (rsync exit 24)"* ]] || return 1
  [[ "$output" != *"OK: dot-chorus"* ]] || return 1
}
