#!/usr/bin/env bash
# Test: pre-commit WIP gate — reject commits when role has no WIP card (#1799)
# RED before gate exists. GREEN after.
set -euo pipefail

SCAN_DIR="/tmp/claude-team-scan"
PASS=0
FAIL=0

# The gate logic extracted for testability.
# In production this runs inside .git/hooks/pre-commit.
# #2204: state file is a cache; board is authoritative. Falls back to board
# check via cards CLI when state file is missing/stale/idle, so a role with
# real WIP on the board commits even when /tmp/.../*-declared.json drifted.
_role_has_board_wip() {
  local role="$1"
  local cards_cmd="${CARDS_CMD:-/Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/cards}"
  # `cards list --status WIP` includes a Next section after the WIP block;
  # scope the grep to the WIP block only. Anchor to the metadata bracket
  # shape '[Role|P<digit>' so a card whose TITLE contains literal '[Role|'
  # can't false-positive (kade #2204 review).
  "$cards_cmd" list --status WIP 2>/dev/null \
    | awk '/^WIP \(/{p=1; next} /^[A-Z]/ && p{p=0} p' \
    | grep -qiE "\[${role}\|P[0-9]"
}

wip_gate_check() {
  local role="${DEPLOY_ROLE:-}"
  local msg="${COMMIT_MSG:-}"

  # No role = not a role commit, bypass
  [ -z "$role" ] && return 0

  # Swat/chore bypass
  echo "$msg" | grep -qiE '\b(swat|chore)\b' && return 0

  # Check role state cache
  local state_file="${SCAN_DIR}/${role}-declared.json"
  local wip_ok=false
  if [ -f "$state_file" ]; then
    local state card
    state=$(python3 -c "import json; d=json.load(open('${state_file}')); print(d.get('state',''))" 2>/dev/null || echo "")
    card=$(python3 -c "import json; d=json.load(open('${state_file}')); print(d.get('card',''))" 2>/dev/null || echo "")
    if [ "$state" = "building" ] && [ -n "$card" ] && [ "$card" != "None" ]; then
      wip_ok=true
    fi
  fi

  # #2204: cache miss/stale/idle — fall back to board (authoritative)
  if ! $wip_ok && _role_has_board_wip "$role"; then
    wip_ok=true
  fi

  if ! $wip_ok; then
    echo "pre-commit: blocked — ${role} has no WIP card declared. Pull a card first."
    return 1
  fi

  return 0
}

run_test() {
  local name="$1"; shift
  if "$@" 2>/dev/null; then
    echo "  PASS: $name"
    PASS=$((PASS + 1))
  else
    echo "  FAIL: $name"
    FAIL=$((FAIL + 1))
  fi
}

run_test_expect_fail() {
  local name="$1"; shift
  if "$@" 2>/dev/null; then
    echo "  FAIL: $name (expected failure but got success)"
    FAIL=$((FAIL + 1))
  else
    echo "  PASS: $name"
    PASS=$((PASS + 1))
  fi
}

echo "=== pre-commit WIP gate tests ==="

mkdir -p "$SCAN_DIR"

# Stubs for cards CLI — drives the board fallback (#2204).
STUB_DIR=$(mktemp -d)
cat > "$STUB_DIR/cards-with-wip" <<'EOF'
#!/bin/bash
# Simulates `cards list --status WIP` output with a testrol-owned card.
echo "WIP (1):"
echo "  1234  Some card title [testrol|P1|chunk:ops|domain:chorus]"
EOF
cat > "$STUB_DIR/cards-empty" <<'EOF'
#!/bin/bash
echo "WIP (0):"
EOF
# Mimics the real cards CLI: WIP block followed by a Next block. The role
# only appears in the Next section — must NOT count as having WIP.
cat > "$STUB_DIR/cards-only-in-next" <<'EOF'
#!/bin/bash
echo "WIP (1):"
echo "  9999  Other role's card [otherrol|P1|chunk:ops|domain:chorus]"
echo ""
echo "Next (2):"
echo "  4321  Some testrol card [testrol|P2|chunk:ops|domain:chorus]"
echo "  5432  Another testrol card [testrol|P2|chunk:ops|domain:chorus]"
EOF

# Card title contains literal '[testrol|' as text — must NOT count as
# testrol owning WIP. Caught by Kade in #2204 review: anchor grep to the
# card-list line shape so titles can't false-positive.
cat > "$STUB_DIR/cards-with-rolename-in-title" <<'EOF'
#!/bin/bash
echo "WIP (1):"
echo "  7777  Card titled with [testrol| as plain text [otherrol|P1|chunk:ops|domain:chorus]"
EOF
chmod +x "$STUB_DIR/cards-with-wip" "$STUB_DIR/cards-empty" "$STUB_DIR/cards-only-in-next" "$STUB_DIR/cards-with-rolename-in-title"

# 1. No DEPLOY_ROLE — bypass (Jeff's direct commits)
run_test "bypass when DEPLOY_ROLE unset" env -u DEPLOY_ROLE bash -c "$(declare -f wip_gate_check); SCAN_DIR='$SCAN_DIR'; wip_gate_check"

# 2. Role with no state file AND no board WIP — blocked
run_test_expect_fail "blocked when no state file" bash -c "$(declare -f _role_has_board_wip wip_gate_check); SCAN_DIR='$SCAN_DIR'; CARDS_CMD='$STUB_DIR/cards-empty'; DEPLOY_ROLE=testrol; export SCAN_DIR CARDS_CMD DEPLOY_ROLE; wip_gate_check"

