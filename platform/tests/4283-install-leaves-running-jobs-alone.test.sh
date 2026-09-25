#!/bin/bash
# @test-type: unit — the installer against a fake launchctl; no live agent is touched
# #4283 — chorus-bin-install must never boot out a job that is running or
# running (and, since #4292, reloads an idle scheduled one). On 2026-09-23 14:07 it booted out com.chorus.nightly-suites
# mid-run and the bootstrap back failed. Three fixtures, one fake launchctl
# that records every call: a running job and a scheduled job are left alone
# (NEGATIVE PROOFS: no bootout call), an idle kept-alive job is reloaded
# (control: the loop still works where it should).
set -u
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
INSTALL="$ROOT/platform/scripts/chorus-bin-install"
T="$(mktemp -d)"; trap 'rm -rf "$T"' EXIT
export HOME="$T/home"; mkdir -p "$HOME/.chorus/bin" "$T/agents" "$T/fakebin"
export CHORUS_BIN_SPINE_LOG="$T/spine.log"
export CHORUS_BIN_AGENT_DIR="$T/agents"
export CHORUS_BIN_SKIP_SMOKE=1
DEST="$HOME/.chorus/bin/chorus-frob"
printf '#!/bin/sh\nexit 0\n' > "$T/candidate"; chmod +x "$T/candidate"

# the fake launchctl: `print` answers from FAKE_PIDS ("label=pid label2=" ; empty = loaded, not running)
cat > "$T/fakebin/launchctl" <<'FAKE'
#!/bin/bash
echo "$*" >> "$FAKE_CALLS"
case "$1" in
  print)
    label="${2##*/}"
    for kv in $FAKE_PIDS; do
      if [ "${kv%%=*}" = "$label" ]; then
        pid="${kv#*=}"
        echo "$label = {"; [ -n "$pid" ] && echo "	pid = $pid"; echo "	state = ${pid:+running}${pid:-not running}"; echo "}"
        exit 0
      fi
    done
    exit 113 ;;
  bootout|bootstrap|kickstart) exit 0 ;;
esac
exit 0
FAKE
chmod +x "$T/fakebin/launchctl"
export PATH="$T/fakebin:$PATH" FAKE_CALLS="$T/calls.log"
# cases 1-3 declare this temp home to be the production home (the proof seam); case 4 does not
export CHORUS_BIN_REAL_HOME="$HOME"

plist() { # label, extra-xml
  cat > "$T/agents/$1.plist" <<PLIST
<plist><dict><key>Label</key><string>$1</string>
<key>ProgramArguments</key><array><string>$DEST</string><string>--serve</string></array>
$2
</dict></plist>
PLIST
}
pass=0; fail=0
check() { if eval "$2"; then echo "  PASS: $1"; pass=$((pass+1)); else echo "  FAIL: $1"; fail=$((fail+1)); fi; }

