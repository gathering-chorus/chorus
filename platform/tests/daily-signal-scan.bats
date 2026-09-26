#!/usr/bin/env bats
# @test-type: integration — reads the live store
# @domain: analytics — the product domain this suite guards (#4334)
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

# #4274 — the shape that killed the scan on an empty section, shown to fail
# under the script's own `set -euo pipefail`, and the replacement shown to pass.
@test "NEGATIVE PROOF (#4274): '[ -n ] && echo' as a loop body ends the scan on an empty section" {
  run bash -c 'set -euo pipefail; echo "" | while read -r line; do [ -n "$line" ] && echo "  - $line"; done; echo reached'
  [ "$status" -ne 0 ]
  [[ "$output" != *reached* ]]
}

@test "#4274: the if-form prints nothing for an empty section and the scan continues" {
  run bash -c 'set -euo pipefail; echo "" | while read -r line; do if [ -n "$line" ]; then echo "  - $line"; fi; done; echo reached'
  [ "$status" -eq 0 ]
  [ "$output" = "reached" ]
}

# #4334 — an unreadable board ended the brief under `set -e`. The scan must
# finish and say the section was not measured.
@test "#4334: an unreadable board still finishes the brief and says flow health was not measured" {
  out="$BATS_TEST_TMPDIR/signal.md"
  run env DAILY_SIGNAL_CARDS=/usr/bin/false bash "$SCRIPT" --dry-run --output "$out"
  [ "$status" -eq 0 ]
  grep -q "flow health not measured" "$out"
}
