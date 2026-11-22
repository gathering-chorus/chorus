#!/usr/bin/env bats
# @test-type: integration — operational; live services, skip-if-absent in CI
# @domain: messages — the product domain this suite guards (#4334)
load test_helper
# nudge-health.bats — Tests for nudge health check (#1847)
# What Jeff sees: macOS notifications saying "3 role(s) unreachable"
# when all three sessions are running. Zombie Terminal windows crash
# the osascript lookup and the whole check fails.

# #3915 — was ${CHORUS_ROOT:-${CHORUS_ROOT}}: a tautology that resolves to an
# EMPTY path whenever CHORUS_ROOT is unset, which is exactly how the nightly
# runs it. The script was fine; the test could not find it and reported the
# health check as broken every night.
HEALTH_SCRIPT="${CHORUS_ROOT:-$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)}/platform/scripts/nudge-health-check.sh"

# --- AC 1: Health check survives zombie windows ---

@test "health check script exists and is executable" {
  [ -x "$HEALTH_SCRIPT" ]
}

# #4336 — a hermetic world for running the REAL script. It hardcodes
# CANARY_DIR=/tmp/nudge-canary and POSTs an alert to the Clearing (3470) on any
# failure, so each run uses a copy with the canary dir pointed into the test's
# tmpdir (the copy is checked to differ — a silent no-op sed would fall back to
# /tmp), a curl stub that records instead of posting, and a CHORUS_ROOT whose
# chorus-log is a recorder. osascript/pgrep/tmux are stubbed only when a case
# asks (STUB_OSA / STUB_PGREP_RC; #4916 retired tmux from the script); otherwise the real ones run.
hc_world() {
  W="$BATS_TEST_TMPDIR/hc"
  mkdir -p "$W/bin" "$W/root/platform/scripts" "$W/where" "$W/canary"
  cat <<'JSON' > "$W/roles.json"
{"data":[{"name":"wren","roleKind":"agent"},{"name":"silas","roleKind":"agent"},{"name":"kade","roleKind":"agent"},{"name":"borg-queen","roleKind":"agent"},{"name":"abby-normal","roleKind":"agent"}]}
JSON
  cp "$HEALTH_SCRIPT" "$W/health.sh"
  printf '#!/bin/bash\necho "$*" >> "%s/curl.calls"\n[[ "$*" == *"file://"* ]] && exec /usr/bin/curl "$@"\nif [[ "$*" == *"/api/actor-state"* ]]; then echo "{\\"actors\\":[{\\"aboutPrincipal\\":\\"principal-wren\\",\\"sessions\\":[{\\"live\\":true}]},{\\"aboutPrincipal\\":\\"principal-silas\\",\\"sessions\\":[{\\"live\\":true}]},{\\"aboutPrincipal\\":\\"principal-kade\\",\\"sessions\\":[{\\"live\\":true}]},{\\"aboutPrincipal\\":\\"principal-borg-queen\\",\\"sessions\\":[{\\"live\\":true}]},{\\"aboutPrincipal\\":\\"principal-abby-normal\\",\\"sessions\\":[{\\"live\\":true}]}]}"; fi\n' "$W" > "$W/bin/curl"
  printf '#!/bin/bash\necho "$*" >> "%s/spine.calls"\n' "$W" > "$W/root/platform/scripts/chorus-log"
  if [ -n "${STUB_OSA:-}" ]; then
    # records the AppleScript it was asked to run; answers "<count>::<name>"
    printf '#!/bin/bash\necho "$*" | tr "\\n" " " >> "%s/osa.calls"; echo >> "%s/osa.calls"\necho "%s"\n' "$W" "$W" "$STUB_OSA" > "$W/bin/osascript"
  fi
  if [ -n "${STUB_PGREP_RC:-}" ]; then
    printf '#!/bin/bash\nexit %s\n' "$STUB_PGREP_RC" > "$W/bin/pgrep"
  fi
  chmod +x "$W/bin/"* "$W/root/platform/scripts/chorus-log"
}

