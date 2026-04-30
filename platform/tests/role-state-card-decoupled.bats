#!/usr/bin/env bats
# role-state-card-decoupled.bats — #2467 wave 2 (AC5)
#
# Asserts that no skill source file passes `card=` or `type=` arguments
# to the role-state CLI. Card belongs to the board; role-state owns
# session/attention metadata only (Jeff 2026-04-30 directive).
#
# The Rust role_state.rs writer (wave 1, PR #72) silently drops these
# args, so the skills don't break — but the literal instruction text
# still lives in skill markdown until this gate goes green and the
# files get cleaned up.
#
# This test is the TDD anchor for AC5: red against current main, green
# after the skill source edits land.

# Default to the repo root the test file lives in (works in any worktree
# per the per-role-worktree convention), not a hardcoded /chorus path.
CHORUS_ROOT="${CHORUS_ROOT:-$(cd "${BATS_TEST_DIRNAME}/../.." && pwd)}"
SKILLS_DIR="$CHORUS_ROOT/skills"

@test "no skill source passes card= to role-state" {
  # role-state <role> <state> ... must not be followed by a card= arg
  matches=$(grep -rn "role-state.*card=" "$SKILLS_DIR" 2>/dev/null \
    | grep -v -E '^\s*#|//' \
    || true)
  if [ -n "$matches" ]; then
    echo "Found skill sources still passing card= to role-state:"
    echo "$matches"
    false
  fi
}

@test "no skill source passes type= to role-state" {
  matches=$(grep -rn "role-state.*type=" "$SKILLS_DIR" 2>/dev/null \
    | grep -v -E '^\s*#|//' \
    || true)
  if [ -n "$matches" ]; then
    echo "Found skill sources still passing type= to role-state:"
    echo "$matches"
    false
  fi
}

@test "no CLAUDE.md fragment uses 'building card=<id>' syntax" {
  # Per AC6: instruction text in CLAUDE.md fragments shouldn't tell
  # roles to declare with card=. Historical / quoted mentions allowed
  # (in code blocks discussing what was removed) but live instructions
  # must not.
  fragments_dir="$CHORUS_ROOT/designing/claudemd/shared"
  [ -d "$fragments_dir" ] || skip "fragments dir not found"
  matches=$(grep -rn 'building card=' "$fragments_dir" 2>/dev/null \
    | grep -v -E '#2467|deprecated|removed|historical|retired' \
    || true)
  if [ -n "$matches" ]; then
    echo "Found CLAUDE.md fragments with live 'building card=' instructions:"
    echo "$matches"
    false
  fi
}