# 1. a RUNNING job is left alone
rm -f "$T/agents"/*.plist "$FAKE_CALLS" "$CHORUS_BIN_SPINE_LOG"; plist com.chorus.nightly-x ""
out="$(FAKE_PIDS="com.chorus.nightly-x=4242" bash "$INSTALL" "$T/candidate" chorus-frob 2>&1)"
check "running job: no bootout call"        '! grep -q "^bootout .*com.chorus.nightly-x" "$FAKE_CALLS"'
check "running job: named as left alone"    'echo "$out" | grep -q "com.chorus.nightly-x .* is running (pid 4242) — left alone"'
check "running job: spine says skipped"     'grep -q "unit=com.chorus.nightly-x result=skipped-running" "$CHORUS_BIN_SPINE_LOG"'

# 2. #4292 NEGATIVE PROOF: an IDLE scheduled job IS reloaded. Left alone, launchd
#    kept the old signature and killed the 2026-09-25 03:00 nightly at spawn
#    (OS_REASON_CODESIGNING). Reverting to "skip scheduled" turns these red.
rm -f "$T/agents"/*.plist "$FAKE_CALLS" "$CHORUS_BIN_SPINE_LOG"; plist com.chorus.nightly-y "<key>StartCalendarInterval</key><dict><key>Hour</key><integer>3</integer></dict>"
out="$(FAKE_PIDS="com.chorus.nightly-y=" bash "$INSTALL" "$T/candidate" chorus-frob 2>&1)"
check "idle scheduled job: bootout called"   'grep -q "^bootout .*com.chorus.nightly-y" "$FAKE_CALLS"'
check "idle scheduled job: bootstrap called" 'grep -q "^bootstrap .*com.chorus.nightly-y.plist" "$FAKE_CALLS"'
check "idle scheduled job: spine says ok"    'grep -q "unit=com.chorus.nightly-y result=ok" "$CHORUS_BIN_SPINE_LOG"'
# 2b. a scheduled job that is RUNNING is still left alone
rm -f "$T/agents"/*.plist "$FAKE_CALLS" "$CHORUS_BIN_SPINE_LOG"; plist com.chorus.nightly-y "<key>StartCalendarInterval</key><dict><key>Hour</key><integer>3</integer></dict>"
out="$(FAKE_PIDS="com.chorus.nightly-y=5151" bash "$INSTALL" "$T/candidate" chorus-frob 2>&1)"
check "running scheduled job: no bootout"    '! grep -q "^bootout .*com.chorus.nightly-y" "$FAKE_CALLS"'

# 3. CONTROL: an idle kept-alive job is still reloaded (bootout + bootstrap)
rm -f "$T/agents"/*.plist "$FAKE_CALLS" "$CHORUS_BIN_SPINE_LOG"; plist com.chorus.daemon-z "<key>KeepAlive</key><true/>"
out="$(FAKE_PIDS="com.chorus.daemon-z=" bash "$INSTALL" "$T/candidate" chorus-frob 2>&1)"
check "idle daemon: bootout called"         'grep -q "^bootout .*com.chorus.daemon-z" "$FAKE_CALLS"'
check "idle daemon: bootstrap called"       'grep -q "^bootstrap .*com.chorus.daemon-z.plist" "$FAKE_CALLS"'
check "idle daemon: spine says reloaded ok" 'grep -q "unit=com.chorus.daemon-z result=ok" "$CHORUS_BIN_SPINE_LOG"'

# 4. NEGATIVE PROOF for the nightly's own kills: a FIXTURE home (not the production
#    home) installs its binary and never touches launchd — no bootout, no kick —
#    even though the production unit com.chorus.frob is loaded and idle.
rm -f "$T/agents"/*.plist "$FAKE_CALLS" "$CHORUS_BIN_SPINE_LOG"; plist com.chorus.daemon-z "<key>KeepAlive</key><true/>"
out="$(CHORUS_BIN_REAL_HOME="/Users/someone-else" FAKE_PIDS="com.chorus.daemon-z= com.chorus.frob=" bash "$INSTALL" "$T/candidate" chorus-frob 2>&1)"
check "fixture home: binary still installed"      '[ -x "$DEST" ]'
check "fixture home: no bootout call"             '! grep -q "^bootout" "$FAKE_CALLS" 2>/dev/null'
check "fixture home: no kickstart call"           '! grep -q "^kickstart" "$FAKE_CALLS" 2>/dev/null'
check "fixture home: says launchd left untouched" 'echo "$out" | grep -q "fixture home .* launchd is left untouched"'

# 5. the invariance suite builds without installing (the other nightly killer)
check "test-build-invariance skips the install" 'grep -q "BUILD_SKIP_INSTALL=1 bash platform/scripts/build-signed.sh" "$ROOT/platform/scripts/test-build-invariance.sh"'

echo "$pass pass, $fail fail"
[ "$fail" -eq 0 ]
