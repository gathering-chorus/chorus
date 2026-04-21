#!/usr/bin/env bats
# gate-spine-vikunja-e2e.bats — #2324 zone (c)
# What Jeff sees: three surfaces coherent whenever a gate passes —
#   (1) skill output, (2) spine event, (3) Vikunja card label/status.
# These tests prove the full chain; failure means divergence.

CHORUS_LOG="/Users/jeffbridwell/CascadeProjects/chorus/platform/logs/chorus.log"
CHORUS_LOG_BIN="/Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/chorus-log"
CARDS="/Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/cards"
BRIDGE="/Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/gate-spine-vikunja-bridge.sh"

# Use a disposable test card created in setup; torn down in teardown.
# Marker in title so stray test cards are identifiable.
TEST_TITLE_PREFIX="[e2e-2324]"

setup() {
  # Create a disposable test card; capture ID.
  TEST_CARD_ID=$(bash "$CARDS" add "${TEST_TITLE_PREFIX} gate-e2e scratch $$-$(date +%s)" \
    --owner silas --priority P3 --domain chorus --type chore --origin reactive --quick 2>&1 \
    | grep -oE 'Added #[0-9]+' | grep -oE '[0-9]+')
  [ -n "$TEST_CARD_ID" ] || skip "Could not create test card"
  export TEST_CARD_ID
}

teardown() {
  # Move test card to Won't Do — idempotent, keeps audit trail, no delete required.
  [ -n "${TEST_CARD_ID:-}" ] && bash "$CARDS" move "$TEST_CARD_ID" "Won't Do" >/dev/null 2>&1 || true
}

# --- AC: gate.<name>.passed spine event lands with correct role + card ---

@test "gate.code.passed emit lands in chorus.log with correct card + role" {
  MARKER="gate-code-emit-$(date +%s)-$$"
  # Attach marker as extra KV so we can uniquely find this emission
  run bash "$CHORUS_LOG_BIN" gate.code.passed silas card="$TEST_CARD_ID" marker="$MARKER"
  [ "$status" -eq 0 ]

  # Give the log 1s to flush
  sleep 1
  run grep -F "$MARKER" "$CHORUS_LOG"
  [ "$status" -eq 0 ]

  # Shape: event name + role + card
  line=$(grep -F "$MARKER" "$CHORUS_LOG" | tail -1)
  echo "$line" | python3 -c "
import sys, json
d = json.loads(sys.stdin.read())
assert d.get('event') == 'gate.code.passed', f\"event={d.get('event')}\"
assert d.get('role') == 'silas', f\"role={d.get('role')}\"
assert str(d.get('card')) == '${TEST_CARD_ID}', f\"card={d.get('card')}\"
print('shape-ok')
"
}

# --- AC: divergence detected — event emitted but board did not move ---

@test "divergence detector: spine event with no corresponding board change surfaces as drift" {
  # Current state: card is in whatever status cards add puts it (Later).
  # Emit a fake gate.product.passed — in a coherent world the card moves to Done.
  # The drift-detector must notice that emission-without-transition = failure.
  MARKER="drift-test-$(date +%s)-$$"
  bash "$CHORUS_LOG_BIN" gate.product.passed silas card="$TEST_CARD_ID" marker="$MARKER" >/dev/null

  # Read board state after emission
  sleep 1
  STATUS=$(bash "$CARDS" view "$TEST_CARD_ID" 2>/dev/null | grep -E 'Status:' | head -1 | awk '{print $2}')

  # This test documents the current divergence: emit happens, board doesn't.
  # The *production* drift detector (once built) will close this gap — for now,
  # the assertion is that status is NOT Done, proving the drift exists today.
  [ "$STATUS" != "Done" ]
}

# --- AC: gate-label writer — gate:<name>-passed label applied when gate emits ---
# Expected RED until the bridge (spine → Vikunja label) is built.

@test "gate:code-passed label appears on card after bridge emits gate.code.passed" {
  MARKER="label-test-$(date +%s)-$$"
  run bash "$BRIDGE" "$TEST_CARD_ID" code silas "marker=$MARKER"
  [ "$status" -eq 0 ]

  sleep 1
  run bash "$CARDS" view "$TEST_CARD_ID"
  [ "$status" -eq 0 ]
  echo "$output" | grep -qE "gate:code-passed"
}

# --- AC: gate:product triggers WIP → Done transition ---
# Expected RED until the bridge writes status changes.

