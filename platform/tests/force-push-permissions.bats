#!/usr/bin/env bats
# #2926 — force-push permissions: allow force-with-lease on role branches,
# deny force on main. Replaces the broad Bash(git push --force*) deny that
# blocked post-rebase role-branch pushes (hit on wren/2924 rebase 2026-05-15).

SETTINGS="$HOME/.claude/settings.json"

@test "settings.json exists" {
  [ -f "$SETTINGS" ]
}

@test "settings.json is valid JSON" {
  python3 -c "import json; json.load(open('$SETTINGS'))"
}

@test "broad force-push deny is gone" {
  python3 -c "
import json, sys
d = json.load(open('$SETTINGS'))
deny = d.get('permissions', {}).get('deny', [])
bad = [r for r in deny if r in ('Bash(git push --force*)', 'Bash(git push * --force*)')]
sys.exit(1 if bad else 0)
"
}

@test "force-with-lease on role branches is allowed" {
  python3 -c "
import json, sys
d = json.load(open('$SETTINGS'))
allow = d.get('permissions', {}).get('allow', [])
matches = [r for r in allow if 'force-with-lease' in r]
sys.exit(0 if matches else 1)
"
}

@test "force-push to main is still denied" {
  python3 -c "
import json, sys
d = json.load(open('$SETTINGS'))
deny = d.get('permissions', {}).get('deny', [])
main_guards = [r for r in deny if 'force' in r and 'main' in r]
sys.exit(0 if main_guards else 1)
"
}
