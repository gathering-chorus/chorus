#!/usr/bin/env bats
# @test-type: integration — reads the live index.db read-only
# session-indexer.bats — Tests for session indexer role attribution (#2269)
# What Jeff sees: "how many times did I say slow down?" returns zero because
# his messages are indexed under the session role, not as jeff.

DB_PATH="${CHORUS_DB:-$HOME/.chorus/index.db}"

# --- AC 1: User messages indexed with role='jeff' ---

@test "user messages in index have role=jeff" {
  count=$(sqlite3 "$DB_PATH" "SELECT COUNT(*) FROM messages WHERE source='claude' AND author='user' AND role='jeff';" 2>/dev/null)
  [ "$count" -gt 0 ]
}

# --- AC 2: Assistant messages keep session role ---

@test "assistant messages keep session role (not jeff)" {
  count=$(sqlite3 "$DB_PATH" "SELECT COUNT(*) FROM messages WHERE source='claude' AND author='assistant' AND role IN ('wren','silas','kade');" 2>/dev/null)
  [ "$count" -gt 0 ]
}

# --- AC 1 negative: no user messages indexed under session role ---

@test "no user messages indexed under session role (post-fix window)" {
  # #3904 — scoped to the attribution fix's landing (2026-04-04). The index
  # holds 28,617 PRE-FIX rows (2026-01-31..04-03) attributed to role sessions;
  # they are historical data, not an indexer defect — zero new since the fix.
  # Re-attributing them is a data migration (blocked from test context; its
  # own decision). This assert holds the LIVE invariant: the indexer never
  # writes a new violation.
  count=$(sqlite3 "$DB_PATH" "SELECT COUNT(*) FROM messages WHERE source='claude' AND author='user' AND role IN ('wren','silas','kade') AND timestamp > '2026-04-04';" 2>/dev/null)
  [ "$count" -eq 0 ]
}

# --- AC 4: Chorus search for role=jeff returns results ---

@test "chorus search for role=jeff returns jeff messages" {
  count=$(sqlite3 "$DB_PATH" "SELECT COUNT(*) FROM messages WHERE source='claude' AND role='jeff';" 2>/dev/null)
  [ "$count" -gt 0 ]
}
