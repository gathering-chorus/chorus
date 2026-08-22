#!/usr/bin/env bash
# athena-make-launch-readiness.test.sh — #3446 AC1 (red-first, DEC-1674)
#
# The athena-make launcher must WAIT for Fuseki to be ready before exec'ing athena-make,
# instead of racing ahead at boot, failing the first fuseki-query, and crash-looping
# under KeepAlive. We prove this by running the launcher in a sandbox with a stubbed
# curl that fails the first two readiness probes then succeeds, and a fake athena-make
# that drops a marker. The launcher must (a) retry past the early failures and
# (b) still exec athena-make once Fuseki is "ready".
#
# Run: bash platform/tests/athena-make-launch-readiness.test.sh

set -uo pipefail

PASS=0
FAIL=0
test_pass() { echo "  PASS: $1"; ((PASS++)); }
test_fail() { echo "  FAIL: $1"; ((FAIL++)); }

echo "=== athena-make launch readiness gate (#3446 AC1) ==="

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
LAUNCHER="$REPO_ROOT/platform/services/athena-make/athena-make-launch.sh"

if [ ! -f "$LAUNCHER" ]; then
  test_fail "canonical launcher source missing at platform/services/athena-make/athena-make-launch.sh"
  echo ""
  echo "=== Results: $PASS passed, $FAIL failed ==="
  exit 1
fi

# --- sandbox ---
SBX="$(mktemp -d)"
trap 'rm -rf "$SBX"' EXIT
export HOME="$SBX/home"
mkdir -p "$HOME/.chorus/bin"

MARKER="$SBX/athena-make-ran"
COUNTER="$SBX/curl-calls"
echo 0 > "$COUNTER"

# fake athena-make: the thing the launcher should exec once Fuseki is ready
cat > "$HOME/.chorus/bin/athena-make" <<EOF
#!/usr/bin/env bash
echo "fake-athena-make started" > "$MARKER"
exit 0
EOF
chmod +x "$HOME/.chorus/bin/athena-make"

# stub curl: fail the first 2 readiness probes, then succeed (Fuseki "comes up")
mkdir -p "$SBX/stubbin"
cat > "$SBX/stubbin/curl" <<EOF
#!/usr/bin/env bash
n=\$(cat "$COUNTER")
n=\$((n + 1))
echo \$n > "$COUNTER"
if [ "\$n" -lt 3 ]; then exit 1; fi
exit 0
EOF
chmod +x "$SBX/stubbin/curl"
# stub sleep so the test doesn't actually wait
cat > "$SBX/stubbin/sleep" <<'EOF'
#!/usr/bin/env bash
exit 0
EOF
chmod +x "$SBX/stubbin/sleep"

export CHORUS_FUSEKI="http://localhost:3030/pods"
# No `timeout` dependency (absent on macOS) — stubbed sleep + curl-succeeds-on-3rd-call
# guarantee the readiness loop terminates fast.
PATH="$SBX/stubbin:$PATH" bash "$LAUNCHER" >/dev/null 2>&1
rc=$?

calls=$(cat "$COUNTER")

# AC1a: launcher retried past the early failures (didn't proceed on first probe)
if [ "$calls" -ge 3 ]; then
  test_pass "launcher polled Fuseki until ready (curl called $calls times, waited past failures)"
else
  test_fail "launcher did not wait for Fuseki (curl called only $calls times — proceeded too early)"
fi

# AC1b: launcher still exec'd athena-make once Fuseki was ready
if [ -f "$MARKER" ]; then
  test_pass "launcher exec'd athena-make after Fuseki became ready"
else
  test_fail "launcher never exec'd athena-make (rc=$rc)"
fi

# AC1c: graceful fallback — if Fuseki NEVER comes up, the launcher must still exec
# athena-make after the bounded wait (never hang boot). curl always fails; stubbed sleep
# keeps it fast. (cold-eyes #3446: the timeout path was previously untested.)
rm -f "$MARKER"
cat > "$SBX/stubbin/curl" <<'EOF'
#!/usr/bin/env bash
exit 1
EOF
chmod +x "$SBX/stubbin/curl"
PATH="$SBX/stubbin:$PATH" bash "$LAUNCHER" >/dev/null 2>&1
if [ -f "$MARKER" ]; then
  test_pass "launcher exec'd athena-make anyway after the bounded wait (no boot hang when Fuseki never readies)"
else
  test_fail "launcher hung / never exec'd athena-make when Fuseki stayed down"
fi

# AC: the installed boot path must invoke the launcher, not athena-make directly —
# otherwise the readiness gate is dead code (product-gate catch #3446). Guard the
# repo plist source against drifting back to a direct athena-make invocation.
PLIST="$REPO_ROOT/config/launchagents/com.chorus.athena-make.plist"
if [ -f "$PLIST" ] && grep -q "athena-make-launch.sh" "$PLIST"; then
  test_pass "athena-make launchd plist invokes athena-make-launch.sh (readiness gate is on the boot path)"
else
  test_fail "athena-make launchd plist does NOT invoke athena-make-launch.sh — readiness gate bypassed on deploy"
fi

echo ""
echo "=== Results: $PASS passed, $FAIL failed ==="
[ "$FAIL" -eq 0 ]
