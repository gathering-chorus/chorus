#!/usr/bin/env bash
# test-crawler-alerts-live.sh — #2817 receipt: each crawler alert correctly
# detects its target condition.
#
# Extracts the check: block from each alert YAML and runs it under three
# scenarios:
#   - normal state (alert should NOT fire; rc=0)
#   - simulated breakage (alert SHOULD fire; rc=1)
#
# Avoids triggering real ops-nudges by running only the check, not the action.

set -uo pipefail

PASS=0
FAIL=0
p() { PASS=$((PASS+1)); echo "  PASS: $*"; }
f() { FAIL=$((FAIL+1)); echo "  FAIL: $*"; }

CHORUS_ROOT="${CHORUS_ROOT:-/Users/jeffbridwell/CascadeProjects/chorus}"
ALERT_DIR="$CHORUS_ROOT/proving/domains/alerts"

extract_check() {
  # awk: capture lines after 'check: |' until the next top-level key.
  # Top-level keys = lines starting at column 1 followed by colon.
  awk '
    /^check: \|/ { in_check=1; next }
    in_check && /^[a-zA-Z_]+:/ { in_check=0 }
    in_check { sub(/^  /,""); print }
  ' "$1"
}

run_check() {
  local yaml="$1"
  local check
  check=$(extract_check "$yaml")
  bash -c "$check" >/dev/null 2>&1
  echo $?
}

echo "=== #2817 alert live-fire receipts ==="

# --- crawler-stale ---
echo "Test 1: crawler-stale fires when no indexed events recent"
RC=$(run_check "$ALERT_DIR/crawler-stale.yml")
echo "  current state rc=$RC"
# Crawler ran in last 5 min → expect rc=0 (fresh)
[ "$RC" = "0" ] && p "crawler-stale: rc=0 when crawler is fresh" || f "expected rc=0 for fresh state, got $RC"

# Simulate stale: temporarily redirect to empty log
TMPLOG=$(mktemp)
HOME_BACKUP="$HOME"
mkdir -p /tmp/fakeshome/.chorus
touch /tmp/fakeshome/.chorus/chorus.log
RC=$(HOME=/tmp/fakeshome run_check "$ALERT_DIR/crawler-stale.yml")
echo "  empty-log state rc=$RC"
[ "$RC" = "1" ] && p "crawler-stale: rc=1 when chorus.log has no indexed events" || f "expected rc=1 for stale state, got $RC"
rm -rf /tmp/fakeshome

# --- crawler-error ---
echo "Test 2: crawler-error fires when crawler.domain.failed events recent"
RC=$(run_check "$ALERT_DIR/crawler-error.yml")
echo "  current state rc=$RC"
# Should be quiet under normal state
[ "$RC" = "0" ] && p "crawler-error: rc=0 when no failures" || f "expected rc=0 for clean state, got $RC"

# Simulate failure: inject a synthetic crawler.domain.failed event into a fake log
mkdir -p /tmp/fakeshome/.chorus
NOW=$(date -u '+%Y-%m-%dT%H:%M:%SZ')
echo "{\"timestamp\":\"$NOW\",\"event\":\"crawler.domain.failed\",\"role\":\"system\",\"domain\":\"test\"}" > /tmp/fakeshome/.chorus/chorus.log
RC=$(HOME=/tmp/fakeshome run_check "$ALERT_DIR/crawler-error.yml")
echo "  injected-failure rc=$RC"
[ "$RC" = "1" ] && p "crawler-error: rc=1 when failed event present in last 10 min" || f "expected rc=1 for injected failure, got $RC"
rm -rf /tmp/fakeshome

# --- hydration-divergence ---
echo "Test 3: hydration-divergence detects high-activity-no-indexing"
RC=$(run_check "$ALERT_DIR/hydration-divergence.yml")
echo "  current state rc=$RC"
# Under quiet repo activity it should pass through to ok or low-activity
[ "$RC" = "0" ] && p "hydration-divergence: rc=0 when activity is fine OR low" || f "expected rc=0, got $RC"

# Positive case: synthesize high git activity AND empty chorus.log
# (zero indexed events in last 3 min) → expect rc=1.
# Build a fake chorus repo with 12 recent commits touching distinct files,
# then point CHORUS_ROOT at it and HOME at an empty log.
FAKE_REPO=$(mktemp -d -t fake-chorus-divergence.XXXX)
(
  cd "$FAKE_REPO" && git init -q && git config user.email t@t && git config user.name t
  for i in $(seq 1 12); do
    echo "v1" > "f${i}.txt"
    git add "f${i}.txt" >/dev/null
    git commit -q -m "synth ${i}"
  done
) >/dev/null 2>&1
mkdir -p /tmp/fakeshome-div/.chorus
touch /tmp/fakeshome-div/.chorus/chorus.log
# The yaml hardcodes CHORUS_ROOT — temporarily rewrite check via env override.
DIV_CHECK=$(extract_check "$ALERT_DIR/hydration-divergence.yml" \
  | sed "s|/Users/jeffbridwell/CascadeProjects/chorus|$FAKE_REPO|g")
RC_OUT=$(HOME=/tmp/fakeshome-div bash -c "$DIV_CHECK" >/dev/null 2>&1; echo $?)
echo "  high-activity-empty-log rc=$RC_OUT"
[ "$RC_OUT" = "1" ] && p "hydration-divergence: rc=1 when activity high + indexing zero" || f "expected rc=1, got $RC_OUT"
rm -rf "$FAKE_REPO" /tmp/fakeshome-div

echo ""
echo "=== Results: $PASS passed, $FAIL failed ==="
[ "$FAIL" -eq 0 ]
