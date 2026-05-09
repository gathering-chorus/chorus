#!/usr/bin/env bash
# test-alert-checks.sh — #2861 verification.
#
# For each of the three crawler alerts (crawler-error, crawler-stale,
# hydration-divergence), apply the alert-runner's exact extraction +
# execution contract and assert the check returns literal "ok" against
# a healthy-state fixture chorus.log.
#
# alert-runner contract (proving/scripts/alert-runner.sh:39-41):
#   check_script=$(awk '/^check: \|/{found=1; next} /^[a-z]/{if(found) exit} found{print}' "$rule_file")
#   result=$(bash -c "$check_script" 2>&1) || true
#   if [[ "$result" == "ok" ]]; then OK
#
# This test pins the fix: every healthy success path of every alert YAML
# returns exactly "ok", so alert-runner doesn't fire on healthy state.

set -euo pipefail

# #2856 canonical contract: emit "=== Results: N passed, M failed ===" on EXIT
trap '_rc=$?; if [ $_rc -eq 0 ]; then echo "=== Results: $PASS passed, $FAIL failed ==="; else echo "=== Results: $PASS passed, $FAIL failed ==="; fi' EXIT

PASS=0
FAIL=0

CHORUS_ROOT="${CHORUS_ROOT:-/Users/jeffbridwell/CascadeProjects/chorus-werk/kade}"
ALERT_DIR="$CHORUS_ROOT/proving/domains/alerts"
TMPDIR=$(mktemp -d)
trap_orig=$(trap -p EXIT)
trap 'rm -rf "$TMPDIR"; eval "$trap_orig"' EXIT

# Build a healthy chorus.log fixture: one recent crawler.domain.indexed (so
# crawler-stale sees a fresh timestamp), zero crawler.domain.failed (so
# crawler-error returns 0 hits).
NOW_ISO=$(date -u '+%Y-%m-%dT%H:%M:%S.000Z')
cat > "$TMPDIR/chorus.log" <<EOF
{"timestamp":"$NOW_ISO","level":"info","appName":"chorus-events","component":"lifecycle","event":"crawler.domain.indexed","role":"system","domain":"chorus","duration_ms":"5000","trigger":"file-watch"}
EOF

# Override HOME so the alert checks read OUR fixture, not the real chorus.log.
# Each alert reads "$HOME/.chorus/chorus.log".
mkdir -p "$TMPDIR/.chorus"
mv "$TMPDIR/chorus.log" "$TMPDIR/.chorus/chorus.log"

run_alert_check() {
  local rule_file="$1"
  local name
  name=$(grep '^name:' "$rule_file" | head -1 | sed 's/name: *//')
  local check_script
  check_script=$(awk '/^check: \|/{found=1; next} /^[a-z]/{if(found) exit} found{print}' "$rule_file")
  if [ -z "$check_script" ]; then
    echo "FAIL [$name]: empty check_script (awk extraction failed)"
    return 2
  fi
  # Run with HOME pointing at our fixture; PATH preserved so python3 + scripts found.
  HOME="$TMPDIR" CHORUS_ROOT="$CHORUS_ROOT" bash -c "$check_script" 2>&1
}

assert_check_returns_ok() {
  local rule_file="$1" label="$2"
  local result
  result=$(run_alert_check "$rule_file" || true)
  if [ "$result" = "ok" ]; then
    echo "PASS [$label]: check returned 'ok'"
    PASS=$((PASS+1))
  else
    echo "FAIL [$label]: check returned '$result' (expected 'ok')"
    FAIL=$((FAIL+1))
  fi
}

# AC1: crawler-error.yml — bash parse error eliminated, healthy state → ok
assert_check_returns_ok "$ALERT_DIR/crawler-error.yml" "crawler-error healthy"

# AC2: crawler-stale.yml — success path emits literal ok against fresh timestamp
assert_check_returns_ok "$ALERT_DIR/crawler-stale.yml" "crawler-stale fresh"

# AC3: hydration-divergence.yml — quiet-period and healthy-with-activity both → ok
# Today's git activity in chorus is low (low-activity branch); test the quiet path.
assert_check_returns_ok "$ALERT_DIR/hydration-divergence.yml" "hydration-divergence quiet"

# AC4 (negative): when crawler-stale has no events at all, it should fire (NOT ok)
empty_log_dir=$(mktemp -d)
mkdir -p "$empty_log_dir/.chorus"
: > "$empty_log_dir/.chorus/chorus.log"
result=$(HOME="$empty_log_dir" CHORUS_ROOT="$CHORUS_ROOT" bash -c "$(awk '/^check: \|/{found=1; next} /^[a-z]/{if(found) exit} found{print}' "$ALERT_DIR/crawler-stale.yml")" 2>&1 || true)
rm -rf "$empty_log_dir"
if [ "$result" != "ok" ]; then
  echo "PASS [crawler-stale-empty-log fires correctly]: returned '$result'"
  PASS=$((PASS+1))
else
  echo "FAIL [crawler-stale-empty-log fires correctly]: returned 'ok' but should have fired"
  FAIL=$((FAIL+1))
fi

# AC5: python sibling scripts callable in isolation (smoke — they parse, run, emit count)
NOW_PY_ISO=$(date -u '+%Y-%m-%dT%H:%M:%S.000Z')
PY_FIXTURE='{"timestamp":"'"$NOW_PY_ISO"'","event":"crawler.domain.failed","domain":"watching"}'
for sibling in crawler-error-check.py hydration-divergence-check.py; do
  if [ ! -f "$ALERT_DIR/$sibling" ]; then
    echo "FAIL [$sibling exists]: missing"
    FAIL=$((FAIL+1))
    continue
  fi
  case "$sibling" in
    crawler-error-check.py)
      result=$(echo "$PY_FIXTURE" | CUTOFF=$(date -u -v-10M '+%Y-%m-%dT%H:%M:%SZ') python3 "$ALERT_DIR/$sibling" 2>&1)
      ;;
    hydration-divergence-check.py)
      PY_FIXTURE_OK='{"timestamp":"'"$NOW_PY_ISO"'","event":"crawler.domain.indexed","domain":"chorus"}'
      result=$(echo "$PY_FIXTURE_OK" | python3 "$ALERT_DIR/$sibling" 2>&1)
      ;;
  esac
  if [[ "$result" =~ ^[0-9]+$ ]]; then
    echo "PASS [$sibling sibling smoke]: returned numeric '$result'"
    PASS=$((PASS+1))
  else
    echo "FAIL [$sibling sibling smoke]: returned '$result' (expected integer)"
    FAIL=$((FAIL+1))
  fi
done
# crawler-stale-check.py takes one log line, returns epoch
if [ -f "$ALERT_DIR/crawler-stale-check.py" ]; then
  result=$(echo "$PY_FIXTURE" | python3 "$ALERT_DIR/crawler-stale-check.py" 2>&1)
  if [[ "$result" =~ ^[0-9]{10}$ ]]; then
    echo "PASS [crawler-stale-check.py sibling smoke]: returned epoch '$result'"
    PASS=$((PASS+1))
  else
    echo "FAIL [crawler-stale-check.py sibling smoke]: returned '$result' (expected 10-digit epoch)"
    FAIL=$((FAIL+1))
  fi
else
  echo "FAIL [crawler-stale-check.py exists]: missing"
  FAIL=$((FAIL+1))
fi

if [ "$FAIL" -gt 0 ]; then exit 1; fi
exit 0
