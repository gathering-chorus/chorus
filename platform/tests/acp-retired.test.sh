#!/usr/bin/env bash
# Retirement gate for the acp skill (#3422, werk-simplify #1).
#
# acp (accept-commit-push) was superseded by werk (the chorus_werk pipeline; GO=accept).
# Jeff retired it 2026-06-14: "we dont want or need acp anymore — we need to clean up
# our skills so the roles dont get confused." Both /acp and the /acp-v2 "until-cutover"
# sibling are gone, plus the platform/skills/acp duplicate and the werk-acp-bin orphan.
#
# This is a STRUCTURAL-MEMORY gate (feedback_retirement_gates_are_structural_memory):
# the deletion alone is reversible drift; this gate makes "acp stays gone" enforceable —
# if a future change re-introduces an acp skill dir or registration, CI fails LOUD.
#
# Repo-scoped only (no ~/.claude/skills or ~/.chorus/bin runtime state — those are
# deploy artifacts, not version-controlled). Resolves CHORUS_REPO from this script's
# location like the sibling *.test.sh gates.
set -uo pipefail

CHORUS_REPO="${CHORUS_REPO:-$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)}"
fail=0
note() { echo "  $1"; }

# 1) No acp skill directories anywhere in the repo skill trees.
for d in skills/acp skills/acp-v2 platform/skills/acp; do
  if [ -e "$CHORUS_REPO/$d" ]; then
    echo "FAIL: acp skill dir still present: $d"; note "acp is retired (#3422) — remove it"; fail=1
  fi
done

# 2) No SKILL.md anywhere declares an acp skill (name: acp / acp-v2).
hits=$(grep -rlE '^name:[[:space:]]*acp(-v2)?[[:space:]]*$' "$CHORUS_REPO/skills" "$CHORUS_REPO/platform/skills" 2>/dev/null || true)
if [ -n "$hits" ]; then
  echo "FAIL: a SKILL.md still declares an acp skill:"; echo "$hits" | sed 's/^/    /'; fail=1
fi

# 3) No live invocation path advertises /acp as a command to run (advice strings, wrappers).
#    Concept/history mentions (chorus_acp git trailers, "survives acp") are NOT matched —
#    only the user-facing "use /acp" / "run /acp" command form.
inv=$(grep -rnE '(use|run|invoke|via)[[:space:]]+/acp\b' "$CHORUS_REPO/platform/scripts" 2>/dev/null || true)
if [ -n "$inv" ]; then
  echo "FAIL: a script still advertises /acp as a runnable command:"; echo "$inv" | sed 's/^/    /'
  note "the accept path is the werk flow (cw <card> go) — GO=accept"; fail=1
fi

if [ "$fail" = 0 ]; then
  echo "PASS: acp is retired — no skill dirs, no SKILL.md registration, no /acp command advice."
fi
exit $fail
