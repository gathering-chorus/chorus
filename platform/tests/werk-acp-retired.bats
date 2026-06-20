#!/usr/bin/env bats
# @test-type: unit — static source/shape guard, hermetic
# werk-acp-retired.bats — #3219
#
# Asserts that the werk-acp composition verb, its MCP thin-skin (chorus_acp),
# and its test script have been retired from disk + the server. Background:
# #3175 made werk-accept atomic finalize-only and #3237 made it the go-signal,
# so the flat sequence ends with accept→finalize — werk-acp (the v2 composition
# verb) is obsolete but was still callable by habit (Silas's #3211 nit). This
# card deletes it so it cannot resurrect. event-name references (Loki queries
# for chorus_acp.completed in ops/health scripts) are historical and explicitly
# out of scope here — this gate only guards the verb + its callable surface.

CHORUS_ROOT="${CHORUS_ROOT:-$(cd "${BATS_TEST_DIRNAME}/../.." && pwd)}"
SERVER="$CHORUS_ROOT/platform/mcp-server/src/server.ts"

@test "werk-acp crate is deleted from disk" {
  if [ -d "$CHORUS_ROOT/platform/services/werk-acp" ]; then
    echo "platform/services/werk-acp still exists — must be deleted (#3219)"
    false
  fi
}

@test "no chorus_acp tool definition or switch case in server.ts" {
  matches=$(grep -n "name: 'chorus_acp'\|case 'chorus_acp'\|ACP_TOOL_DEF" "$SERVER" 2>/dev/null \
    | grep -v -E '^\s*//|retired|removed|deprecated|#3219' \
    || true)
  if [ -n "$matches" ]; then
    echo "Found chorus_acp tool surface in server.ts:"
    echo "$matches"
    false
  fi
}

@test "no werk-acp in the executeWerkVerb verb union type" {
  matches=$(grep -n "'werk-acp'" "$SERVER" 2>/dev/null \
    | grep -v -E '^\s*//|retired|removed|deprecated|#3219' \
    || true)
  if [ -n "$matches" ]; then
    echo "Found 'werk-acp' verb reference in server.ts:"
    echo "$matches"
    false
  fi
}

@test "test-acp.sh is deleted" {
  if [ -f "$CHORUS_ROOT/platform/scripts/test-acp.sh" ]; then
    echo "platform/scripts/test-acp.sh still exists — obsolete, must be deleted (#3219)"
    false
  fi
}

# ── #3422 (werk-simplify #1): the SKILL layer ──────────────────────────────
# #3219 retired the verb/crate/MCP. #3422 retires the user-facing /acp + /acp-v2
# SKILLS (Jeff 2026-06-14: "we dont want or need acp anymore"), since werk (the
# chorus_werk pipeline; GO=accept) is the one accept path. acp-v2 was the
# "both live until cutover" sibling — cutover is now. Skill dirs + catalog +
# command advice are guarded here; the ~/.claude/skills symlinks and the
# ~/.chorus/bin/werk-acp-bin orphan are runtime (deploy) artifacts, out of
# scope for a repo gate (same boundary #3219 drew for event-name history).

@test "acp skill directories are deleted (skills + platform/skills)" {
  for d in skills/acp skills/acp-v2 platform/skills/acp; do
    if [ -e "$CHORUS_ROOT/$d" ]; then
      echo "acp skill dir still present: $d — retired #3422"
      false
    fi
  done
}

@test "no SKILL.md declares an acp skill (name: acp / acp-v2)" {
  matches=$(grep -rlE '^name:[[:space:]]*acp(-v2)?[[:space:]]*$' \
    "$CHORUS_ROOT/skills" "$CHORUS_ROOT/platform/skills" 2>/dev/null || true)
  if [ -n "$matches" ]; then
    echo "a SKILL.md still declares an acp skill:"; echo "$matches"; false
  fi
}

@test "no platform script advertises /acp as a runnable command" {
  # Concept/history mentions (chorus_acp git trailers, "survives acp") are NOT
  # matched — only the user-facing 'use/run/invoke/via /acp' command form.
  matches=$(grep -rnE '(use|run|invoke|via)[[:space:]]+/acp\b' \
    "$CHORUS_ROOT/platform/scripts" 2>/dev/null || true)
  if [ -n "$matches" ]; then
    echo "a script still advertises /acp as a runnable command:"; echo "$matches"
    echo "the accept path is the werk flow (cw <card> go) — GO=accept"; false
  fi
}

@test "SKILLS.md catalog has no /acp entry" {
  if grep -qE '^\|[[:space:]]*/acp\b' "$CHORUS_ROOT/skills/SKILLS.md" 2>/dev/null; then
    echo "skills/SKILLS.md still lists /acp — retired #3422"; false
  fi
}
