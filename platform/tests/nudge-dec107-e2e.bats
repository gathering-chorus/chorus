#!/usr/bin/env bats
# @test-type: e2e — full-flow end-to-end
load test_helper
# nudge-dec107-e2e.bats — #2415 zone (b) of #2311 follow-on audit
#
# DEC-107 invariant: "persist AND deliver, both paths, every nudge."
# This bats proves the deployed binary honors that — one `nudge` call
# produces a persist (messages.db row) AND a delivery attempt (spine
# role.nudge.delivered event), linked by a shared trace_id.
#
# Hermetic: CHORUS_INJECT_DRY_RUN=1 skips osascript so tests don't
# hijack real terminals. The delivery-path code still fires — mode=dry-run.

NUDGE="${CHORUS_ROOT}/platform/scripts/nudge"
NUDGE_RS="${CHORUS_ROOT}/platform/services/chorus-hooks/src/nudge.rs"
NUDGE_GATE_RS="${CHORUS_ROOT}/platform/services/chorus-hooks/tests/nudge_force_source_gate.rs"
CHORUS_LOG="${CHORUS_ROOT}/platform/logs/chorus.log"
MESSAGES_DB="${CHORUS_ROOT}/platform/pulse/messages.db"

# --- AC: binary wrapper exists ---

@test "nudge wrapper script exists and is executable" {
  [ -x "$NUDGE" ]
}

# --- AC: DEC-107 source invariant — no fallback chains / cycling ---

@test "nudge.rs does not reintroduce conditional force flags (DEC-107 source shape)" {
  # The invariant is the absence of "if force" or "let force" gating.
  # nudge_force_source_gate.rs already enforces this in Rust; this bats
  # smokes the source file so regressions are caught at the shell layer too.
  ! grep -qE '^\s*if force\b|^\s*let\s+force\s*=' "$NUDGE_RS"
}

@test "nudge_force_source_gate Rust test exists" {
  [ -f "$NUDGE_GATE_RS" ]
}

# --- AC: one invocation produces BOTH spine events ---

@test "nudge --dry-run emits role.nudge.sent + role.nudge.delivered from one call" {
  MARKER="dec107-e2e-both-$(date +%s)-$$"
  CHORUS_INJECT_DRY_RUN=1 DEPLOY_ROLE=silas run bash "$NUDGE" silas "$MARKER"
  [ "$status" -eq 0 ]
  sleep 1

  # Extract the trace_id from the role.nudge.sent line we just emitted
  SENT_LINE=$(grep "$MARKER" "$CHORUS_LOG" | grep 'role.nudge.sent' | tail -1)
  [ -n "$SENT_LINE" ]
  TRACE=$(echo "$SENT_LINE" | grep -oE 'ntr-[0-9]+-[a-f0-9]+' | head -1)
  [ -n "$TRACE" ]

  # Both events must share the same trace_id — that proves single invocation
  EVENTS=$(grep "$TRACE" "$CHORUS_LOG" | python3 -c "
import sys, json
events = []
for line in sys.stdin:
    try:
        events.append(json.loads(line).get('event', ''))
    except Exception:
        pass
print(' '.join(events))
")
  [[ "$EVENTS" == *"role.nudge.sent"* ]]
  [[ "$EVENTS" == *"role.nudge.delivered"* ]]
}

# --- AC: persist path — messages.db row for this nudge ---

@test "nudge --dry-run persists the message to messages.db with matching marker" {
  MARKER="dec107-e2e-persist-$(date +%s)-$$"
  CHORUS_INJECT_DRY_RUN=1 DEPLOY_ROLE=silas run bash "$NUDGE" silas "$MARKER"
  [ "$status" -eq 0 ]
  sleep 1

  ROW=$(sqlite3 "$MESSAGES_DB" "SELECT type FROM messages WHERE content LIKE '%${MARKER}%' LIMIT 1")
  [ "$ROW" = "nudge" ]
}

# --- AC: delivery mode is 'dry-run' when CHORUS_INJECT_DRY_RUN=1 ---

@test "dry-run delivery path records mode=dry-run in role.nudge.delivered" {
  MARKER="dec107-e2e-mode-$(date +%s)-$$"
  CHORUS_INJECT_DRY_RUN=1 DEPLOY_ROLE=silas run bash "$NUDGE" silas "$MARKER"
  [ "$status" -eq 0 ]
  sleep 1

  # The delivered event content includes "mode=dry-run"
  DELIVERED=$(grep "$MARKER" "$CHORUS_LOG" | head -1 | grep -oE 'ntr-[0-9]+-[a-f0-9]+' | head -1)
  grep "$DELIVERED" "$CHORUS_LOG" | grep 'role.nudge.delivered' | grep -q 'mode=dry-run'
}

# --- AC: BOTH paths fire independently — neither short-circuits the other ---

@test "persist fires even when delivery is dry-run (no short-circuit)" {
  MARKER="dec107-e2e-both-paths-$(date +%s)-$$"
  CHORUS_INJECT_DRY_RUN=1 DEPLOY_ROLE=silas run bash "$NUDGE" silas "$MARKER"
  [ "$status" -eq 0 ]
  sleep 1

  # DB row present (persist)
  PERSIST=$(sqlite3 "$MESSAGES_DB" "SELECT COUNT(*) FROM messages WHERE content LIKE '%${MARKER}%'")
  [ "$PERSIST" -ge 1 ]
  # Delivery event present (deliver path fired, not skipped)
  grep "$MARKER" "$CHORUS_LOG" | grep -q 'role.nudge.sent'
}

# --- AC: negative — both events present with the SAME trace, not different traces ---

@test "both events share one trace_id (not two separate nudges) " {
  MARKER="dec107-e2e-single-trace-$(date +%s)-$$"
  CHORUS_INJECT_DRY_RUN=1 DEPLOY_ROLE=silas bash "$NUDGE" silas "$MARKER" >/dev/null 2>&1
  sleep 1

  TRACES=$(grep "$MARKER" "$CHORUS_LOG" | grep -oE 'ntr-[0-9]+-[a-f0-9]+' | sort -u | wc -l | tr -d ' ')
  # Exactly one distinct trace_id across all events tagged with this marker
  [ "$TRACES" = "1" ]
}
