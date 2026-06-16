#!/usr/bin/env bash
# owl-api-launch-readiness.test.sh — #3446 AC1 (red-first, DEC-1674)
#
# The owl-api launcher must WAIT for Fuseki to be ready before exec'ing owl-api,
# instead of racing ahead at boot, failing the first fuseki-query, and crash-looping
# under KeepAlive. We prove this by running the launcher in a sandbox with a stubbed
# curl that fails the first two readiness probes then succeeds, and a fake owl-api
# that drops a marker. The launcher must (a) retry past the early failures and
# (b) still exec owl-api once Fuseki is "ready".
#
# Run: bash platform/tests/owl-api-launch-readiness.test.sh

set -uo pipefail

PASS=0
FAIL=0
test_pass() { echo "  PASS: $1"; ((PASS++)); }
test_fail() { echo "  FAIL: $1"; ((FAIL++)); }

echo "=== owl-api launch readiness gate (#3446 AC1) ==="

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
LAUNCHER="$REPO_ROOT/platform/services/owl-api/owl-api-launch.sh"

if [ ! -f "$LAUNCHER" ]; then
  test_fail "canonical launcher source missing at platform/services/owl-api/owl-api-launch.sh"
  echo ""
  echo "=== Results: $PASS passed, $FAIL failed ==="
  exit 1
fi

# --- sandbox ---
SBX="$(mktemp -d)"
trap 'rm -rf "$SBX"' EXIT
export HOME="$SBX/home"
mkdir -p "$HOME/.chorus/bin"

MARKER="$SBX/owl-api-ran"
COUNTER="$SBX/curl-calls"
echo 0 > "$COUNTER"

# fake owl-api: the thing the launcher should exec once Fuseki is ready
cat > "$HOME/.chorus/bin/owl-api" <<EOF
#!/usr/bin/env bash
echo "fake-owl-api started" > "$MARKER"
exit 0
EOF
chmod +x "$HOME/.chorus/bin/owl-api"

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

# AC1b: launcher still exec'd owl-api once Fuseki was ready
if [ -f "$MARKER" ]; then
  test_pass "launcher exec'd owl-api after Fuseki became ready"
else
  test_fail "launcher never exec'd owl-api (rc=$rc)"
fi

echo ""
echo "=== Results: $PASS passed, $FAIL failed ==="
[ "$FAIL" -eq 0 ]
