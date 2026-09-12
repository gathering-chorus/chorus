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

BIN="${WERK_TEST_BIN:-$BATS_TEST_DIRNAME/../services/werk-test/target/release/werk-test}"
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

# #4148 (Jeff, 2026-09-12): "3am only + our current work on demand". The two
# daytime slots #4037 added are gone; one scheduled run at 03:00.
@test "the agent carries ONE scheduled slot, 03:00 (#4148)" {
  [ "$(count_slots "$PLIST")" -eq 1 ]
  python3 - "$PLIST" <<'PY'
import plistlib,sys
d=plistlib.load(open(sys.argv[1],'rb'))['StartCalendarInterval']
d=d[0] if isinstance(d,list) else d
assert d.get('Hour')==3 and d.get('Minute',0)==0, d
PY
}

@test "negative proof: the check separates its states — a two-slot plist reads 2, not 1" {
  cat > "$TMP/two.plist" <<'P'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>Label</key><string>x</string>
<key>StartCalendarInterval</key><array><dict><key>Hour</key><integer>6</integer></dict><dict><key>Hour</key><integer>13</integer></dict></array>
</dict></plist>
P
  [ "$(count_slots "$TMP/two.plist")" -eq 2 ]
}

@test "negative proof: a single-slot plist reads 1, not 2" {
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

# #4145 — the runner owns the lock. RUNALL drives `werk-test-bin --nightly
# --run-all` with every outside thing stubbed: no legs, a runner that prints one
# passing unit, a dead registry port, the nudge stub above.
runall() {
  printf '#!/bin/bash\necho "nightly-unit|bats|platform/tests/x.bats|pass|1 pass, 0 fail"\n' > "$TMP/runner.sh"; chmod +x "$TMP/runner.sh"
  mkdir -p "$TMP/root"
  CHORUS_ROOT="$TMP/root" CHORUS_HOME="$TMP/root" NIGHTLY_LOG_PATH="$TMP/run.log" NIGHTLY_FAIL_DIR="$TMP/fail" \
  OWLAPI=http://127.0.0.1:9 NIGHTLY_API=http://127.0.0.1:9 NIGHTLY_RUNNER_CMD="$TMP/runner.sh" NIGHTLY_LEGS_NOOP=1 \
  NIGHTLY_LOAD_MAX_PER_CORE=99 "$BIN" --nightly --run-all
}

@test "a LIVE lock holder makes the slot refuse LOUDLY: refusal line + ops-nudge" {
  mkdir -p "$NIGHTLY_LOCKDIR"; echo $$ > "$NIGHTLY_LOCKDIR/pid"   # us: alive
  NIGHTLY_PS="$PS_NO_RUNNER" run runall
  [ "$status" -eq 0 ]
  has "$output" "REFUSED"
  grep -q "SKIPPED" "$TMP/nudged.txt"
  grep -q "$$" "$TMP/nudged.txt"
  [ ! -f "$TMP/run.log" ]
}

@test "a STALE lock (dead holder) is stolen silently — the run proceeds, no refusal nudge" {
  mkdir -p "$NIGHTLY_LOCKDIR"; echo 4999999 > "$NIGHTLY_LOCKDIR/pid"  # dead pid
  NIGHTLY_PS="$PS_NO_RUNNER" run runall
  [ "$status" -eq 0 ]
  grep -q '^RUN|complete|' "$TMP/run.log"
  ! grep -q "SKIPPED" "$TMP/nudged.txt" 2>/dev/null
}

# Negative proof for the one above: same dead holder, same lock — the only thing
# that changes is that the process table still shows a nightly runner mid-lane.
# If the lock could not tell those two states apart it would steal here too, and
# we would be back to 2026-08-25: two lanes running beside each other for 1h52m.
@test "negative proof: a dead holder whose RUNNER is still alive is NOT stolen" {
  mkdir -p "$NIGHTLY_LOCKDIR"; echo 4999999 > "$NIGHTLY_LOCKDIR/pid"  # dead pid
  NIGHTLY_PS="$PS_LIVE_RUNNER" run runall
  [ ! -f "$TMP/run.log" ]
  grep -q "runner pid 4242 is alive" "$TMP/nudged.txt"
}
