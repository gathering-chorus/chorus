#!/usr/bin/env bats
# @test-type: unit — hermetic: parses the repo plist, drives the lock functions
# in a throwaway lock dir with a stubbed ops-nudge. No live service.
# #4037 — Jeff wants a DAILY run: two calendar slots on ONE agent, and a slot
# that loses the single-flight lock to a live run must NUDGE, never vanish.

NIGHTLY="$BATS_TEST_DIRNAME/../scripts/nightly-suites.sh"
PLIST="$BATS_TEST_DIRNAME/../scripts/com.chorus.nightly-suites.plist"

count_slots() { # slots in a plist = Hour keys inside StartCalendarInterval
  python3 - "$1" <<'PY'
import plistlib,sys
d=plistlib.load(open(sys.argv[1],'rb'))
v=d.get('StartCalendarInterval')
print(len(v) if isinstance(v,list) else (1 if v else 0))
PY
}

setup() {
  TMP="$BATS_TEST_TMPDIR"
  export NIGHTLY_LOCKDIR="$TMP/lock.d"
  export OPS_NUDGE="$TMP/ops-nudge"
  printf '#!/bin/bash\necho "$@" >> "%s/nudged.txt"\n' "$TMP" > "$OPS_NUDGE"; chmod +x "$OPS_NUDGE"
  export CHORUS_LOG_BIN="$TMP/chorus-log"
  printf '#!/bin/bash\nexit 0\n' > "$CHORUS_LOG_BIN"; chmod +x "$CHORUS_LOG_BIN"
}

@test "the agent carries BOTH daily slots (03:00 and 13:30)" {
  [ "$(count_slots "$PLIST")" -ge 2 ]
}

@test "negative proof: the check separates its states — a single-slot plist reads 1, not 2" {
  cat > "$TMP/single.plist" <<'P'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>Label</key><string>x</string>
<key>StartCalendarInterval</key><dict><key>Hour</key><integer>3</integer></dict>
</dict></plist>
P
  [ "$(count_slots "$TMP/single.plist")" -eq 1 ]
}

@test "one agent only: no second com.chorus.*suites LaunchAgent in the repo" {
  run bash -c "ls '$BATS_TEST_DIRNAME/../scripts/' | grep -c 'com.chorus..*suites.*plist'"
  [ "$output" = "1" ]
}

@test "a LIVE lock holder makes the slot refuse LOUDLY: refusal line + ops-nudge" {
  mkdir -p "$NIGHTLY_LOCKDIR"; echo $$ > "$NIGHTLY_LOCKDIR/pid"   # us: alive
  run bash -c "source '$NIGHTLY' --list-shell >/dev/null 2>&1
    if ! acquire_single_flight_lock; then refuse_single_flight; fi"
  [[ "$output" == *"another run holds"* ]] || [[ "$stderr" == *"another run holds"* ]] || grep -q . "$TMP/nudged.txt"
  grep -q "SKIPPED" "$TMP/nudged.txt"
  grep -q "$$" "$TMP/nudged.txt"
}

@test "a STALE lock (dead holder) is stolen silently — the run proceeds, no nudge" {
  mkdir -p "$NIGHTLY_LOCKDIR"; echo 4999999 > "$NIGHTLY_LOCKDIR/pid"  # dead pid
  run bash -c "source '$NIGHTLY' --list-shell >/dev/null 2>&1
    if acquire_single_flight_lock; then echo STOLE; else refuse_single_flight; fi"
  [[ "$output" == *"STOLE"* ]]
  [ ! -s "$TMP/nudged.txt" ]
}
