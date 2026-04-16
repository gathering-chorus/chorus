#!/usr/bin/env bash
# app-state-no-docker.test.sh — Verify app-state.sh has no Docker dependency
# Card #2075 AC: no Docker references, status works without Docker, no auto-start
# Run: bash platform/tests/app-state-no-docker.test.sh

set -uo pipefail

PASS=0
FAIL=0
test_pass() { echo "  PASS: $1"; ((PASS++)); }
test_fail() { echo "  FAIL: $1"; ((FAIL++)); }

SCRIPT="/Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/app-state.sh"

echo "=== app-state.sh Docker Removal Tests (#2075) ==="

# --- AC #1: app-state.sh does not reference Docker daemon or Docker Desktop ---

# Test 1: No check_docker function
if grep -q 'check_docker()' "$SCRIPT"; then
  test_fail "Script still has check_docker() function"
else
  test_pass "No check_docker() function"
fi

# Test 2: No 'docker info' calls (Docker daemon detection)
if grep -q 'docker info' "$SCRIPT"; then
  test_fail "Script still calls 'docker info'"
else
  test_pass "No 'docker info' calls"
fi

# Test 3: No 'open -a Docker' (Docker Desktop auto-start)
if grep -q 'open -a Docker' "$SCRIPT"; then
  test_fail "Script still auto-starts Docker Desktop"
else
  test_pass "No Docker Desktop auto-start (open -a Docker)"
fi

# Test 4: No docker ps/start/stop/restart/rm commands
if grep -qE 'docker (ps|start|stop|restart|rm|logs|network|volume)' "$SCRIPT"; then
  test_fail "Script still has docker CLI commands"
else
  test_pass "No docker CLI commands"
fi

# Test 5: No Docker container name references
if grep -q 'jeff-bridwell-personal-site-app\|jeff-bridwell-personal-site-fuseki' "$SCRIPT"; then
  test_fail "Script still references Docker container names"
else
  test_pass "No Docker container name references"
fi

# Test 6: No Terraform references (Terraform managed Docker infra)
if grep -q 'terraform\|check_terraform' "$SCRIPT"; then
  test_fail "Script still references Terraform"
else
  test_pass "No Terraform references"
fi

# --- AC #2: Status check works without Docker installed/running ---

# Test 7: cmd_status does not call check_docker
if grep -A5 'cmd_status()' "$SCRIPT" | grep -q 'check_docker'; then
  test_fail "cmd_status still calls check_docker"
else
  test_pass "cmd_status does not call check_docker"
fi

# Test 8: check_health does not check Docker daemon
if grep -A5 'check_health()' "$SCRIPT" | grep -q 'docker info'; then
  test_fail "check_health still checks Docker daemon"
else
  test_pass "check_health does not check Docker daemon"
fi

# Test 9: Status uses LaunchAgent checks (service_running or launchctl)
if grep -q 'service_running\|launchctl' "$SCRIPT"; then
  test_pass "Script uses LaunchAgent-based service checks"
else
  test_fail "Script does not use LaunchAgent-based service checks"
fi

# Test 10: Status command runs without error (live check)
STATUS_OUTPUT=$("$SCRIPT" status 2>&1)
STATUS_EXIT=$?
if [ "$STATUS_EXIT" -eq 0 ] || [ "$STATUS_EXIT" -eq 1 ]; then
  # Exit 0 = all healthy, exit 1 = some unhealthy — both are valid
  test_pass "Status command executes without crashing"
else
  test_fail "Status command crashed with exit code $STATUS_EXIT"
fi

# --- AC #3: No auto-start of any container runtime ---

# Test 11: No 'open -a' calls for any runtime
if grep -q 'open -a' "$SCRIPT"; then
  test_fail "Script still has 'open -a' calls (auto-starts something)"
else
  test_pass "No 'open -a' calls"
fi

# Test 12: No docker-compose references
if grep -qi 'docker-compose\|docker compose' "$SCRIPT"; then
  test_fail "Script still references docker-compose"
else
  test_pass "No docker-compose references"
fi

# Test 13: Help text does not mention Docker
if "$SCRIPT" help 2>&1 | grep -qi 'docker'; then
  test_fail "Help text still mentions Docker"
else
  test_pass "Help text does not mention Docker"
fi

# Test 14: Script has valid syntax
if bash -n "$SCRIPT" 2>/dev/null; then
  test_pass "Script has valid bash syntax"
else
  test_fail "Script has syntax errors"
fi

echo ""
echo "=== Results: $PASS passed, $FAIL failed ==="
[ "$FAIL" -eq 0 ] && exit 0 || exit 1
