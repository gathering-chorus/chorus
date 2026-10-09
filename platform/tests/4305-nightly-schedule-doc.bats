#!/usr/bin/env bats
# @domain: tests
# @test-type: unit
# #4305 — CLAUDE.md said the nightly ran at 06:00 and 13:30; launchd runs it
# once, at 03:00. Every role loads CLAUDE.md, so the wrong times reached Jeff
# twice (2026-09-25, 2026-10-08). The plist is the source; the doc must match.
# Covers: CLAUDE.md
# Covers: platform/scripts/com.chorus.nightly-suites.plist

setup() {
  ROOT="${BATS_TEST_DIRNAME}/../.."
  PLIST="$ROOT/platform/scripts/com.chorus.nightly-suites.plist"
}

# HH:MM for every StartCalendarInterval slot in the plist, one per line.
plist_slots() {
  python3 - "$1" <<'EOF'
import plistlib, sys
iv = plistlib.load(open(sys.argv[1], 'rb')).get('StartCalendarInterval', [])
for d in (iv if isinstance(iv, list) else [iv]):
    print(f"{d.get('Hour', 0):02d}:{d.get('Minute', 0):02d}")
EOF
}

# The nightly sentence in a CLAUDE.md: every HH:MM it names must be a plist slot.
# Prints the offending times; non-zero when there are any.
doc_names_only_plist_slots() {
  local doc="$1" slots line bad=""
  slots=$(plist_slots "$PLIST")
  [ -n "$slots" ] || { echo "plist has no slots"; return 1; }
  line=$(grep -E 'red-`main` detector is the daily nightly' "$doc")
  [ -n "$line" ] || { echo "nightly sentence not found"; return 1; }
  for t in $(printf '%s\n' "$line" | grep -oE '[0-9]{2}:[0-9]{2}'); do
    printf '%s\n' "$slots" | grep -qx "$t" || bad="$bad $t"
  done
  [ -z "$bad" ] || { echo "not in plist:$bad"; return 1; }
}

@test "the plist schedules the nightly once, at 03:00" {
  run plist_slots "$PLIST"
  [ "$status" -eq 0 ]
  [ "$output" = "03:00" ]
}

@test "CLAUDE.md names only times the plist schedules" {
  run doc_names_only_plist_slots "$ROOT/CLAUDE.md"
  [ "$status" -eq 0 ]
}

@test "NEGATIVE PROOF: the old 06:00 and 13:30 line goes red" {
  old="$BATS_TEST_TMPDIR/CLAUDE.md"
  printf '%s\n' '**The red-`main` detector is the daily nightly** (`werk-test --nightly --run-all`, launchd slots 06:00 and 13:30; the bash wrapper was deleted by #4145), not a per-PR check.' > "$old"
  run doc_names_only_plist_slots "$old"
  [ "$status" -eq 1 ]
  [ "$output" = "not in plist: 06:00 13:30" ]
}

@test "NEGATIVE PROOF: a doc with the nightly sentence deleted fails, never passes vacuously" {
  empty="$BATS_TEST_TMPDIR/empty.md"
  printf '# nothing here\n' > "$empty"
  run doc_names_only_plist_slots "$empty"
  [ "$status" -eq 1 ]
}
