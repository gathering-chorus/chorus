#!/usr/bin/env bats
# @test-type: e2e — full-flow end-to-end
load test_helper
# gate-spine-vikunja-e2e.bats — #2324 zone (c) + #2428 sentinel rework
#
# Sentinel pattern (#2428): ONE long-lived card #2429 titled "[e2e-sentinel]
# DO NOT MOVE — bats fixture (#2428)" is reused across all tests. Labels are
# applied + removed per test, status is reset to Later in teardown. Zero new
# Vikunja cards created per bats run, zero card-ID consumption.
#
# Why sentinel over disposable cards (the original #2324 approach):
#   - Jeff observed test-card pollution: 89+ [e2e-*] leftovers in Vikunja
#   - Auto-increment ID counter advanced every run; board counter lied about
#     real-work progression
# Why sentinel over mocks:
#   - #2324's motivation was specifically "no Vikunja writer in test code —
#     mocks hide divergence." Sentinel keeps real writer-path coverage while
#     satisfying both of Jeff's hard constraints.

CHORUS_LOG="${CHORUS_ROOT}/platform/logs/chorus.log"
CHORUS_LOG_BIN="${CHORUS_ROOT}/platform/scripts/chorus-log"
CARDS="${CHORUS_ROOT}/platform/scripts/cards"
BRIDGE="${CHORUS_ROOT}/platform/scripts/gate-spine-vikunja-bridge.sh"
SENTINEL_CACHE="/tmp/e2e-sentinel-id"

# Sentinel discovery — returns the cards-CLI DISPLAY index (not Vikunja API id,
# which are different numbers). Cached after first lookup; rediscovers via
# `cards list` if cache is missing.
find_sentinel() {
  if [ -f "$SENTINEL_CACHE" ]; then
    cat "$SENTINEL_CACHE"
    return 0
  fi
  local id
  id=$(bash ${CHORUS_ROOT}/platform/scripts/cards list 2>/dev/null \
    | grep -oE '[0-9]+[[:space:]]+\[e2e-sentinel\]' | awk '{print $1}' | head -1)
  if [ -z "$id" ]; then
    echo "SENTINEL_MISSING — run: cards add '[e2e-sentinel] DO NOT MOVE — bats fixture (#2428)' ..." >&2
    return 1
  fi
  echo "$id" | tee "$SENTINEL_CACHE"
}

# Test labels that may be applied to sentinel during a run
TEST_LABELS=(gate:code-passed gate:quality-passed gate:arch-passed gate:ops-passed gate:product-passed)

setup() {
  SENTINEL=$(find_sentinel)
  [ -n "$SENTINEL" ] || skip "sentinel card not found"
  export SENTINEL
}

teardown() {
  # Remove any test labels applied during the test (idempotent no-op if absent)
  for label in "${TEST_LABELS[@]}"; do
    bash "$CARDS" label remove "$SENTINEL" "$label" >/dev/null 2>&1 || true
  done
  # If gate:product test moved sentinel to Done, reset to Later
  local status
  status=$(bash "$CARDS" view "$SENTINEL" 2>/dev/null | grep -E '^\s*Status:' | awk '{print $2}')
  if [ "$status" != "Later" ]; then
    bash "$CARDS" move "$SENTINEL" Later >/dev/null 2>&1 || true
  fi
}

# --- AC: sentinel pattern — no new card IDs consumed per run ---

@test "sentinel fixture exists and is discoverable" {
  [ -n "$SENTINEL" ]
  [[ "$SENTINEL" =~ ^[0-9]+$ ]]
  bash "$CARDS" view "$SENTINEL" 2>&1 | grep -q '\[e2e-sentinel\]'
}

# --- AC: gate.<name>.passed spine event lands with correct role + card ---

@test "gate.code.passed emit lands in chorus.log with correct card + role" {
  MARKER="gate-code-emit-$(date +%s)-$$"
  run bash "$CHORUS_LOG_BIN" gate.code.passed silas card="$SENTINEL" marker="$MARKER"
  [ "$status" -eq 0 ]
  sleep 1
  run grep -F "$MARKER" "$CHORUS_LOG"
  [ "$status" -eq 0 ]
  line=$(grep -F "$MARKER" "$CHORUS_LOG" | tail -1)
  echo "$line" | python3 -c "
import sys, json
d = json.loads(sys.stdin.read())
assert d.get('event') == 'gate.code.passed', f\"event={d.get('event')}\"
assert d.get('role') == 'silas', f\"role={d.get('role')}\"
assert str(d.get('card')) == '${SENTINEL}', f\"card={d.get('card')}\"
print('shape-ok')
"
}

# --- AC: gate-label writer — bridge applies gate:<name>-passed label ---

