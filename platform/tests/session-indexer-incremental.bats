#!/usr/bin/env bats
# session-indexer-incremental.bats — #2455
# Tests that active sessions (jsonl being appended to) continue to accumulate
# indexed rows on every indexer run. The bug: line_num resets to 0 on each run
# so source_ids collide with prior rows and INSERT OR IGNORE silently drops.

INDEXER="$HOME/.chorus/scripts/chorus-index.sh"
INIT_DB="/Users/jeffbridwell/CascadeProjects/chorus/designing/index/chorus-init-db.sh"

setup() {
  TMPROOT="$(mktemp -d)"
  export CHORUS_DB="${TMPROOT}/index.db"
  export CHORUS_CLAUDE_PROJECTS="${TMPROOT}/projects"
  mkdir -p "${CHORUS_CLAUDE_PROJECTS}/-Users-jeffbridwell-CascadeProjects-chorus-roles-silas"
  bash "$INIT_DB" >/dev/null 2>&1
  SESSION_ID="aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee"
  JSONL="${CHORUS_CLAUDE_PROJECTS}/-Users-jeffbridwell-CascadeProjects-chorus-roles-silas/${SESSION_ID}.jsonl"
}

teardown() {
  rm -rf "$TMPROOT"
}

# Emit one user record (valid shape per Claude Code session log)
emit_user() {
  local uuid="$1" ; local text="$2" ; local ts="$3"
  printf '{"type":"user","uuid":"%s","sessionId":"%s","timestamp":"%s","message":{"role":"user","content":"%s"}}\n' \
    "$uuid" "$SESSION_ID" "$ts" "$text" >> "$JSONL"
}

emit_assistant() {
  local uuid="$1" ; local text="$2" ; local ts="$3"
  printf '{"type":"assistant","uuid":"%s","sessionId":"%s","timestamp":"%s","message":{"role":"assistant","content":[{"type":"text","text":"%s"}]}}\n' \
    "$uuid" "$SESSION_ID" "$ts" "$text" >> "$JSONL"
}

count_session_rows() {
  sqlite3 "$CHORUS_DB" "SELECT COUNT(*) FROM messages WHERE source='claude' AND session_id='$SESSION_ID';"
}

# --- AC: incremental indexing captures appended rows ---

@test "initial index captures all 5 records in a fresh session" {
  emit_user     "u-1" "first from jeff"            "2026-04-23T16:00:00Z"
  emit_assistant "a-1" "first from silas"            "2026-04-23T16:00:01Z"
  emit_user     "u-2" "followup question from jeff" "2026-04-23T16:00:10Z"
  emit_assistant "a-2" "detailed answer from silas"  "2026-04-23T16:00:11Z"
  emit_user     "u-3" "thanks — keep going"          "2026-04-23T16:00:20Z"

  bash "$INDEXER" sessions >/dev/null 2>&1
  count=$(count_session_rows)
  [ "$count" -eq 5 ]
}

@test "second index after append captures the new records" {
  emit_user     "u-1" "first from jeff"            "2026-04-23T16:00:00Z"
  emit_assistant "a-1" "first from silas"            "2026-04-23T16:00:01Z"
  emit_user     "u-2" "followup from jeff"          "2026-04-23T16:00:10Z"
  bash "$INDEXER" sessions >/dev/null 2>&1

  first=$(count_session_rows)
  [ "$first" -eq 3 ]

  emit_assistant "a-2" "another silas reply"         "2026-04-23T16:05:00Z"
  emit_user     "u-3" "a new jeff message"          "2026-04-23T16:05:10Z"
  emit_assistant "a-3" "and silas replies again"     "2026-04-23T16:05:11Z"

  bash "$INDEXER" sessions >/dev/null 2>&1
  second=$(count_session_rows)
  [ "$second" -eq 6 ]
}

@test "idempotent: re-running without appending produces no new rows" {
  emit_user     "u-1" "only message"               "2026-04-23T16:00:00Z"
  emit_assistant "a-1" "only reply"                 "2026-04-23T16:00:01Z"
  bash "$INDEXER" sessions >/dev/null 2>&1
  bash "$INDEXER" sessions >/dev/null 2>&1
  bash "$INDEXER" sessions >/dev/null 2>&1
  count=$(count_session_rows)
  [ "$count" -eq 2 ]
}

@test "user messages get role=jeff, assistant messages get session role" {
  emit_user     "u-1" "a question of ten characters"          "2026-04-23T16:00:00Z"
  emit_assistant "a-1" "an answer of at least ten characters"  "2026-04-23T16:00:01Z"
  bash "$INDEXER" sessions >/dev/null 2>&1
  user_role=$(sqlite3 "$CHORUS_DB" "SELECT role FROM messages WHERE session_id='$SESSION_ID' AND author='user';")
  asst_role=$(sqlite3 "$CHORUS_DB" "SELECT role FROM messages WHERE session_id='$SESSION_ID' AND author='assistant';")
  [ "$user_role" = "jeff" ]
  [ "$asst_role" = "silas" ]
}
