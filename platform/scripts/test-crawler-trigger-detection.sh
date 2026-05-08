#!/usr/bin/env bash
# test-crawler-trigger-detection.sh — #2817 unit tests for the file-watch-vs-polling
# trigger heuristic in index-crawler-snapshots.sh.
#
# The heuristic: if any file under the watched dirs is newer than the
# last-run timestamp file, this invocation is file-event-driven; else it's
# the 60s polling tick. The script writes /tmp/crawler-last-run-ts at
# every run; reads it at next run.
#
# Tests:
#   1. First-run-ever: no last-run file → trigger=polling (default).
#   2. No filesystem changes since last run → trigger=polling.
#   3. New file under a watched dir since last run → trigger=file-watch.
#   4. Modified file under a watched dir since last run → trigger=file-watch.
#   5. New file in node_modules/target/dist → does NOT flip to file-watch
#      (excluded paths shouldn't trigger).
#
# We exercise the trigger-detection block in isolation, not the full crawler
# script (which hits the live API + writes to index.db). The block is
# self-contained at lines 22-39 of index-crawler-snapshots.sh — extract
# the heuristic logic and run it with controlled fixtures.

set -uo pipefail

PASS=0
FAIL=0
p() { PASS=$((PASS+1)); echo "  PASS: $*"; }
f() { FAIL=$((FAIL+1)); echo "  FAIL: $*"; }

# Extract the trigger-detection logic into a function we can test against
# a controlled CHORUS_ROOT fixture.
detect_trigger() {
  local CHORUS_ROOT="$1"
  local LAST_RUN_FILE="$2"
  local TRIGGER="polling"
  if [ -f "$LAST_RUN_FILE" ]; then
    for watched in "$CHORUS_ROOT/designing" "$CHORUS_ROOT/platform" "$CHORUS_ROOT/roles" "$CHORUS_ROOT/skills" "$CHORUS_ROOT/directing" "$CHORUS_ROOT/proving"; do
      [ -d "$watched" ] || continue
      newest=$(find "$watched" -type f -newer "$LAST_RUN_FILE" -not -path "*/node_modules/*" -not -path "*/target/*" -not -path "*/dist/*" 2>/dev/null | head -1)
      if [ -n "$newest" ]; then TRIGGER="file-watch"; break; fi
    done
  fi
  echo "$TRIGGER"
}

new_fixture() {
  FIXTURE=$(mktemp -d -t crawler-trigger-test.XXXX)
  mkdir -p "$FIXTURE/designing" "$FIXTURE/platform" "$FIXTURE/roles" "$FIXTURE/skills" "$FIXTURE/directing" "$FIXTURE/proving"
  LAST_RUN="$FIXTURE/.last-run"
}

cleanup() {
  [ -n "${FIXTURE:-}" ] && [ -d "$FIXTURE" ] && rm -rf "$FIXTURE"
}
trap cleanup EXIT

echo "=== #2817 trigger detection tests ==="

# --- Test 1: first run ever (no last-run file) → polling ---
echo "Test 1: first-run defaults to polling"
new_fixture
result=$(detect_trigger "$FIXTURE" "$LAST_RUN")
[ "$result" = "polling" ] && p "no last-run file → trigger=polling" || f "expected polling, got: $result"
cleanup

# --- Test 2: no changes since last run → polling ---
echo "Test 2: no fs changes since last-run → polling"
new_fixture
echo "existing" > "$FIXTURE/platform/foo.sh"
sleep 1
touch "$LAST_RUN"
result=$(detect_trigger "$FIXTURE" "$LAST_RUN")
[ "$result" = "polling" ] && p "no changes after last-run → polling" || f "expected polling, got: $result"
cleanup

# --- Test 3: new file under a watched dir → file-watch ---
echo "Test 3: new file in watched dir → file-watch"
new_fixture
touch "$LAST_RUN"
sleep 1
echo "new" > "$FIXTURE/platform/new.sh"
result=$(detect_trigger "$FIXTURE" "$LAST_RUN")
[ "$result" = "file-watch" ] && p "new file post-last-run → file-watch" || f "expected file-watch, got: $result"
cleanup

# --- Test 4: modified file → file-watch ---
echo "Test 4: modified file in watched dir → file-watch"
new_fixture
echo "v1" > "$FIXTURE/roles/foo.md"
touch "$LAST_RUN"
sleep 1
echo "v2" > "$FIXTURE/roles/foo.md"
result=$(detect_trigger "$FIXTURE" "$LAST_RUN")
[ "$result" = "file-watch" ] && p "modified file post-last-run → file-watch" || f "expected file-watch, got: $result"
cleanup

# --- Test 5: new file under node_modules/target/dist → ignored ---
echo "Test 5: excluded paths do not flip to file-watch"
new_fixture
mkdir -p "$FIXTURE/platform/node_modules" "$FIXTURE/roles/target" "$FIXTURE/skills/dist"
touch "$LAST_RUN"
sleep 1
echo "vendor" > "$FIXTURE/platform/node_modules/foo.js"
echo "build" > "$FIXTURE/roles/target/bar.o"
echo "dist" > "$FIXTURE/skills/dist/baz.js"
result=$(detect_trigger "$FIXTURE" "$LAST_RUN")
[ "$result" = "polling" ] && p "excluded-path changes ignored → polling" || f "expected polling, got: $result"
cleanup

echo ""
echo "=== Results: $PASS passed, $FAIL failed ==="
[ "$FAIL" -eq 0 ]
