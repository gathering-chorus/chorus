#!/bin/bash
# Infrastructure Guardrails Hook — PreToolUse on Bash
#
# Blocks prohibited infrastructure commands and redirects to correct tools.
# This enforces rules from engineer/CLAUDE.md and ADR-011.
#
# Prohibited commands and their correct alternatives:
#   docker exec    → Fix code and redeploy (ADR-011). Read-only inspection: ask Jeff first.
#   docker logs    → Use Loki via Grafana (http://localhost:3100 → Explore → Loki)
#   kill/pkill     → Use app-state.sh stop
#   docker stop    → Use app-state.sh stop
#   docker rm      → Use app-state.sh stop (Terraform manages containers)
#   docker restart → Use app-state.sh restart
#   git commit     → Use git-queue.sh (serializes cross-role commits)
#   git add        → Use git-queue.sh (atomic add+commit)

set -euo pipefail

CHORUS_LOG="/Users/jeffbridwell/CascadeProjects/platform/scripts/chorus-log"

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

# ── BLOCKED: docker exec (no modifying running containers) ──
# ADR-011: "No more docker exec to fix things. If it's broken, fix the code and deploy."
# Exception: read-only inspection requires Jeff's explicit approval.
if echo "$COMMAND" | grep -qE '\bdocker\s+exec\b'; then
  deny "BLOCKED: docker exec is prohibited (ADR-011). Fix the code and redeploy using app-state.sh deploy. If you need read-only inspection for debugging, ask Jeff for permission first." "docker-exec"
fi

# ── BLOCKED: docker logs (use Loki) ──
# Escape hatch: --tail N (≤50 lines, no --follow/-f) allowed for crash diagnostics
# when container dies before Promtail can scrape.
if echo "$COMMAND" | grep -qE '\bdocker\s+logs\b'; then
  # Block --follow / -f (streaming is never needed)
  if echo "$COMMAND" | grep -qE '\s(-f|--follow)\b'; then
    deny "BLOCKED: docker logs --follow is prohibited. Use Loki for real-time log streaming. For crash diagnostics, use: docker logs --tail 20 <container>" "docker-logs-follow"
  fi
  # Allow --tail N where N ≤ 50 (read-only crash diagnostics)
  TAIL_N=$(echo "$COMMAND" | sed -n 's/.*--tail[= ]\([0-9][0-9]*\).*/\1/p')
  if [ -n "$TAIL_N" ] && [ "$TAIL_N" -le 50 ] 2>/dev/null; then
    log_guardrail "allow" "docker-logs-tail"
    exit 0
  fi
  # Block all other docker logs usage
  deny "BLOCKED: docker logs without --tail is prohibited. Use Loki for log search: Grafana at http://localhost:3100 → Explore → Loki. For crash diagnostics (container died before Promtail scraped): docker logs --tail 20 <container>" "docker-logs"
fi

# ── BLOCKED: kill / pkill / killall (use app-state.sh) ──
if echo "$COMMAND" | grep -qE '\b(kill|pkill|killall)\s'; then
  deny "BLOCKED: Manual process killing is prohibited. Use app-state.sh stop for graceful shutdown. Manual PID killing causes orphaned processes, port conflicts, and cascading failures. If app-state.sh can't stop a process, that's a bug to fix in the script." "kill"
fi

# ── BLOCKED: docker stop / docker rm / docker restart (use app-state.sh) ──
if echo "$COMMAND" | grep -qE '\bdocker\s+(stop|rm|restart|kill)\b'; then
  deny "BLOCKED: Direct Docker lifecycle commands are prohibited. Use app-state.sh (start|stop|restart|status) which manages containers through docker-compose. Direct Docker commands bypass the deployment pipeline and create drift." "docker-lifecycle"
fi

# ── BLOCKED: docker compose down / docker-compose down ──
if echo "$COMMAND" | grep -qE '\bdocker[\s-]compose\s+down\b'; then
  deny "BLOCKED: docker compose down is prohibited. Use app-state.sh stop which manages graceful teardown." "docker-compose-down"
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
        deny "BLOCKED: Direct git commit is prohibited in the team repo. Use git-queue.sh which serializes commits across roles with lockf. Three roles share one git index — raw commits cause staging collisions. Path: ../messages/scripts/git-queue.sh" "git-commit"
      fi
      if echo "$COMMAND" | grep -qE '\bgit\s+add\b'; then
        deny "BLOCKED: Direct git add is prohibited in the team repo. Use git-queue.sh which performs atomic add+commit under lock. Path: ../messages/scripts/git-queue.sh" "git-add"
      fi
    fi
  fi
  # Outside team repo — allow normal git operations
fi

# ── ASK: docker run (should use app-state.sh deploy or Terraform) ──
if echo "$COMMAND" | grep -qE '\bdocker\s+run\b'; then
  ask "docker run detected. Containers should be managed through app-state.sh and Terraform, not run directly. Is this a temporary test container? If so, Jeff must approve." "docker-run"
fi

# ── ASK: terraform commands (should go through app-state.sh) ──
if echo "$COMMAND" | grep -qE '\bterraform\s+(apply|destroy)\b'; then
  ask "Direct terraform apply/destroy detected. These should go through app-state.sh which wraps Terraform with health checks and verification. Is there a reason to bypass app-state.sh?" "terraform-direct"
fi

# Allow everything else
exit 0