@test "bridge on gate:product transitions card WIP → Done within 2s" {
  bash "$CARDS" move "$TEST_CARD_ID" WIP >/dev/null 2>&1 || true

  MARKER="product-transition-$(date +%s)-$$"
  run bash "$BRIDGE" "$TEST_CARD_ID" product silas "marker=$MARKER"
  [ "$status" -eq 0 ]

  sleep 1
  STATUS=$(bash "$CARDS" view "$TEST_CARD_ID" 2>/dev/null | grep -E 'Status:' | head -1 | awk '{print $2}')
  [ "$STATUS" = "Done" ]
}

# --- AC: bridge reads chorus.log, not a mock ---

@test "gate-spine-vikunja bridge exists and is invocable" {
  [ -x "$BRIDGE" ]
}

@test "bridge emits spine event AND applies label in a single invocation" {
  MARKER="bridge-integration-$(date +%s)-$$"
  run bash "$BRIDGE" "$TEST_CARD_ID" quality silas "marker=$MARKER"
  [ "$status" -eq 0 ]

  # Spine event lands
  sleep 1
  run grep -F "$MARKER" "$CHORUS_LOG"
  [ "$status" -eq 0 ]
  line=$(grep -F "$MARKER" "$CHORUS_LOG" | tail -1)
  echo "$line" | python3 -c "
import sys, json
d = json.loads(sys.stdin.read())
assert d.get('event') == 'gate.quality.passed', f\"event={d.get('event')}\"
assert str(d.get('card')) == '${TEST_CARD_ID}', f\"card={d.get('card')}\"
"
  # Label applied
  run bash "$CARDS" view "$TEST_CARD_ID"
  echo "$output" | grep -qE "gate:quality-passed"
}

@test "bridge rejects unknown gate name" {
  run bash "$BRIDGE" "$TEST_CARD_ID" nonsense silas
  [ "$status" -ne 0 ]
  [[ "$output" == *"unknown gate"* ]]
}

# --- AC: every supported gate emits spine event + applies label ---

@test "gate:arch bridge emits spine event + applies gate:arch-passed label" {
  MARKER="arch-e2e-$(date +%s)-$$"
  run bash "$BRIDGE" "$TEST_CARD_ID" arch silas "marker=$MARKER"
  [ "$status" -eq 0 ]
  sleep 1
  grep -F "$MARKER" "$CHORUS_LOG" | grep -q 'gate.arch.passed'
  bash "$CARDS" view "$TEST_CARD_ID" | grep -qE "gate:arch-passed"
}

@test "gate:ops bridge emits spine event + applies gate:ops-passed label" {
  MARKER="ops-e2e-$(date +%s)-$$"
  run bash "$BRIDGE" "$TEST_CARD_ID" ops silas "marker=$MARKER"
  [ "$status" -eq 0 ]
  sleep 1
  grep -F "$MARKER" "$CHORUS_LOG" | grep -q 'gate.ops.passed'
  bash "$CARDS" view "$TEST_CARD_ID" | grep -qE "gate:ops-passed"
}

# --- folded from #2288 gemba: emit-path assertions beyond gate events ---

@test "chorus-log emit writes JSON line with trace_id on arbitrary event" {
  MARKER="emit-path-$(date +%s)-$$"
  run bash "$CHORUS_LOG_BIN" emit.smoke.test silas marker="$MARKER" card="$TEST_CARD_ID"
  [ "$status" -eq 0 ]
  sleep 1
  line=$(grep -F "$MARKER" "$CHORUS_LOG" | tail -1)
  [ -n "$line" ]
  echo "$line" | python3 -c "
import sys, json
d = json.loads(sys.stdin.read())
assert d.get('event') == 'emit.smoke.test', f\"event={d.get('event')}\"
assert d.get('role') == 'silas', f\"role={d.get('role')}\"
# trace_id is optional in some events but when present it's a non-empty string
tid = d.get('trace_id')
if tid is not None:
    assert isinstance(tid, str) and len(tid) > 0, f\"bad trace_id={tid}\"
print('emit-path-ok')
"
}

# --- folded from #2288 gemba: discover-pages ontology-write smoke ---

@test "discover-pages endpoint exists and responds to POST without crashing" {
  # Light smoke only — full Fuseki-triple verification belongs in an API-level test.
  # This proves the endpoint is wired up and not returning 404 or 500 silently.
  CODE=$(curl -s -o /tmp/discover-pages-resp.$$ -w '%{http_code}' \
    -X POST -H 'Content-Type: application/json' -d '{}' \
    "http://localhost:3340/api/athena/discover-pages" 2>/dev/null)
  rm -f /tmp/discover-pages-resp.$$
  # Accept 200 (success) or 4xx with structured error; fail on 404 / 5xx / connection error
  [[ "$CODE" == "200" || "$CODE" == "400" || "$CODE" == "422" ]]
}
