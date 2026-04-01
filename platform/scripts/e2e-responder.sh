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
    curl -s -X POST http://localhost:3470/api/message \
      -H 'Content-Type: application/json' \
      -d "{\"from\":\"${ROLE}\",\"text\":\"[e2e-ack] ${ROLE} received ${MARKER}\",\"type\":\"role-response\"}" \
      > /dev/null 2>&1 || true
  fi
fi

# Always allow — this is observation only
echo '{"hookSpecificOutput":{}}'
