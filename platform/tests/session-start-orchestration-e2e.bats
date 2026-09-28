#!/usr/bin/env bats
# @test-type: e2e — full-flow end-to-end
# @domain: roles — the product domain this suite guards (#4334)
load test_helper
bats_require_minimum_version 1.5.0
# session-start-orchestration-e2e.bats — #2416 zone (d) of #2311 follow-on audit
#
# What Jeff sees: a role boot that completes *all* SessionStart orchestration
# steps in one pass — cache rebuilt if stale, next-session.md merged, pulse
# regenerated, CLAUDE.md defensively regenerated (#3288: regen replaced the
# stamp-compare), .done written, Bridge subscriber alive. These tests prove
# the deployed binary does the whole orchestration end-to-end, not just
# individual pieces.
#
# #4336 — how the boot runs here without touching a live session.
#
# session-start hardcodes /tmp paths a live Silas session uses:
# /tmp/claude-session-init/<role>.{pending,done} (the boot gate),
# /tmp/session-context-<role>.md, /tmp/pulse-latest.json and
# /tmp/role-checkpoint-<role>.json. There is no env seam for them. So each
# boot runs under sandbox-exec with a profile that DENIES every file write
# outside $BATS_TEST_TMPDIR and all network — the shim cannot touch a live
# path even if it tries — and denies reads of the live cache and checkpoint,
# so the output depends only on this test's world. Everything else rides the
# shim's own seams:
#   CHORUS_ROOT      a fake root: roles/silas/next-session.md, a claudemd-gen stub
#   HOME             a fake home (no bridge-subscriber, no pulse drain secret)
#   CHORUS_PULSE_PATH, CHORUS_LOG_FILE, CHORUS_SESSIONS_DIR   files in the tmpdir
#   CHORUS_PRINCIPLES_FIXTURE_FILE, CHORUS_SESSION_ROWS_FIXTURE_DIR
#                    fixtures/4336/session-start/
#
# Three properties live ONLY in those hardcoded /tmp paths (the stale-cache
# rebuild, the .done marker, crash recovery from the checkpoint). Observing them
# means writing a live Silas path, so they run only with RUN_LIVE_INTEGRATION=true
# and snapshot/restore every live file into $BATS_TEST_TMPDIR.

# #3904 — resolve the INSTALLED shim (#2734: ~/.chorus/bin is the deploy
# artifact; target/release is a build artifact that may be stale or absent).
SHIM="$(command -v chorus-hook-shim || true)"
[ -x "$SHIM" ] || SHIM="$HOME/.chorus/bin/chorus-hook-shim"
[ -x "$SHIM" ] || SHIM="${CHORUS_ROOT}/platform/services/chorus-hooks/target/release/chorus-hook-shim"
FIXTURES="${CHORUS_ROOT}/platform/tests/fixtures/4336/session-start"
LIVE_CACHE="/tmp/session-context-silas.md"
LIVE_INIT_DIR="/tmp/claude-session-init"
LIVE_CHECKPOINT="/tmp/role-checkpoint-silas.json"
LIVE_FILES=("$LIVE_INIT_DIR/silas.pending" "$LIVE_INIT_DIR/silas.done" "$LIVE_CACHE" "/tmp/pulse-latest.json" "$LIVE_CHECKPOINT")

live_gate() {
  [ "${RUN_LIVE_INTEGRATION:-}" = "true" ] || skip "UNMEASURED — $1 lives only in a hardcoded /tmp path a live Silas session uses; set RUN_LIVE_INTEGRATION=true (#4336)"
}

setup() {
  [ -x "$SHIM" ] || skip "UNMEASURED — chorus-hook-shim is not installed or built (#4336)"
  command -v sandbox-exec >/dev/null || skip "UNMEASURED — sandbox-exec absent; cannot keep session-start off live /tmp paths (#4336)"
  T="$(cd "$BATS_TEST_TMPDIR" && pwd -P)"
  # #3608 — the shim honors CHORUS_SESSIONS_DIR; never the live registry.
  export CHORUS_SESSIONS_DIR="$T/sessions"
  mkdir -p "$T/sessions" "$T/home" "$T/root/roles/silas" "$T/root/platform/scripts" "$T/snap"
  # claudemd-gen stub: records that regen ran, exits $REGEN_EXIT (default 0)
  printf '#!/bin/bash\necho regen-ran >> "%s/regen.log"\nexit ${REGEN_EXIT:-0}\n' "$T" > "$T/root/platform/scripts/claudemd-gen"
  chmod +x "$T/root/platform/scripts/claudemd-gen"
  echo "NEXT-SESSION-FIXTURE-4336 pick up the card" > "$T/root/roles/silas/next-session.md"
  # snapshot every live path, so a live-gated case can put back exactly what was there
  local i=0 f
  for f in "${LIVE_FILES[@]}"; do
    if [ -e "$f" ]; then cp -p "$f" "$T/snap/$i"; fi
    i=$((i + 1))
  done
}

