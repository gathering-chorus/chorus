#!/bin/bash
# @test-type: unit — chorus-bin-install against a fake launchctl that counts restarts; no live agent is touched
# @domain: pipelines
#
# #4380 — one deploy restarts com.chorus.hooks once. werk-deploy installs every
# binary of the crate (chorus-hooks and chorus-hook-shim) and then restarts the
# service itself; the installer, told the caller restarts, must not restart it
# per binary. On 2026-09-28 08:32Z the daemon logged "Shutting down... SIGTERM"
# twice in one second: once from the installer, once from werk-deploy.
set -u
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
INSTALL="$ROOT/platform/scripts/chorus-bin-install"
T="$(mktemp -d)"; trap 'rm -rf "$T"' EXIT
export HOME="$T/home"; mkdir -p "$HOME/.chorus/bin" "$T/agents" "$T/fakebin"
export CHORUS_BIN_SPINE_LOG="$T/spine.log" CHORUS_BIN_AGENT_DIR="$T/agents" CHORUS_BIN_SKIP_SMOKE=1 CHORUS_BIN_SKIP_SIGCHECK=1
export CHORUS_BIN_REAL_HOME="$HOME"
printf '#!/bin/sh\nexit 0\n' > "$T/candidate"; chmod +x "$T/candidate"
cat > "$T/fakebin/launchctl" <<'FAKE'
#!/bin/bash
echo "$*" >> "$FAKE_CALLS"
case "$1" in print) [ "${2##*/}" = "com.chorus.hooks" ] && { echo "pid = 1"; exit 0; }; exit 113 ;; esac
exit 0
FAKE
chmod +x "$T/fakebin/launchctl"
export PATH="$T/fakebin:$PATH" FAKE_CALLS="$T/calls.log"
restarts() { grep -c "^kickstart .*com.chorus.hooks" "$FAKE_CALLS" 2>/dev/null || true; }
pass=0; fail=0
check() { if [ "$2" = "$3" ]; then echo "  PASS: $1"; pass=$((pass+1)); else echo "  FAIL: $1 (got $2, want $3)"; fail=$((fail+1)); fi; }

# werk-deploy's call: both binaries installed with the caller-restarts flag, then its own one restart
: > "$FAKE_CALLS"
CHORUS_BIN_CALLER_RESTARTS=1 bash "$INSTALL" --target canonical "$T/candidate" chorus-hooks >/dev/null 2>&1
CHORUS_BIN_CALLER_RESTARTS=1 bash "$INSTALL" --target canonical "$T/candidate" chorus-hook-shim >/dev/null 2>&1
check "the installer restarts nothing when the caller restarts" "$(restarts)" 0
launchctl kickstart -k "gui/$(id -u)/com.chorus.hooks"   # werk-deploy's single restart
check "one deploy of both binaries = one restart" "$(restarts)" 1

# the shim alone never restarts the daemon
: > "$FAKE_CALLS"
bash "$INSTALL" --target canonical "$T/candidate" chorus-hook-shim >/dev/null 2>&1
check "installing the shim alone restarts nothing" "$(restarts)" 0

# NEGATIVE PROOF — the pre-#4380 call (no flag) plus werk-deploy's own restart = two
: > "$FAKE_CALLS"
bash "$INSTALL" --target canonical "$T/candidate" chorus-hooks >/dev/null 2>&1
launchctl kickstart -k "gui/$(id -u)/com.chorus.hooks"
check "NEGATIVE: without the flag one deploy restarts the daemon twice" "$(restarts)" 2

# and werk-deploy really passes the flag
if grep -q '("CHORUS_BIN_CALLER_RESTARTS", "1")' "$ROOT/platform/services/werk-deploy/src/lib.rs"; then echo "  PASS: werk-deploy tells the installer it restarts"; pass=$((pass+1)); else echo "  FAIL: werk-deploy does not pass CHORUS_BIN_CALLER_RESTARTS"; fail=$((fail+1)); fi
echo "=== Results: $pass passed, $fail failed ==="
[ "$fail" -eq 0 ]
