#!/usr/bin/env bats
# @test-type: unit — hermetic: parses the repo plist, drives the lock functions
# in a throwaway lock dir with a stubbed ops-nudge. No live service.
#
# 2026-09-08 (#4119): "hermetic" was not true. The stale-lock test read the REAL
# process table through nightly_live_runners, so on a box where a nightly run
# happens to be going (there were two live 03:00 lanes when this was found) the
# lock is correctly NOT stolen and the test goes red — reporting a defect in the
# lock when the only thing wrong was the box. NIGHTLY_PS is the seam the #4008
# work already built for exactly this; both lock tests now go through it.
#
# The `[[ ]]` assertions are also gone. Measured on bats-core 1.13.0 / this bash:
# a failing `[[ ]]` does NOT trip errexit, so an intermediate `[[ ]]` inside a
# @test is never graded — only the LAST command decides ok/not ok. Proof:
#   bash -c 'set -e; [[ a == b ]]; echo REACHED'   # prints REACHED, exits 0
# `grep -qF` is a simple command and does trip it.
# #4037 — Jeff wants a DAILY run: two calendar slots on ONE agent, and a slot
# that loses the single-flight lock to a live run must NUDGE, never vanish.

NIGHTLY="$BATS_TEST_DIRNAME/../scripts/nightly-suites.sh"
PLIST="$BATS_TEST_DIRNAME/../scripts/com.chorus.nightly-suites.plist"

has() { grep -qF -- "$2" <<<"$1"; }

# A process table with no nightly runner in it, and one with a runner mid-lane.
# Format is what nightly_live_runners parses: a header row, then pid ppid etime cmd.

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
  # NIGHTLY_PS is the seam #4008 built: a command that prints a `ps` table.
  # It has to be a real executable, not a string of shell — the script expands
  # $NIGHTLY_PS unquoted, so an inline `printf "a" "b"` word-splits into garbage.
  PS_NO_RUNNER="$TMP/ps-none"; PS_LIVE_RUNNER="$TMP/ps-runner"; export PS_NO_RUNNER PS_LIVE_RUNNER
  printf '#!/bin/bash\necho "  PID  PPID ELAPSED COMMAND"\necho "  111     1   00:10 /bin/sleep 1"\n' > "$PS_NO_RUNNER"
  printf '#!/bin/bash\necho "  PID  PPID ELAPSED COMMAND"\necho " 4242     1 01:52:00 /bin/bash werk-test --nightly"\n' > "$PS_LIVE_RUNNER"
  chmod +x "$PS_NO_RUNNER" "$PS_LIVE_RUNNER"
  export CHORUS_LOG_BIN="$TMP/chorus-log"
  printf '#!/bin/bash\nexit 0\n' > "$CHORUS_LOG_BIN"; chmod +x "$CHORUS_LOG_BIN"
}

# RETIRED REQUIREMENT, 2026-09-03. This test asserted two slots because that is
# what Jeff asked for on 08-31 (#4037). On 09-03 he had the schedule
# consolidated: 03:00 single slot, "was 06:00 + 13:30 — both gone", with the
# repo copies left saying the old hours as a named loose end. That loose end is
# what #4085 corrects. An assertion that outlives its decision is not a test,
# it is a second opinion nobody asked for — it would have reverted the schedule
# the next time someone made the suite green.
#
# Now asserts what is actually wanted, so a re-added slot fails loudly.
@test "the nightly agent carries exactly the 03:00 slot (#4064 consolidation)" {
  [ "$(count_slots "$PLIST")" -eq 1 ]
  run /usr/libexec/PlistBuddy -c "Print :StartCalendarInterval:0:Hour" "$PLIST"
  [ "$output" = "3" ]
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
  NIGHTLY_PS="$PS_NO_RUNNER" run bash -c "source '$NIGHTLY' --list-shell >/dev/null 2>&1
    if ! acquire_single_flight_lock; then refuse_single_flight; fi"
  grep -q "SKIPPED" "$TMP/nudged.txt"
  grep -q "$$" "$TMP/nudged.txt"
}

@test "a STALE lock (dead holder) is stolen silently — the run proceeds, no nudge" {
  mkdir -p "$NIGHTLY_LOCKDIR"; echo 4999999 > "$NIGHTLY_LOCKDIR/pid"  # dead pid
  NIGHTLY_PS="$PS_NO_RUNNER" run bash -c "source '$NIGHTLY' --list-shell >/dev/null 2>&1
    if acquire_single_flight_lock; then echo STOLE; else refuse_single_flight; fi"
  has "$output" "STOLE"
  [ ! -s "$TMP/nudged.txt" ]
}

# Negative proof for the one above: same dead holder, same lock — the only thing
# that changes is that the process table still shows a nightly runner mid-lane.
# If the lock could not tell those two states apart it would steal here too, and
# we would be back to 2026-08-25: two lanes running beside each other for 1h52m.
@test "negative proof: a dead holder whose RUNNER is still alive is NOT stolen" {
  mkdir -p "$NIGHTLY_LOCKDIR"; echo 4999999 > "$NIGHTLY_LOCKDIR/pid"  # dead pid
  NIGHTLY_PS="$PS_LIVE_RUNNER" run bash -c "source '$NIGHTLY' --list-shell >/dev/null 2>&1
    if acquire_single_flight_lock; then echo STOLE; else refuse_single_flight; fi"
  ! grep -q "STOLE" <<<"$output"
  grep -q "runner pid 4242 is alive" "$TMP/nudged.txt"
}
