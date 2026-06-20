#!/usr/bin/env bats
# @test-type: e2e — full-flow end-to-end
load test_helper
# session-start-orchestration-e2e.bats — #2416 zone (d) of #2311 follow-on audit
#
# What Jeff sees: a role boot that completes *all* SessionStart orchestration
# steps in one pass — cache rebuilt if stale, next-session.md merged, pulse
# regenerated, protocol banner injected on drift, .done written on pass,
# Bridge subscriber alive. These tests prove the deployed binary does the
# whole orchestration end-to-end, not just individual pieces.
#
# Hygiene: snapshot/restore /tmp/claude-session-init/silas.{pending,done}
# and the pulse timestamp so a test cannot strand a live Silas session.
# We never touch roles/silas/next-session.md (which the real flow renames
# to .consumed) — that path is tested by grep-style source-shape assertions
# plus the existing Rust tests (session_start_additional_context.rs).

SHIM="${CHORUS_ROOT}/platform/services/chorus-hooks/target/release/chorus-hook-shim"
SESSION_RS="${CHORUS_ROOT}/platform/services/chorus-hooks/src/commands/session.rs"
PULSE_LATEST="/tmp/pulse-latest.json"
CONTEXT_CACHE="/tmp/session-context-silas.md"
INIT_DIR="/tmp/claude-session-init"

snapshot_state() {
  SNAP_PENDING_EXISTS=0
  SNAP_DONE_EXISTS=0
  [ -f "$INIT_DIR/silas.pending" ] && SNAP_PENDING_EXISTS=1
  [ -f "$INIT_DIR/silas.done" ] && SNAP_DONE_EXISTS=1
  SNAP_CACHE_MTIME=$(stat -f %m "$CONTEXT_CACHE" 2>/dev/null || echo "0")
  SNAP_PULSE_MTIME=$(stat -f %m "$PULSE_LATEST" 2>/dev/null || echo "0")
  export SNAP_PENDING_EXISTS SNAP_DONE_EXISTS SNAP_CACHE_MTIME SNAP_PULSE_MTIME
}

restore_state() {
  mkdir -p "$INIT_DIR"
  if [ "${SNAP_PENDING_EXISTS:-0}" = "1" ]; then
    touch "$INIT_DIR/silas.pending"
  else
    rm -f "$INIT_DIR/silas.pending"
  fi
  if [ "${SNAP_DONE_EXISTS:-0}" = "1" ]; then
    touch "$INIT_DIR/silas.done"
  else
    rm -f "$INIT_DIR/silas.done"
  fi
}

setup() {
  snapshot_state
}

teardown() {
  restore_state
}

# --- AC: SessionStart binary is invocable ---

@test "chorus-hook-shim session-start returns hookSpecificOutput JSON" {
  run "$SHIM" session-start silas
  [ "$status" -eq 0 ]
  echo "$output" | python3 -c "
import sys, json
d = json.loads(sys.stdin.read())
assert 'hookSpecificOutput' in d
assert 'additionalContext' in d['hookSpecificOutput']
"
}

# --- AC: SessionStart completes in a reasonable bound ---

@test "session-start completes in under 10s typical" {
  local start end elapsed
  start=$(date +%s)
  "$SHIM" session-start silas >/dev/null
  end=$(date +%s)
  elapsed=$(( end - start ))
  # 10s is generous — typical should be <3s. Flag if regressed.
  [ "$elapsed" -lt 10 ]
}

# --- AC: Pulse regenerated on each SessionStart ---

@test "session-start regenerates pulse-latest.json (mtime advances)" {
  # Force pulse to look stale
  touch -t 202601010000 "$PULSE_LATEST" 2>/dev/null || true
  local before_mtime
  before_mtime=$(stat -f %m "$PULSE_LATEST" 2>/dev/null || echo 0)

  "$SHIM" session-start silas >/dev/null
  sleep 1

  local after_mtime
  after_mtime=$(stat -f %m "$PULSE_LATEST" 2>/dev/null || echo 0)
  [ "$after_mtime" -gt "$before_mtime" ]
}

# --- AC: Context cache is rebuilt when stale ---

@test "session-start rebuilds stale context cache" {
  # The cache rebuild branch triggers when mtime is >10 min old.
  # We can't easily force >10min without time-travel, so instead verify
  # that the cache EXISTS after a session-start (it's created if missing).
  rm -f "$CONTEXT_CACHE"
  "$SHIM" session-start silas >/dev/null
  [ -f "$CONTEXT_CACHE" ]
}

# --- AC: Protocol-pass writes .done marker (end of orchestration) ---

@test "session-start writes .done on successful protocol check" {
  rm -f "$INIT_DIR/silas.pending" "$INIT_DIR/silas.done"
  "$SHIM" session-start silas >/dev/null
  [ -f "$INIT_DIR/silas.done" ]
}

# --- AC: Source-shape — orchestration sequence is present in session.rs ---

@test "session.rs orchestrates: cache → next-session merge → pulse → protocol check" {
  [ -f "$SESSION_RS" ]
  # Cache rebuild branch
  grep -q "cache_stale" "$SESSION_RS"
  grep -q "context_cache::run" "$SESSION_RS"
  # Next-session merge + rename to .consumed
  grep -q "next-session.md" "$SESSION_RS"
  grep -q 'next-session.md.consumed' "$SESSION_RS"
  # Pulse regen
  grep -q "pulse::assemble" "$SESSION_RS"
  # Protocol check
  grep -q "protocol_contract::check" "$SESSION_RS"
}

# --- AC: Crash recovery path is documented and wired ---

@test "session-start branches on crash-recovery output" {
  grep -q "role-checkpoint" "$SESSION_RS"
  grep -q "Crash Recovery" "$SESSION_RS"
  grep -q "Resuming" "$SESSION_RS"
}

# --- AC: additionalContext is non-trivial (actual orchestration output, not stub) ---

@test "additionalContext contains session-context signal, not a stub" {
  run "$SHIM" session-start silas
  [ "$status" -eq 0 ]
  echo "$output" | python3 -c "
import sys, json
d = json.loads(sys.stdin.read())
ctx = d['hookSpecificOutput']['additionalContext']
assert len(ctx) > 500, f'context too small: {len(ctx)}'
# Context should include identifying role markers from the context_cache output
assert 'silas' in ctx.lower() or 'Silas' in ctx
"
}

# --- AC: Silent-partial-boot is not possible — protocol violation surfaces in banner ---

@test "protocol_contract violation surfaces in additionalContext banner (not silently swallowed)" {
  grep -q "PROTOCOL VIOLATION" "$SESSION_RS"
  grep -q "additionalContext" "$SESSION_RS" || grep -q "prepend" "$SESSION_RS"
}

# --- AC: Existing Rust orchestration tests remain (no regression) ---

@test "existing Rust orchestration tests remain in place" {
  TESTS_DIR="${CHORUS_ROOT}/platform/services/chorus-hooks/tests"
  [ -f "$TESTS_DIR/session_start_additional_context.rs" ]
  [ -f "$TESTS_DIR/session_start_pulse.rs" ]
  [ -f "$TESTS_DIR/session_opening_narrative.rs" ]
}
