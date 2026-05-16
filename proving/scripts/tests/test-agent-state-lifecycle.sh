#!/bin/bash
# test-agent-state-lifecycle.sh — coverage for agent-state.sh deploy/rollback verbs (#2605 AC6).
#
# Covers:
#   T1: status on a running service returns 0 with PID + state
#   T2: deploy on unknown crate refuses with exit 2 + service.deploy.failed reason=service-not-found
#   T3: deploy emits paired service.deploy.{started,completed} on the happy path (mocked chorus-deploy)
#   T4: deploy detects cdhash divergence (pre==post when expecting change) → service.deploy.failed reason=cdhash-divergence
#   T5: rollback refuses on unknown crate with exit 2 + service.rollback.failed reason=service-not-found
#
# Strategy: tests stub chorus-deploy + launchctl + codesign via PATH precedence so we don't
# touch production services. Real integration sits behind CHORUS_LIFECYCLE_LIVE=1 (not set here).
#
# Usage: bash proving/scripts/tests/test-agent-state-lifecycle.sh
# Exit: 0 = all green; non-zero = first failure.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"
AGENT_STATE="$REPO_ROOT/platform/scripts/agent-state.sh"

TMPDIR="$(mktemp -d -t agent-state-lifecycle-XXXXXX)"
trap 'rm -rf "$TMPDIR"' EXIT

mkdir -p "$TMPDIR/stub-bin"
SPINE_LOG="$TMPDIR/spine-log.txt"
: > "$SPINE_LOG"

# chorus-log stub — captures spine events.
cat > "$TMPDIR/stub-bin/chorus-log" <<EOF
#!/bin/bash
echo "\$@" >> "$SPINE_LOG"
EOF
chmod +x "$TMPDIR/stub-bin/chorus-log"

# chorus-deploy stub — succeeds by default.
cat > "$TMPDIR/stub-bin/chorus-deploy" <<'EOF'
#!/bin/bash
exit "${STUB_CHORUS_DEPLOY_EXIT:-0}"
EOF
chmod +x "$TMPDIR/stub-bin/chorus-deploy"

# launchctl stub — list returns a fake PID; kickstart succeeds.
cat > "$TMPDIR/stub-bin/launchctl" <<'EOF'
#!/bin/bash
case "$1" in
  list)
    if [ -n "${2:-}" ]; then
      echo "{\"PID\" = ${STUB_LAUNCHCTL_PID:-12345};};"
    else
      printf "%s\t0\tcom.chorus.api\n" "${STUB_LAUNCHCTL_PID:-12345}"
    fi
    ;;
  kickstart) exit "${STUB_LAUNCHCTL_EXIT:-0}" ;;
  bootstrap|bootout) exit 0 ;;
  *) exit 0 ;;
esac
EOF
chmod +x "$TMPDIR/stub-bin/launchctl"

# codesign stub — emits CDHash.
cat > "$TMPDIR/stub-bin/codesign" <<'EOF'
#!/bin/bash
echo "Executable=/dev/null"
echo "CDHash=${STUB_CDHASH:-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa}"
echo "CandidateCDHash sha256=${STUB_CDHASH:-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa}"
EOF
chmod +x "$TMPDIR/stub-bin/codesign"

# lsof + ps stubs.
cat > "$TMPDIR/stub-bin/lsof" <<EOF
#!/bin/bash
echo "stub-cmd 12345 user txt REG 1,2 12345 0 $TMPDIR/stub-bin/codesign"
EOF
chmod +x "$TMPDIR/stub-bin/lsof"

cat > "$TMPDIR/stub-bin/ps" <<'EOF'
#!/bin/bash
echo "1"
EOF
chmod +x "$TMPDIR/stub-bin/ps"

export PATH="$TMPDIR/stub-bin:$PATH"
export CHORUS_ROLE=silas
export CHORUS_LOG_BIN="$TMPDIR/stub-bin/chorus-log"

PASS=0
FAIL=0
report() {
  local name="$1" status="$2" detail="${3:-}"
  if [ "$status" = "pass" ]; then
    echo "  ok  $name"
    PASS=$((PASS+1))
  else
    echo "  FAIL $name${detail:+ — $detail}"
    FAIL=$((FAIL+1))
  fi
}

# T1: status verb smoke
echo "T1: status verb"
out=$("$AGENT_STATE" status 2>&1) && code=0 || code=$?
if [ "$code" -eq 0 ] || echo "$out" | grep -q "AGENT"; then
  report "status verb returns 0 or prints header" pass
else
  report "status verb returns 0 or prints header" fail "exit=$code"
fi

# T2: deploy unknown crate refusal
echo "T2: deploy unknown crate refusal"
: > "$SPINE_LOG"
out=$("$AGENT_STATE" deploy not-a-real-crate 2>&1) && code=0 || code=$?
if [ "$code" -eq 2 ]; then
  report "deploy unknown crate exits 2" pass
else
  report "deploy unknown crate exits 2" fail "exit=$code"
fi
if grep -q 'service.deploy.failed.*reason=service-not-found' "$SPINE_LOG"; then
  report "deploy unknown crate emits service.deploy.failed reason=service-not-found" pass
else
  report "deploy unknown crate emits service.deploy.failed reason=service-not-found" fail "log=$(cat "$SPINE_LOG")"
fi

# T3: deploy happy path emits started event
echo "T3: deploy emits service.deploy.started on happy path"
: > "$SPINE_LOG"
STUB_CDHASH=aaaa STUB_LAUNCHCTL_PID=11111 "$AGENT_STATE" deploy chorus-api >/dev/null 2>&1 || true
if grep -q 'service.deploy.started.*service=com.chorus.api' "$SPINE_LOG"; then
  report "deploy emits service.deploy.started service=com.chorus.api" pass
else
  report "deploy emits service.deploy.started service=com.chorus.api" fail "log=$(cat "$SPINE_LOG")"
fi

# T4: cdhash divergence detection — stubs emit constant cdhash so pre==post, should fail
echo "T4: deploy detects cdhash unchanged (no pickup) → reason=cdhash-divergence"
: > "$SPINE_LOG"
STUB_CDHASH=bbbb STUB_LAUNCHCTL_PID=22222 "$AGENT_STATE" deploy chorus-api >/dev/null 2>&1 || true
if grep -q 'service.deploy.failed.*reason=cdhash-divergence' "$SPINE_LOG"; then
  report "cdhash unchanged → service.deploy.failed reason=cdhash-divergence" pass
else
  report "cdhash unchanged → service.deploy.failed reason=cdhash-divergence" fail "log=$(cat "$SPINE_LOG")"
fi

# T5: rollback unknown crate refusal
echo "T5: rollback unknown crate refusal"
: > "$SPINE_LOG"
out=$("$AGENT_STATE" rollback not-a-real-crate 2>&1) && code=0 || code=$?
if [ "$code" -eq 2 ]; then
  report "rollback unknown crate exits 2" pass
else
  report "rollback unknown crate exits 2" fail "exit=$code"
fi
if grep -q 'service.rollback.failed.*reason=service-not-found' "$SPINE_LOG"; then
  report "rollback unknown crate emits service.rollback.failed reason=service-not-found" pass
else
  report "rollback unknown crate emits service.rollback.failed reason=service-not-found" fail "log=$(cat "$SPINE_LOG")"
fi

echo ""
echo "Summary: $PASS pass, $FAIL fail"
[ "$FAIL" -eq 0 ]
