#!/usr/bin/env bats
# session-watcher.bats — Tests for chorus-watch-sessions.sh lockfile behavior
# Card #2227: stuck lockfile blocks all indexing permanently
#
# What Jeff sees: gemba and /chorus search show 2-day-old session data because
# a crashed index run left a lockfile that permanently blocks future indexing.

WATCHER_SCRIPT="$HOME/.chorus/scripts/chorus-watch-sessions.sh"
LOCKFILE="$HOME/.chorus/watcher.lock"
LOGFILE="$HOME/.chorus/watcher.log"

# Extract the watcher's lock-check logic so we can test it in isolation.
# The watcher's while-loop does:
#   if [ -f "$LOCKFILE" ]; then continue; fi
# We simulate this by sourcing a helper that mirrors the watcher's behavior.
LOCK_HELPER="$HOME/.chorus/scripts/watcher-lock-check.sh"

setup() {
  rm -f "$LOCKFILE"
}

teardown() {
  rm -f "$LOCKFILE"
}

# --- Test the actual watcher script's lock behavior ---

@test "watcher script has lock-check helper" {
  # After fix: a watcher-lock-check.sh helper exists and is sourceable
  [ -f "$LOCK_HELPER" ]
}

@test "stale lockfile (>300s) is recovered by lock-check" {
  # Create a lockfile backdated 10 minutes
  touch "$LOCKFILE"
  touch -t "$(date -v-10M '+%Y%m%d%H%M.%S')" "$LOCKFILE"

  # After fix: lock-check returns 0 (proceed) and removes stale lock
  run bash "$LOCK_HELPER" "$LOCKFILE" "$LOGFILE"
  [ "$status" -eq 0 ]
  # Stale lockfile should be gone
  [ ! -f "$LOCKFILE" ]
}

@test "fresh lockfile (<300s) blocks indexing" {
  # Lock created just now — indexing is in progress
  touch "$LOCKFILE"

  # lock-check returns 1 (skip) and lockfile remains
  run bash "$LOCK_HELPER" "$LOCKFILE" "$LOGFILE"
  [ "$status" -eq 1 ]
  [ -f "$LOCKFILE" ]
}

@test "no lockfile allows indexing" {
  rm -f "$LOCKFILE"

  # lock-check returns 0 (proceed)
  run bash "$LOCK_HELPER" "$LOCKFILE" "$LOGFILE"
  [ "$status" -eq 0 ]
}

@test "stale lock recovery is logged" {
  touch "$LOCKFILE"
  touch -t "$(date -v-10M '+%Y%m%d%H%M.%S')" "$LOCKFILE"

  run bash "$LOCK_HELPER" "$LOCKFILE" "$LOGFILE"
  [ "$status" -eq 0 ]
  # Log should contain a stale-lock recovery message
  grep -q "Stale lock detected" "$LOGFILE"
}
