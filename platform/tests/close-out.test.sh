#!/usr/bin/env bash
# @test-type: unit — hermetic source guard
# close-out.test.sh — #2230 test: /close script writes all 3 artifacts
#
# Tests what Jeff sees in session-state artifacts after /close runs:
#   - roles/<role>/next-session.md has the paragraph + session summary
#   - activity.md has a one-line session-close entry
#   - roles/<role>/journal/YYYY-MM-DD.md has a journal entry
#
# Does NOT test session-close.sh (board audit + commit) — that's #1866's territory,
# our script just invokes it. Uses --dry-run to avoid real commits in tests.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
SCRIPT="${REPO_ROOT}/platform/scripts/close-out.sh"

# Setup isolated temp workspace
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

pass=0
fail=0

test_case() {
  local name="$1"
  shift
  if "$@"; then
    echo "PASS: $name"
    pass=$((pass + 1))
  else
    echo "FAIL: $name"
    fail=$((fail + 1))
  fi
}

# Build a throwaway repo shape
setup_fake_repo() {
  local base="$1"
  mkdir -p "${base}/roles/wren/journal"
  mkdir -p "${base}/platform/scripts"
  touch "${base}/activity.md"
  touch "${base}/roles/wren/next-session.md"
  # Stub session-close.sh so we don't try to commit during tests
  cat > "${base}/platform/scripts/session-close.sh" <<'EOF'
#!/usr/bin/env bash
echo "stub: session-close $*"
exit 0
EOF
  chmod +x "${base}/platform/scripts/session-close.sh"
  # Copy the real close-out.sh
  cp "${SCRIPT}" "${base}/platform/scripts/close-out.sh"
}

# Test 1: --dry-run prints planned writes without touching files
test_dry_run_no_writes() {
  local base="${TMP}/t1"
  setup_fake_repo "$base"
  local before_mtime
  before_mtime=$(stat -f %m "${base}/roles/wren/next-session.md" 2>/dev/null || echo "0")
  CHORUS_ROOT="$base" bash "${base}/platform/scripts/close-out.sh" wren "test paragraph" --dry-run > "${TMP}/dry-out.txt" 2>&1 || return 1
  # Check dry-run output mentions all 3 artifacts
  grep -q "next-session.md" "${TMP}/dry-out.txt" || return 1
  grep -q "activity.md" "${TMP}/dry-out.txt" || return 1
  grep -q "journal/" "${TMP}/dry-out.txt" || return 1
  # Check files NOT modified
  local after_mtime
  after_mtime=$(stat -f %m "${base}/roles/wren/next-session.md" 2>/dev/null || echo "0")
  [ "$before_mtime" = "$after_mtime" ] || return 1
  return 0
}

# Test 2: Real run writes all 3 artifacts
test_writes_three_artifacts() {
  local base="${TMP}/t2"
  setup_fake_repo "$base"
  CHORUS_ROOT="$base" bash "${base}/platform/scripts/close-out.sh" wren "shipped #999 and #998, blocked on foo" > "${TMP}/run-out.txt" 2>&1 || return 1
  # Check next-session.md contains paragraph
  grep -q "shipped #999 and #998" "${base}/roles/wren/next-session.md" || { echo "next-session missing paragraph"; return 1; }
  # Check activity.md has a new entry
  grep -q "wren" "${base}/activity.md" || { echo "activity.md missing wren entry"; return 1; }
  grep -q "shipped" "${base}/activity.md" || { echo "activity.md missing summary"; return 1; }
  # Check journal entry exists for today
  local today
  today=$(date '+%Y-%m-%d')
  [ -f "${base}/roles/wren/journal/${today}.md" ] || { echo "journal entry for ${today} missing"; return 1; }
  grep -q "shipped #999 and #998" "${base}/roles/wren/journal/${today}.md" || { echo "journal missing paragraph"; return 1; }
  return 0
}

# Test 3: Rejects invalid role
test_invalid_role() {
  local base="${TMP}/t3"
  setup_fake_repo "$base"
  local exit_code=0
  CHORUS_ROOT="$base" bash "${base}/platform/scripts/close-out.sh" invalid "paragraph" > /dev/null 2>&1 || exit_code=$?
  [ "$exit_code" != "0" ] || return 1
  return 0
}

# Test 4: Requires non-empty paragraph
test_requires_paragraph() {
  local base="${TMP}/t4"
  setup_fake_repo "$base"
  local exit_code=0
  CHORUS_ROOT="$base" bash "${base}/platform/scripts/close-out.sh" wren "" > /dev/null 2>&1 || exit_code=$?
  [ "$exit_code" != "0" ] || return 1
  return 0
}

# Test 5: Journal appends when entry exists for today (doesn't overwrite)
test_journal_appends() {
  local base="${TMP}/t5"
  setup_fake_repo "$base"
  local today
  today=$(date '+%Y-%m-%d')
  # Prime journal with existing content
  echo "# Earlier session" > "${base}/roles/wren/journal/${today}.md"
  echo "Earlier work done." >> "${base}/roles/wren/journal/${today}.md"
  CHORUS_ROOT="$base" bash "${base}/platform/scripts/close-out.sh" wren "second session paragraph" > /dev/null 2>&1 || return 1
  # Both should be present
  grep -q "Earlier work done" "${base}/roles/wren/journal/${today}.md" || { echo "earlier content lost"; return 1; }
  grep -q "second session paragraph" "${base}/roles/wren/journal/${today}.md" || { echo "new content missing"; return 1; }
  return 0
}

test_case "--dry-run does not modify files" test_dry_run_no_writes
test_case "real run writes next-session, activity, journal" test_writes_three_artifacts
test_case "rejects invalid role" test_invalid_role
test_case "requires non-empty paragraph" test_requires_paragraph
test_case "journal appends, doesn't overwrite" test_journal_appends

echo ""
echo "Results: ${pass} pass, ${fail} fail"
[ "$fail" -eq 0 ] || exit 1
