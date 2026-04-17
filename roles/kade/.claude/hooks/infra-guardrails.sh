#!/bin/bash
# Infrastructure Guardrails Hook — PreToolUse on Bash
#
# Blocks prohibited infrastructure commands and redirects to correct tools.
# This enforces rules from engineer/CLAUDE.md and ADR-011.
#
# Prohibited commands and their correct alternatives:
#   kill/pkill     → Use app-state.sh stop
#   git commit     → Use git-queue.sh (serializes cross-role commits)
#   git add        → Use git-queue.sh (atomic add+commit)
#
# Docker blocks removed 2026-04-17 (#2119): Docker is retired per #2020.
# Services run as native LaunchAgents; no docker commands reach production.

set -euo pipefail

CHORUS_ROOT="${CHORUS_ROOT:-/Users/jeffbridwell/CascadeProjects/chorus}"
CHORUS_LOG="$CHORUS_ROOT/platform/scripts/chorus-log"

INPUT=$(cat)
COMMAND=$(echo "$INPUT" | jq -r '.tool_input.command // empty')

# If no command, allow
if [ -z "$COMMAND" ]; then
  exit 0
fi

# Log guardrail events via chorus-log (schema-validated)
log_guardrail() {
  local decision="$1" pattern="$2"
  local cmd_truncated
  cmd_truncated=$(echo "$COMMAND" | head -c 200 | sed 's/"/\\"/g')
  "$CHORUS_LOG" guard.rule.decided system "decision=$decision" "pattern=$pattern" "command=$cmd_truncated" 2>/dev/null || true
}

deny() {
  local reason="$1"
  local pattern="${2:-unknown}"
  log_guardrail "deny" "$pattern"
  jq -n \
    --arg reason "$reason" \
    '{
      hookSpecificOutput: {
        hookEventName: "PreToolUse",
        permissionDecision: "deny",
        permissionDecisionReason: $reason
      }
    }'
  exit 0
}

ask() {
  local reason="$1"
  local pattern="${2:-unknown}"
  log_guardrail "ask" "$pattern"
  jq -n \
    --arg reason "$reason" \
    '{
      hookSpecificOutput: {
        hookEventName: "PreToolUse",
        permissionDecision: "ask",
        permissionDecisionReason: $reason
      }
    }'
  exit 0
}

# ── BLOCKED: kill / pkill / killall (use app-state.sh) ──
if echo "$COMMAND" | grep -qE '\b(kill|pkill|killall)\s'; then
  deny "BLOCKED: Manual process killing is prohibited. Use app-state.sh stop for graceful shutdown. Manual PID killing causes orphaned processes, port conflicts, and cascading failures. If app-state.sh can't stop a process, that's a bug to fix in the script." "kill"
fi

# ── BLOCKED: git commit / git add in team repo only (use git-queue.sh) ──
# git-queue.sh serializes commits via lockf to prevent cross-role staging collisions.
# Only applies to the team monorepo (CascadeProjects/) — app repo has its own git index.
TEAM_REPO_ROOT="/Users/jeffbridwell/CascadeProjects"
if echo "$COMMAND" | grep -qE '\bgit\s+(commit|add)\b'; then
  # Skip if git appears only inside a heredoc (e.g., cat <<'EOF' ... git commit ... EOF)
  if echo "$COMMAND" | grep -qE "<<['\"]?EOF"; then
    log_guardrail "allow" "git-in-heredoc"
  else
    # Determine the effective working directory for the git command.
    # The hook's own CWD may differ from the command's target (e.g., cd /app/repo && git add).
    CMD_DIR=""
    if echo "$COMMAND" | grep -qE '^\s*cd\s+'; then
      CMD_DIR=$(echo "$COMMAND" | sed -n "s|^[[:space:]]*cd[[:space:]][[:space:]]*\([^ &;]*\).*|\1|p" | sed "s|~|$HOME|")
    fi
    if [ -n "$CMD_DIR" ] && [ -d "$CMD_DIR" ]; then
      GIT_ROOT=$(git -C "$CMD_DIR" rev-parse --show-toplevel 2>/dev/null || echo "")
    else
      GIT_ROOT=$(git rev-parse --show-toplevel 2>/dev/null || echo "")
    fi
    if [ "$GIT_ROOT" = "$TEAM_REPO_ROOT" ]; then
      if echo "$COMMAND" | grep -qE '\bgit\s+commit\b'; then
        deny "BLOCKED: Direct git commit is prohibited in the team repo. Use git-queue.sh which serializes commits across roles with lockf. Three roles share one git index — raw commits cause staging collisions. Path: ../../scripts/git-queue.sh" "git-commit"
      fi
      if echo "$COMMAND" | grep -qE '\bgit\s+add\b'; then
        deny "BLOCKED: Direct git add is prohibited in the team repo. Use git-queue.sh which performs atomic add+commit under lock. Path: ../../scripts/git-queue.sh" "git-add"
      fi
    fi
  fi
  # Outside team repo — allow normal git operations
fi

# ── ASK: terraform commands (should go through app-state.sh) ──
if echo "$COMMAND" | grep -qE '\bterraform\s+(apply|destroy)\b'; then
  ask "Direct terraform apply/destroy detected. These should go through app-state.sh which wraps Terraform with health checks and verification. Is there a reason to bypass app-state.sh?" "terraform-direct"
fi

# Allow everything else
exit 0
