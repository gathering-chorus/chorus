#!/usr/bin/env bats
# @test-type: integration — runs the real scan end to end. It reads the live
# repo (git log, jest --listTests, eslint) and the board through the cards CLI,
# so it is not hermetic and it is not fast: six runs of ~35s each. That is
# deliberate — the defects it caught on #4085 (dies against a werk root,
# reports success while writing nothing) are only visible when the whole script
# runs against a real tree.
# Tests for daily-signal-scan.sh (#2088)
# What Jeff sees: a brief ready by 6am with codebase weather, trust verification,
# backlog coherence, and golfball detection. No session required.

# #3369: resolve relative to this repo (werk or canonical), not a hardcoded
# canonical path — the old pin made in-werk runs test the UNFIXED main copy.
SCRIPT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../scripts" && pwd)/daily-signal-scan.sh"

# #4085 — every test wrote the same fixed /tmp path, and each one
# removed it at the end. Two runs of this suite at once (a nightly and a werk
# pipeline, which is the normal case here) share that path: one deletes the
# file the other is still asserting on, and the loser reports a red that has
# nothing to do with the scan. A test brings its own world.
OUT="${BATS_TEST_TMPDIR:-${TMPDIR:-/tmp}}/daily-signal.md"

@test "AC8: script exists and is executable" {
  [ -x "$SCRIPT" ]
}

@test "AC8: produces output file" {
  run bash "$SCRIPT" --dry-run --output "$OUT"
  [ "$status" -eq 0 ]
  [ -f "$OUT" ]
  rm -f "$OUT"
}

@test "AC1: output includes codebase weather section" {
  bash "$SCRIPT" --dry-run --output "$OUT" 2>/dev/null
  grep -q "Codebase Weather\|codebase weather\|Test.*trend\|Lint.*trend" "$OUT"
  rm -f "$OUT"
}

@test "AC2: output includes trust verification section" {
  bash "$SCRIPT" --dry-run --output "$OUT" 2>/dev/null
  grep -q "Trust\|Gate\|hook" "$OUT"
  rm -f "$OUT"
}

@test "AC4: output includes doc freshness section" {
  bash "$SCRIPT" --dry-run --output "$OUT" 2>/dev/null
  grep -q "Doc\|freshness\|decisions.md\|projects.md" "$OUT"
  rm -f "$OUT"
}

@test "AC5: output includes flow health section" {
  bash "$SCRIPT" --dry-run --output "$OUT" 2>/dev/null
  grep -q "Flow Health\|WIP\|Now queue" "$OUT"
  rm -f "$OUT"
}
