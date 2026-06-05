#!/usr/bin/env bash
# Test: /pair-heartbeat-check exists and monitors via pulse-gather, not a bespoke
# timestamp file (#2317). Red until the skill ships and /pair references it.

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
PASS=0
FAIL=0

assert() {
  local desc="$1"; shift
  if "$@" >/dev/null 2>&1; then echo "  ✓ $desc"; PASS=$((PASS + 1))
  else echo "  ✗ $desc"; FAIL=$((FAIL + 1)); fi
}

echo "## pair-heartbeat-check coherence (#2317)"

# The missing skill now exists (the #2317 bug: cron fired 'unknown skill')
assert "skill file exists" test -f "$ROOT/skills/pair-heartbeat-check/SKILL.md"

# It takes the documented arg contract
assert "documents <card-id> <navigator-role> args" grep -q "card-id> <navigator-role" "$ROOT/skills/pair-heartbeat-check/SKILL.md"

# Activity signal is pulse-gather (Kade's adoption note — one path, not hand-rolled reads)
assert "monitors via pulse-gather" grep -q "pulse-gather" "$ROOT/skills/pair-heartbeat-check/SKILL.md"
# Precise: the retired file must not be the MECHANISM (written/read), prose mention OK.
assert "does NOT seed/read the retired pair-nav-last-activity file" bash -c "! grep -qE '(> |cat |stat |echo .*>).*pair-nav-last-activity' '$ROOT/skills/pair-heartbeat-check/SKILL.md'"

# The 60/120/180 escalation is present
assert "escalates at 180s with a spine event" grep -q "pair.navigator.stall" "$ROOT/skills/pair-heartbeat-check/SKILL.md"

# /pair points at the skill and dropped the bespoke timestamp file
assert "/pair references the heartbeat-check skill" grep -q "pair-heartbeat-check" "$ROOT/skills/pair/SKILL.md"
assert "/pair no longer seeds pair-nav-last-activity" bash -c "! grep -qE '(> |echo .*>).*pair-nav-last-activity' '$ROOT/skills/pair/SKILL.md'"

echo ""
echo "Results: $PASS passed, $FAIL failed"
[ "$FAIL" -eq 0 ] && exit 0 || exit 1
