#!/usr/bin/env bash
# Test: pair-heartbeat is DAEMON-side, and the agent-cron→skill fragility class is gone (#3253).
# This is the anti-jenga guard: it fails if the cron-fires-skill path ever returns.

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
PASS=0
FAIL=0
assert() {
  local desc="$1"; shift
  if "$@" >/dev/null 2>&1; then echo "  ✓ $desc"; PASS=$((PASS + 1))
  else echo "  ✗ $desc"; FAIL=$((FAIL + 1)); fi
}

echo "## pair-heartbeat daemon-side (#3253 — fragility class removed)"

# The daemon + its host exist
assert "pair-heartbeat crate exists" test -f "$ROOT/platform/services/pair-heartbeat/Cargo.toml"
assert "pair-heartbeat has unit tests" test -f "$ROOT/platform/services/pair-heartbeat/tests/units.rs"
assert "com.chorus.pair-heartbeat plist exists" test -f "$ROOT/config/launchagents/com.chorus.pair-heartbeat.plist"
assert "plist runs on a StartInterval timer" grep -q "StartInterval" "$ROOT/config/launchagents/com.chorus.pair-heartbeat.plist"
assert "chorus-deploy ships pair-heartbeat" grep -q "pair-heartbeat" "$ROOT/platform/scripts/chorus-deploy"

# The fragility class is GONE: no agent-cron, no skill-name (the #2317 bug shapes)
assert "the /pair-heartbeat-check skill is retired" bash -c "test ! -f '$ROOT/skills/pair-heartbeat-check/SKILL.md'"
assert "/pair no longer CronCreates a heartbeat" bash -c "! grep -qiE 'CronCreate.*pair-heartbeat' '$ROOT/skills/pair/SKILL.md'"
assert "/pair no longer invokes the heartbeat-check skill" bash -c "! grep -qE '/pair-heartbeat-check' '$ROOT/skills/pair/SKILL.md'"

# /pair does the one thing it should: register the pair for the daemon
assert "/pair registers the active pair durably" grep -q "active-pairs" "$ROOT/skills/pair/SKILL.md"
assert "/pair references the daemon monitor" grep -q "com.chorus.pair-heartbeat" "$ROOT/skills/pair/SKILL.md"

echo ""
echo "Results: $PASS passed, $FAIL failed"
[ "$FAIL" -eq 0 ] && exit 0 || exit 1
