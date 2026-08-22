#!/bin/bash
# e2e-responder.sh — Auto-respond to [e2e-test] nudges via Clearing API (#1936)
# Runs as UserPromptSubmit hook. Reads prompt from stdin, responds if e2e marker found.
set -euo pipefail

INPUT=$(cat)
PROMPT=$(echo "$INPUT" | python3 -c "import json,sys; print(json.load(sys.stdin).get('prompt',''))" 2>/dev/null || echo "")

if echo "$PROMPT" | grep -q '\[e2e-test\]'; then
  ROLE="${DEPLOY_ROLE:-unknown}"
  MARKER=$(echo "$PROMPT" | grep -oE 'e2e-[a-z]+-[0-9]+' | head -1)
  if [ -n "$MARKER" ]; then
    # #3966 — /api/message requires a caller identity; present the BRIDGE_TOKEN
    # (server path os.homedir()/.chorus/bridge-auth-token, not $CHORUS_HOME).
    E2E_BRIDGE_TOKEN="$(cat "$HOME/.chorus/bridge-auth-token" 2>/dev/null || echo '')"
    curl -s -X POST http://localhost:3470/api/message \
      -H 'Content-Type: application/json' \
      -H "Authorization: Bearer ${E2E_BRIDGE_TOKEN}" \
      -d "{\"from\":\"${ROLE}\",\"text\":\"[e2e-ack] ${ROLE} received ${MARKER}\",\"type\":\"role-response\"}" \
      > /dev/null 2>&1 || true
  fi
fi

# Always allow — this is observation only
echo '{"hookSpecificOutput":{}}'