# 3. Role with WIP card — allowed
echo '{"role":"testrol","state":"building","card":1799}' > "$SCAN_DIR/testrol-declared.json"
run_test "allowed when role has WIP card" bash -c "$(declare -f wip_gate_check); SCAN_DIR='$SCAN_DIR'; DEPLOY_ROLE=testrol; export DEPLOY_ROLE; wip_gate_check"

# 4. Role idle (no card) AND no board WIP — blocked
echo '{"role":"testrol","state":"idle"}' > "$SCAN_DIR/testrol-declared.json"
run_test_expect_fail "blocked when role is idle" bash -c "$(declare -f _role_has_board_wip wip_gate_check); SCAN_DIR='$SCAN_DIR'; CARDS_CMD='$STUB_DIR/cards-empty'; DEPLOY_ROLE=testrol; export SCAN_DIR CARDS_CMD DEPLOY_ROLE; wip_gate_check"

# 5. Swat bypass — allowed even without card
echo '{"role":"testrol","state":"idle"}' > "$SCAN_DIR/testrol-declared.json"
run_test "bypass for swat commit" bash -c "$(declare -f wip_gate_check); SCAN_DIR='$SCAN_DIR'; DEPLOY_ROLE=testrol; COMMIT_MSG='silas: swat — fix something'; export DEPLOY_ROLE COMMIT_MSG; wip_gate_check"

# 6. Chore bypass — allowed even without card
run_test "bypass for chore commit" bash -c "$(declare -f wip_gate_check); SCAN_DIR='$SCAN_DIR'; DEPLOY_ROLE=testrol; COMMIT_MSG='silas: chore — cleanup'; export DEPLOY_ROLE COMMIT_MSG; wip_gate_check"

# --- #2204 board-fallback tests ---
# Cache says no-WIP, but board says role owns one → allowed (the fix).
# Cache says no-WIP and board agrees → blocked (regression guard).
# Both signals must default to no-WIP for the block to fire.

# 7. Stale state file (idle) but role owns WIP on board → allowed (#2204)
echo '{"role":"testrol","state":"idle"}' > "$SCAN_DIR/testrol-declared.json"
run_test "stale-cache + board-has-WIP allowed (#2204)" bash -c "$(declare -f _role_has_board_wip wip_gate_check); SCAN_DIR='$SCAN_DIR'; CARDS_CMD='$STUB_DIR/cards-with-wip'; DEPLOY_ROLE=testrol; export SCAN_DIR CARDS_CMD DEPLOY_ROLE; wip_gate_check"

# 8. Missing state file but role owns WIP on board → allowed (#2204)
rm -f "$SCAN_DIR/testrol-declared.json"
run_test "missing-cache + board-has-WIP allowed (#2204)" bash -c "$(declare -f _role_has_board_wip wip_gate_check); SCAN_DIR='$SCAN_DIR'; CARDS_CMD='$STUB_DIR/cards-with-wip'; DEPLOY_ROLE=testrol; export SCAN_DIR CARDS_CMD DEPLOY_ROLE; wip_gate_check"

# 9. Missing state file AND empty board → still blocked (regression guard)
run_test_expect_fail "missing-cache + empty-board blocked" bash -c "$(declare -f _role_has_board_wip wip_gate_check); SCAN_DIR='$SCAN_DIR'; CARDS_CMD='$STUB_DIR/cards-empty'; DEPLOY_ROLE=testrol; export SCAN_DIR CARDS_CMD DEPLOY_ROLE; wip_gate_check"

# 10. Idle cache AND empty board → still blocked
echo '{"role":"testrol","state":"idle"}' > "$SCAN_DIR/testrol-declared.json"
run_test_expect_fail "idle-cache + empty-board blocked" bash -c "$(declare -f _role_has_board_wip wip_gate_check); SCAN_DIR='$SCAN_DIR'; CARDS_CMD='$STUB_DIR/cards-empty'; DEPLOY_ROLE=testrol; export SCAN_DIR CARDS_CMD DEPLOY_ROLE; wip_gate_check"

# 11. Role appears only in Next (not WIP) → still blocked. Regression guard
# for the section-scoping bug: naive grep across the whole listing matches
# Next entries too.
rm -f "$SCAN_DIR/testrol-declared.json"
run_test_expect_fail "role-only-in-Next-not-WIP blocked" bash -c "$(declare -f _role_has_board_wip wip_gate_check); SCAN_DIR='$SCAN_DIR'; CARDS_CMD='$STUB_DIR/cards-only-in-next'; DEPLOY_ROLE=testrol; export SCAN_DIR CARDS_CMD DEPLOY_ROLE; wip_gate_check"

# 12. Card title contains '[testrol|' as plain text but is owned by another
# role → still blocked. Regression guard for grep-anchor weakness (#2204
# review by kade): without line-shape anchor, the title text false-positives.
run_test_expect_fail "rolename-in-title-not-owner blocked" bash -c "$(declare -f _role_has_board_wip wip_gate_check); SCAN_DIR='$SCAN_DIR'; CARDS_CMD='$STUB_DIR/cards-with-rolename-in-title'; DEPLOY_ROLE=testrol; export SCAN_DIR CARDS_CMD DEPLOY_ROLE; wip_gate_check"

# Cleanup test state
rm -f "$SCAN_DIR/testrol-declared.json"
rm -rf "$STUB_DIR"

echo ""
echo "Results: $PASS passed, $FAIL failed"
[ "$FAIL" -eq 0 ] && exit 0 || exit 1
