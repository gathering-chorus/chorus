#!/usr/bin/env bash
# Test: gemba-start.sh and gemba-tick.sh show consistent data sources
# Red until gemba-start.sh reads session JSONL like gemba-tick.sh does

SCRIPT_DIR="/Users/jeffbridwell/CascadeProjects/chorus/platform/scripts"
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

echo "## gemba coherence tests"

# Scripts exist
assert "gemba-start.sh exists" test -f "$SCRIPT_DIR/gemba-start.sh"
assert "gemba-tick.sh exists" test -f "$SCRIPT_DIR/gemba-tick.sh"

# Coherence: gemba-start uses same sources as gemba-tick
assert "gemba-start.sh reads session JSONL" grep -q "SESSION_DIR" "$SCRIPT_DIR/gemba-start.sh"
assert "gemba-start.sh has observer fallback" grep -q "claude-team-scan" "$SCRIPT_DIR/gemba-start.sh"

# Namespace: wren path is current
assert "gemba-start.sh wren path is current" grep -q "roles-wren\|roles/wren" "$SCRIPT_DIR/gemba-start.sh"
assert "gemba-tick.sh wren path is current" grep -q "roles-wren\|roles/wren" "$SCRIPT_DIR/gemba-tick.sh"

# Skill files at new root location
assert "skills/gemba/SKILL.md exists at root" test -f "/Users/jeffbridwell/CascadeProjects/chorus/skills/gemba/SKILL.md"
assert "skills/gemba-tick/SKILL.md exists at root" test -f "/Users/jeffbridwell/CascadeProjects/chorus/skills/gemba-tick/SKILL.md"

# Wren has real copies not symlinks
assert "wren gemba skill is real file" test -f "/Users/jeffbridwell/CascadeProjects/chorus/roles/wren/.claude/skills/gemba/SKILL.md"
assert "wren gemba-tick is not a symlink" test ! -L "/Users/jeffbridwell/CascadeProjects/chorus/roles/wren/.claude/skills/gemba-tick"

echo ""
echo "Results: $PASS passed, $FAIL failed"
[ "$FAIL" -eq 0 ] && exit 0 || exit 1
