#!/usr/bin/env bash
# MIGRATE: TypeScript P3 — DEC-100 (no bash APIs)
# chat.sh — Lightweight role-to-role chat channel
#
# Usage:
#   chat.sh start <from-role> <to-role> "<topic>"   Create chat, return chat ID
#   chat.sh say <chat-id> <role> "message"           Append message
#   chat.sh read <chat-id> [--since <line>]          Read new lines (default: all)
#   chat.sh end <chat-id>                            Close chat, return transcript
#   chat.sh active                                   List active chats

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
CHAT_DIR="/tmp/chorus-chat"
CHORUS_LOG="${SCRIPT_DIR}/chorus-log"
MESSAGING_API="http://localhost:3475"
# --dry-run: persist messages but skip nudge delivery (osascript inject)
CHAT_DRY_RUN="${CHAT_DRY_RUN:-}"

mkdir -p "$CHAT_DIR"

die() { echo "ERROR: $1" >&2; exit 1; }

cmd_start() {
  local from="${1:-}" to="${2:-}" topic="${3:-chat}"
  [ -z "$from" ] || [ -z "$to" ] && die "Usage: chat.sh start <from-role> <to-role> \"topic\""

  local chat_id="${from}-${to}-$(date +%s)"
  local chat_file="$CHAT_DIR/${chat_id}.md"

  cat > "$chat_file" <<EOF
# Chat: ${topic}
**Between:** ${from} ↔ ${to}
**Started:** $(TZ=America/New_York date '+%Y-%m-%d %H:%M')
---
EOF

  # Track active chat
  echo "${chat_id}|${from}|${to}|${topic}|$(date +%s)" >> "$CHAT_DIR/active.txt"

  # Persist to messaging tier
  curl -s -X POST "$MESSAGING_API/api/chat/start" -H 'Content-Type: application/json' \
    -d "{\"roleA\":\"$from\",\"roleB\":\"$to\",\"topic\":\"$topic\"}" > /dev/null 2>&1 &

  # Emit spine event
  [ -x "$CHORUS_LOG" ] && "$CHORUS_LOG" "chat.started" "$from" "with=$to,topic=$topic,id=$chat_id" 2>/dev/null || true

  echo "$chat_id"
}

cmd_say() {
  local chat_id="${1:-}" role="${2:-}" message="${3:-}"
  [ -z "$chat_id" ] || [ -z "$role" ] || [ -z "$message" ] && die "Usage: chat.sh say <chat-id> <role> \"message\""

  local chat_file="$CHAT_DIR/${chat_id}.md"
  [ -f "$chat_file" ] || die "Chat $chat_id not found"

  local ts
  ts=$(TZ=America/New_York date '+%H:%M')
  printf '\n**[%s] %s:** %s' "$ts" "$role" "$message" >> "$chat_file"

  # Persist to messaging tier
  curl -s -X POST "$MESSAGING_API/api/chat/$chat_id/message" -H 'Content-Type: application/json' \
    -d "{\"from\":\"$role\",\"content\":\"$(echo "$message" | sed 's/"/\\"/g')\"}" > /dev/null 2>&1 &

  # Auto-nudge the other party. Script owns delivery — /chat skill must NOT also nudge.
  # Restored from pre-8bc9f0d9. Prior bug (#1811) was double-delivery from both paths nudging.
  local role1 role2 other
  role1=$(echo "$chat_id" | cut -d- -f1)
  role2=$(echo "$chat_id" | cut -d- -f2)
  if [ "$role" = "$role1" ]; then
    other="$role2"
  else
    other="$role1"
  fi
  if [ -z "$CHAT_DRY_RUN" ]; then
    "$SCRIPT_DIR/nudge" "$other" "${role}: replied in chat ${chat_id}" 2>/dev/null || true
  fi

  # Return current line count so caller can track position
  wc -l < "$chat_file" | tr -d ' '
}

cmd_read() {
  local chat_id="${1:-}" since=0
  [ -z "$chat_id" ] && die "Usage: chat.sh read <chat-id> [--since <line>]"
  shift

  while [ $# -gt 0 ]; do
    case "$1" in
      --since) shift; since="${1:-0}" ;;
    esac
    shift
  done

  local chat_file="$CHAT_DIR/${chat_id}.md"
  [ -f "$chat_file" ] || die "Chat $chat_id not found"

  if [ "$since" -gt 0 ]; then
    tail -n "+$((since + 1))" "$chat_file"
  else
    cat "$chat_file"
  fi
}

cmd_end() {
  local chat_id="${1:-}"
  [ -z "$chat_id" ] && die "Usage: chat.sh end <chat-id>"

  local chat_file="$CHAT_DIR/${chat_id}.md"
  [ -f "$chat_file" ] || die "Chat $chat_id not found"

  local ts
  ts=$(TZ=America/New_York date '+%H:%M')
  printf '\n\n---\n**Chat ended:** %s\n' "$ts" >> "$chat_file"

  # Remove from active list
  if [ -f "$CHAT_DIR/active.txt" ]; then
    grep -v "^${chat_id}|" "$CHAT_DIR/active.txt" > "$CHAT_DIR/active.tmp" 2>/dev/null || true
    mv "$CHAT_DIR/active.tmp" "$CHAT_DIR/active.txt"
  fi

  # Emit spine event
  local lines
  lines=$(wc -l < "$chat_file" | tr -d ' ')
  [ -x "$CHORUS_LOG" ] && "$CHORUS_LOG" "chat.ended" "system" "id=$chat_id,lines=$lines" 2>/dev/null || true

  echo "Chat $chat_id ended ($lines lines). Transcript: $chat_file"
}

cmd_active() {
  if [ ! -f "$CHAT_DIR/active.txt" ] || [ ! -s "$CHAT_DIR/active.txt" ]; then
    echo "No active chats"
    return 0
  fi
  echo "Active chats:"
  while IFS='|' read -r id from to topic started; do
    local age=$(( $(date +%s) - started ))
    echo "  $id — $from ↔ $to — $topic (${age}s ago)"
  done < "$CHAT_DIR/active.txt"
}

# --- Main ---
CMD="${1:-help}"
shift 2>/dev/null || true

case "$CMD" in
  start)  cmd_start "$@" ;;
  say)    cmd_say "$@" ;;
  read)   cmd_read "$@" ;;
  end)    cmd_end "$@" ;;
  active) cmd_active ;;
  help|*)
    echo "chat.sh — Lightweight role-to-role chat"
    echo ""
    echo "  start <from> <to> \"topic\"    Create chat"
    echo "  say <id> <role> \"message\"    Send message"
    echo "  read <id> [--since N]        Read new lines"
    echo "  end <id>                     Close chat"
    echo "  active                       List active chats"
    ;;
esac