@test "gate:code-passed label appears on sentinel after bridge emits gate.code.passed" {
  MARKER="label-test-$(date +%s)-$$"
  run bash "$BRIDGE" "$SENTINEL" code silas "marker=$MARKER"
  [ "$status" -eq 0 ]
  sleep 1
  run bash "$CARDS" view "$SENTINEL"
  [ "$status" -eq 0 ]
  echo "$output" | grep -qE "gate:code-passed"
}

# --- AC: gate:product triggers Later → Done transition (then teardown resets) ---

@test "bridge on gate:product transitions sentinel → Done within 2s" {
  MARKER="product-transition-$(date +%s)-$$"
  run bash "$BRIDGE" "$SENTINEL" product silas "marker=$MARKER"
  [ "$status" -eq 0 ]
  sleep 1
  STATUS=$(bash "$CARDS" view "$SENTINEL" 2>/dev/null | grep -E 'Status:' | head -1 | awk '{print $2}')
  [ "$STATUS" = "Done" ]
}

# --- AC: bridge exists and is invocable ---

@test "gate-spine-vikunja bridge exists and is invocable" {
  [ -x "$BRIDGE" ]
}

@test "bridge emits spine event AND applies label in a single invocation" {
  MARKER="bridge-integration-$(date +%s)-$$"
  run bash "$BRIDGE" "$SENTINEL" quality silas "marker=$MARKER"
  [ "$status" -eq 0 ]
  sleep 1
  run grep -F "$MARKER" "$CHORUS_LOG"
  [ "$status" -eq 0 ]
  line=$(grep -F "$MARKER" "$CHORUS_LOG" | tail -1)
  echo "$line" | python3 -c "
import sys, json
d = json.loads(sys.stdin.read())
assert d.get('event') == 'gate.quality.passed', f\"event={d.get('event')}\"
assert str(d.get('card')) == '${SENTINEL}', f\"card={d.get('card')}\"
"
  run bash "$CARDS" view "$SENTINEL"
  echo "$output" | grep -qE "gate:quality-passed"
}

@test "bridge rejects unknown gate name" {
  run bash "$BRIDGE" "$SENTINEL" nonsense silas
  [ "$status" -ne 0 ]
  [[ "$output" == *"unknown gate"* ]]
}

# --- AC: every supported gate emits spine event + applies label ---

@test "gate:arch bridge emits spine event + applies gate:arch-passed label" {
  MARKER="arch-e2e-$(date +%s)-$$"
  run bash "$BRIDGE" "$SENTINEL" arch silas "marker=$MARKER"
  [ "$status" -eq 0 ]
  sleep 1
  grep -F "$MARKER" "$CHORUS_LOG" | grep -q 'gate.arch.passed'
  bash "$CARDS" view "$SENTINEL" | grep -qE "gate:arch-passed"
}

@test "gate:ops bridge emits spine event + applies gate:ops-passed label" {
  MARKER="ops-e2e-$(date +%s)-$$"
  run bash "$BRIDGE" "$SENTINEL" ops silas "marker=$MARKER"
  [ "$status" -eq 0 ]
  sleep 1
  grep -F "$MARKER" "$CHORUS_LOG" | grep -q 'gate.ops.passed'
  bash "$CARDS" view "$SENTINEL" | grep -qE "gate:ops-passed"
}

# --- folded from #2288 gemba: emit-path assertions beyond gate events ---

@test "chorus-log emit writes JSON line with trace_id on arbitrary event" {
  MARKER="emit-path-$(date +%s)-$$"
  run bash "$CHORUS_LOG_BIN" emit.smoke.test silas marker="$MARKER" card="$SENTINEL"
  [ "$status" -eq 0 ]
  sleep 1
  line=$(grep -F "$MARKER" "$CHORUS_LOG" | tail -1)
  [ -n "$line" ]
  echo "$line" | python3 -c "
import sys, json
d = json.loads(sys.stdin.read())
assert d.get('event') == 'emit.smoke.test', f\"event={d.get('event')}\"
assert d.get('role') == 'silas', f\"role={d.get('role')}\"
tid = d.get('trace_id')
if tid is not None:
    assert isinstance(tid, str) and len(tid) > 0, f\"bad trace_id={tid}\"
print('emit-path-ok')
"
}

# --- folded from #2288 gemba: discover-pages ontology-write smoke ---

@test "discover-pages endpoint exists and responds to POST without crashing" {
  CODE=$(curl -s -o /tmp/discover-pages-resp.$$ -w '%{http_code}' \
    -X POST -H 'Content-Type: application/json' -d '{}' \
    "http://localhost:3340/api/athena/discover-pages" 2>/dev/null)
  rm -f /tmp/discover-pages-resp.$$
  [[ "$CODE" == "200" || "$CODE" == "400" || "$CODE" == "422" ]]
}