teardown() {
  # Only a live-gated case writes a live path; only then is anything put back.
  [ -n "${LIVE_TOUCHED:-}" ] || return 0
  local i=0 f
  for f in "${LIVE_FILES[@]}"; do
    if [ -e "$T/snap/$i" ]; then
      cmp -s "$T/snap/$i" "$f" 2>/dev/null || cp -p "$T/snap/$i" "$f"
    else
      rm -f "$f"
    fi
    i=$((i + 1))
  done
}

# boot [extra write allows] — session-start silas, sandboxed, in this test's world.
#   READ_CHECKPOINT=1  let the shim read the live checkpoint (crash-recovery case)
#   READ_CACHE=1       let the shim read the live cache (stale-cache case)
boot() {
  local allow="${1:-}" deny_read=""
  [ -n "${READ_CACHE:-}" ] || deny_read="$deny_read (literal \"/private$LIVE_CACHE\")"
  [ -n "${READ_CHECKPOINT:-}" ] || deny_read="$deny_read (literal \"/private$LIVE_CHECKPOINT\")"
  local profile="(version 1)(allow default)(deny network*)
(deny file-write* (subpath \"/\"))
(allow file-write* (subpath \"$T\") (literal \"/dev/null\") (literal \"/dev/tty\") (regex #\"^/dev/fd/\") $allow)"
  [ -n "$deny_read" ] && profile="$profile
(deny file-read* $deny_read)"
  run --separate-stderr env -i PATH=/usr/bin:/bin HOME="$T/home" CHORUS_ROOT="$T/root" \
    CHORUS_CONTEXT=test CHORUS_LOG_FILE="$T/spine.log" CHORUS_SESSIONS_DIR="$T/sessions" \
    CHORUS_PULSE_PATH="$T/pulse.json" CHORUS_PRINCIPLES_FIXTURE_FILE="$FIXTURES/principles.json" \
    CHORUS_PRINCIPLES_CACHE_FILE="$T/principles-cache.json" CHORUS_SESSION_ROWS_FIXTURE_DIR="$FIXTURES" \
    TMPDIR="$T" REGEN_EXIT="${REGEN_EXIT:-0}" \
    sandbox-exec -p "$profile" "$SHIM" session-start silas
}

# additionalContext of the last boot, on stdout
context() {
  printf '%s' "$output" | python3 -c "import sys,json; print(json.load(sys.stdin)['hookSpecificOutput']['additionalContext'])"
}

# --- AC: SessionStart binary is invocable ---

@test "chorus-hook-shim session-start returns hookSpecificOutput JSON" {
  # #4131 — the contract is the JSON on stdout; stderr stays out of it.
  boot
  [ "$status" -eq 0 ]
  echo "$output" | python3 -c "
import sys, json
d = json.loads(sys.stdin.read())
assert 'hookSpecificOutput' in d
assert d['hookSpecificOutput'].get('hookEventName') == 'SessionStart'
assert 'additionalContext' in d['hookSpecificOutput']
"
}

# --- AC: SessionStart completes in a reasonable bound --- moved to
# session-start-timing.test.sh (#4138: a perf row, shown in seconds, never red)

# --- AC: Pulse regenerated on each SessionStart ---

@test "session-start regenerates the pulse snapshot (mtime advances)" {
  # #4336 — the durable pulse (#3202: the source of truth; /tmp is a derived
  # cache) at CHORUS_PULSE_PATH, forced to look stale first.
  echo '{}' > "$T/pulse.json"
  touch -t 202601010000 "$T/pulse.json"
  before=$(stat -f %m "$T/pulse.json")
  boot
  [ "$status" -eq 0 ]
  after=$(stat -f %m "$T/pulse.json")
  [ "$after" -gt "$before" ]
  python3 -c "import json,sys; d=json.load(open(sys.argv[1])); assert isinstance(d, dict) and d, 'empty pulse'" "$T/pulse.json"
}

# --- AC: Context cache is rebuilt when stale ---

@test "session-start rebuilds stale context cache" {
  live_gate "the context cache"
  LIVE_TOUCHED=1
  # A cache older than 10 minutes must be rebuilt at boot...
  echo "old" > "$LIVE_CACHE"
  touch -t 202601010000 "$LIVE_CACHE"
  before=$(stat -f %m "$LIVE_CACHE")
  READ_CACHE=1 boot "(literal \"/private$LIVE_CACHE\")"
  [ "$status" -eq 0 ]
  [[ "$stderr" == *"Context cached: $LIVE_CACHE"* ]] || return 1
  [ "$(stat -f %m "$LIVE_CACHE")" -gt "$before" ]
  # ...and a fresh, non-empty one is not.
  READ_CACHE=1 boot "(literal \"/private$LIVE_CACHE\")"
  [[ "$stderr" != *"Context cached:"* ]] || return 1
}

# --- AC: Protocol-pass writes .done marker (end of orchestration) ---

@test "session-start writes .done on successful protocol check" {
  live_gate "the .done boot marker"
  LIVE_TOUCHED=1
  rm -f "$LIVE_INIT_DIR/silas.pending" "$LIVE_INIT_DIR/silas.done"
  boot "(subpath \"/private$LIVE_INIT_DIR\")"
  [ "$status" -eq 0 ]
  [ -f "$LIVE_INIT_DIR/silas.done" ]
}

# --- AC: the orchestration sequence runs: next-session merge → pulse → regen ---

@test "session-start orchestrates: next-session merge + .consumed → pulse → defensive regen" {
  # #4336 — was a grep of session.rs for each step's name. Now each step is
  # observed in the world the boot ran in. (The cache step is the live-gated
  # case above.)
  boot
  [ "$status" -eq 0 ]
  ctx="$(context)"
  # next-session merged into the context, then renamed so it is read once
  [[ "$ctx" == *"## Next Session Notes"* ]] || return 1
  [[ "$ctx" == *"NEXT-SESSION-FIXTURE-4336 pick up the card"* ]] || return 1
  [ ! -e "$T/root/roles/silas/next-session.md" ]
  [ -f "$T/root/roles/silas/next-session.md.consumed" ]
  # pulse assembled
  [ -s "$T/pulse.json" ]
  # #3288: defensive regen ran, and said so on the spine
  grep -q "^regen-ran$" "$T/regen.log"
  grep -q '"event":"session.bootstrap.regen_ok"' "$T/spine.log"
}

# --- AC: Crash recovery path is wired ---

@test "session-start branches on crash-recovery output" {
  live_gate "crash recovery (reads /tmp/role-checkpoint-silas.json)"
  LIVE_TOUCHED=1
  # A recent checkpoint and no current next-session.md → the boot resumes.
  rm -f "$T/root/roles/silas/next-session.md"
  printf '{"role":"silas","timestamp":"fixture-4336","state":"active","card_id":"4336","card_title":"fixture card","recent_files":[]}\n' > "$LIVE_CHECKPOINT"
  READ_CHECKPOINT=1 boot
  [ "$status" -eq 0 ]
  ctx="$(context)"
  [[ "$ctx" == *"## Crash Recovery"* ]] || return 1
  [[ "$ctx" == *"Resuming from checkpoint: card #4336: fixture card"* ]] || return 1
  # and with no checkpoint to read, no recovery section
  boot
  [[ "$(context)" != *"## Crash Recovery"* ]] || return 1
}

# --- AC: additionalContext is non-trivial (actual orchestration output, not stub) ---

@test "additionalContext contains session-context signal, not a stub" {
  boot
  [ "$status" -eq 0 ]
  echo "$output" | python3 -c "
import sys, json
d = json.loads(sys.stdin.read())
ctx = d['hookSpecificOutput']['additionalContext']
assert len(ctx) > 500, f'context too small: {len(ctx)}'
assert 'silas' in ctx.lower()
# the principles section built from the fixture, not a placeholder
assert 'Stack functions' in ctx, 'principles section missing'
"
}

# --- AC: Silent-partial-boot is not possible — regen failure surfaces in banner ---
# (#3288: the PROTOCOL VIOLATION stamp-compare banner is retired; the remaining
# runtime failure class is "claudemd-gen did not complete", surfaced loudly.)

@test "regen failure surfaces in additionalContext banner (not silently swallowed)" {
  # #4336 — a claudemd-gen that fails, and the banner + spine event it must produce.
  REGEN_EXIT=3 boot
  [ "$status" -eq 0 ]
  ctx="$(context)"
  [[ "$ctx" == *"CLAUDE.md regen failed at boot"* ]] || return 1
  grep -q '"event":"session.bootstrap.regen_failed".*"exit_code":"\{0,1\}3' "$T/spine.log"
  # NEGATIVE PROOF: a regen that succeeds carries no banner
  rm -f "$T/spine.log"
  echo "NEXT-SESSION-FIXTURE-4336 again" > "$T/root/roles/silas/next-session.md"
  REGEN_EXIT=0 boot
  [[ "$(context)" != *"regen failed at boot"* ]] || return 1
  run grep -q "regen_failed" "$T/spine.log"
  [ "$status" -ne 0 ]
}

# --- AC: Existing Rust orchestration tests remain (no regression) ---

@test "existing Rust orchestration tests remain in place" {
  # #4336 — was a file-exists check. Now: the test binaries compile and still
  # carry the named cases. (Running them needs RUN_INTEGRATION: they write the
  # same live /tmp paths.)
  command -v cargo >/dev/null || skip "UNMEASURED — cargo absent (#4336)"
  run bash -c "cd '${CHORUS_ROOT}/platform/services/chorus-hooks' && cargo test --release --quiet \
    --test session_start_additional_context --test session_start_pulse --test session_opening_narrative -- --list 2>/dev/null"
  [ "$status" -eq 0 ]
  [[ "$output" == *"session_start_emits_additional_context_json: test"* ]] || return 1
  [[ "$output" == *"session_start_writes_done: test"* ]] || return 1
  [[ "$output" == *"session_start_regenerates_pulse: test"* ]] || return 1
  [[ "$output" == *"boot_requires_verify_before_asserting: test"* ]] || return 1
}
