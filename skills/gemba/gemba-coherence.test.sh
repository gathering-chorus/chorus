#!/usr/bin/env bash
# Test: gemba is the pulse-gather poll, not the retired snapshot-diff scaffolding (#3205).
# Red until /gemba points at the pulse-gather verb and the start/tick scripts are gone.

# Resolve ROOT from this script's own location (skills/gemba/) so the test verifies
# the tree it lives in — the werk pre-merge, canonical after — never a hardcoded path
# that silently tests the wrong checkout (eliminate-runtime-dep, DEC).
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
PASS=0
FAIL=0

assert() {
  local desc="$1"
  shift
  if "$@" >/dev/null 2>&1; then
    echo "  ✓ $desc"
    PASS=$((PASS + 1))
  else
    echo "  ✗ $desc"
    FAIL=$((FAIL + 1))
  fi
}

echo "## gemba coherence tests (#3205 — one poll path)"

# The retired scaffolding is GONE (AC4: deleted, not dormant)
assert "gemba-start.sh removed" test ! -f "$ROOT/platform/scripts/gemba-start.sh"
assert "gemba-tick.sh removed" test ! -f "$ROOT/platform/scripts/gemba-tick.sh"
assert "gemba-tick skill removed" test ! -f "$ROOT/skills/gemba-tick/SKILL.md"

# The new source of truth exists
assert "pulse-gather crate exists" test -f "$ROOT/platform/services/pulse-gather/Cargo.toml"
assert "pulse-gather has unit tests" test -f "$ROOT/platform/services/pulse-gather/tests/units.rs"

# /gemba polls the verb, not a snapshot script
assert "gemba skill invokes pulse-gather" grep -q "pulse-gather" "$ROOT/skills/gemba/SKILL.md"
assert "gemba skill no longer calls gemba-start" bash -c "! grep -q 'gemba-start' '$ROOT/skills/gemba/SKILL.md'"
assert "gemba skill no longer calls gemba-tick.sh" bash -c "! grep -q 'gemba-tick.sh' '$ROOT/skills/gemba/SKILL.md'"

# #3274: the skill no longer claims a continuous loop it doesn't maintain
assert "gemba skill no longer claims a CronCreate loop (#3274)" bash -c "! grep -q 'CronCreate' '$ROOT/skills/gemba/SKILL.md'"
assert "gemba skill no longer CronDeletes a loop (#3274)" bash -c "! grep -q 'CronDelete' '$ROOT/skills/gemba/SKILL.md'"

# Deploy path knows the verb (so it ships — merged≠live guard)
assert "chorus-deploy registers pulse-gather" grep -q "pulse-gather" "$ROOT/platform/scripts/chorus-deploy"

# #3319: gemba is the loom-gemba verb (observation semantics) ON the pulse-gather core
assert "loom-gemba crate exists" test -f "$ROOT/platform/services/loom-gemba/Cargo.toml"
assert "loom-gemba has unit tests" test -f "$ROOT/platform/services/loom-gemba/tests/units.rs"
assert "loom-gemba links pulse-gather (no second implementation)" grep -q 'path = "../pulse-gather"' "$ROOT/platform/services/loom-gemba/Cargo.toml"
assert "gemba skill invokes the loom-gemba MCP tool" grep -q "loom-gemba" "$ROOT/skills/gemba/SKILL.md"
assert "chorus-deploy registers loom-gemba" grep -q "loom-gemba" "$ROOT/platform/scripts/chorus-deploy"
assert "MCP server exposes loom-gemba" grep -q "LOOM_GEMBA_TOOL_DEF" "$ROOT/platform/mcp-server/src/server.ts"

echo ""
echo "Results: $PASS passed, $FAIL failed"
[ "$FAIL" -eq 0 ] && exit 0 || exit 1
