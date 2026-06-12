#!/usr/bin/env bats
# Tests for daily-signal-scan.sh (#2088)
# What Jeff sees: a brief ready by 6am with codebase weather, trust verification,
# backlog coherence, and golfball detection. No session required.

# #3369: resolve relative to this repo (werk or canonical), not a hardcoded
# canonical path — the old pin made in-werk runs test the UNFIXED main copy.
SCRIPT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../scripts" && pwd)/daily-signal-scan.sh"

@test "AC8: script exists and is executable" {
  [ -x "$SCRIPT" ]
}

@test "AC8: produces output file" {
  run bash "$SCRIPT" --dry-run --output /tmp/test-daily-signal.md
  [ "$status" -eq 0 ]
  [ -f /tmp/test-daily-signal.md ]
  rm -f /tmp/test-daily-signal.md
}

@test "AC1: output includes codebase weather section" {
  bash "$SCRIPT" --dry-run --output /tmp/test-daily-signal.md 2>/dev/null
  grep -q "Codebase Weather\|codebase weather\|Test.*trend\|Lint.*trend" /tmp/test-daily-signal.md
  rm -f /tmp/test-daily-signal.md
}

@test "AC2: output includes trust verification section" {
  bash "$SCRIPT" --dry-run --output /tmp/test-daily-signal.md 2>/dev/null
  grep -q "Trust\|Gate\|hook" /tmp/test-daily-signal.md
  rm -f /tmp/test-daily-signal.md
}

@test "AC4: output includes doc freshness section" {
  bash "$SCRIPT" --dry-run --output /tmp/test-daily-signal.md 2>/dev/null
  grep -q "Doc\|freshness\|decisions.md\|projects.md" /tmp/test-daily-signal.md
  rm -f /tmp/test-daily-signal.md
}

@test "AC5: output includes flow health section" {
  bash "$SCRIPT" --dry-run --output /tmp/test-daily-signal.md 2>/dev/null
  grep -q "Flow Health\|WIP\|Now queue" /tmp/test-daily-signal.md
  rm -f /tmp/test-daily-signal.md
}