# register <role> <host> [pane] — #4895: where pulse says the role is (/api/where/<role>)
register() {
  mkdir -p "$W/where"
  printf '{"role":"%s","kind":"resolved","pane":"%s","reachability":"reachable"}' "$1" "${3:-}" > "$W/where/$1"
}

run_health() {
  run env PATH="$W/bin:$PATH" CHORUS_ROOT="$W/root" CHORUS_ROLES_URL="file://$W/roles.json" WHERE_URL="file://$W/where" CANARY_DIR="$W/canary" bash "$W/health.sh"
  echo "output: $output"
  [ -f "$W/canary/health.log" ] || { echo "canary health.log does not exist in $W/canary" >&2; return 1; }
}

# #4925 — the two live cases ("succeeds when role sessions are running", "reports all
# roles reachable") were deleted: gated on RUN_LIVE_INTEGRATION, which no lane sets, they
# skipped every night. Covered by platform/tests/4895-every-role-reachable.bats, which
# asks pulse where every agent role is and that its newest nudge was delivered.

# --- AC 3: Still detects genuinely missing roles ---

# #4786 (Slice 3 of #4780): nudge-health judges delivery results and run liveness,
# not Terminal windows.
@test "health check detects delivery failures from messages.db" {
  hc_world
  STUB_FAILED_NUDGES="silas|unknown role: silas" run_health
  [ "$status" -eq 1 ]
  echo "$output" | grep -q "FAIL: silas — nudge delivery failed: unknown role: silas"
  grep -q "role=silas,reason=delivery-failed" "$W/spine.calls"
}

@test "health check detects delivery path refusal for unknown role" {
  hc_world
  ROLES_OVERRIDE="unknown-actor" run_health
  [ "$status" -eq 1 ]
  echo "$output" | grep -q "FAIL: unknown-actor — delivery path refuses role (unknown role: unknown-actor)"
  grep -q "role=unknown-actor,reason=unknown-role" "$W/spine.calls"
}

@test "NEGATIVE: all roles reachable when live sessions exist and no delivery errors" {
  hc_world
  for r in wren silas kade borg-queen abby-normal; do
    register "$r" tmux "%1"
  done
  run_health
  [ "$status" -eq 0 ]
  echo "$output" | grep -q "NUDGE HEALTH: all roles reachable"
}

@test "health check queries only latest nudge per role from messages.db" {
  hc_world
  for r in wren silas kade borg-queen abby-normal; do
    register "$r" tmux "%1"
  done
  DB="$W/messages.db"
  sqlite3 "$DB" "
    CREATE TABLE messages (
      id INTEGER PRIMARY KEY,
      type TEXT,
      \"to\" TEXT,
      delivery_status TEXT,
      last_delivery_error TEXT,
      created_at TEXT
    );
    -- Older failure
    INSERT INTO messages (id, type, \"to\", delivery_status, last_delivery_error, created_at)
    VALUES (1, 'nudge', 'silas', 'failed', 'no-window-found', datetime('now', '-1 hour'));
    -- Newer success
    INSERT INTO messages (id, type, \"to\", delivery_status, last_delivery_error, created_at)
    VALUES (2, 'nudge', 'silas', 'delivered', NULL, datetime('now'));
  "

  # Older failure ignored because latest succeeded
  CHORUS_MESSAGES_DB="$DB" run_health
  [ "$status" -eq 0 ]
  echo "$output" | grep -q "NUDGE HEALTH: all roles reachable"

  # NEGATIVE PROOF: latest nudge failed -> must fail and name error
  sqlite3 "$DB" "
    INSERT INTO messages (id, type, \"to\", delivery_status, last_delivery_error, created_at)
    VALUES (3, 'nudge', 'silas', 'failed', 'connection-reset-negative-proof', datetime('now'));
  "
  CHORUS_MESSAGES_DB="$DB" run_health
  [ "$status" -eq 1 ]
  echo "$output" | grep -q "FAIL: silas — nudge delivery failed: connection-reset-negative-proof"
}
