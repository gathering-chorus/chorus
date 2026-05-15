#!/usr/bin/env bats
# #2925 AC1 — Permission allow for daemon-runtime deploy commands.
# Verifies the user's ~/.claude/settings.json carries explicit allow entries
# for chorus-deploy + deploy-daemon-card.sh, so the Claude Code classifier
# stops gating them (the Bash(*) broad pattern is overridden by the semantic
# classifier for consequential commands — empirical, kade's 2026-05-15T00:05
# spine entry).

SETTINGS="$HOME/.claude/settings.json"

@test "settings.json exists" {
  [ -f "$SETTINGS" ]
}

@test "settings.json is valid JSON" {
  python3 -c "import json; json.load(open('$SETTINGS'))"
}

@test "allow contains explicit chorus-deploy entry" {
  python3 -c "
import json,sys
d=json.load(open('$SETTINGS'))
allow=d.get('permissions',{}).get('allow',[])
matches=[a for a in allow if 'chorus-deploy' in a]
sys.exit(0 if matches else 1)
"
}

@test "allow contains explicit deploy-daemon-card.sh entry" {
  python3 -c "
import json,sys
d=json.load(open('$SETTINGS'))
allow=d.get('permissions',{}).get('allow',[])
matches=[a for a in allow if 'deploy-daemon-card' in a]
sys.exit(0 if matches else 1)
"
}
