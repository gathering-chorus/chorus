#!/usr/bin/env bash
# MIGRATE: TypeScript P1 — DEC-100 (no bash APIs)
# chorus-query.sh — Query the shared memory index
# Supports: reconcile, search, role, recent
# Part of the Chorus context service (DEC-030, Wren's vertical)
set -euo pipefail

DB_PATH="${CHORUS_DB:-$HOME/.chorus/index.db}"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
CHORUS_INDEX="$SCRIPT_DIR/../../chorus/scripts/chorus-index.sh"
LOCK_FILE="$HOME/.chorus/index.lock"

# Acquire lock — if another instance is indexing, skip to query-only
acquire_lock() {
  if [ -f "$LOCK_FILE" ]; then
    lock_pid=$(cat "$LOCK_FILE" 2>/dev/null)
    # Check if the process holding the lock is still alive
    if [ -n "$lock_pid" ] && kill -0 "$lock_pid" 2>/dev/null; then
      echo "Index update in progress (pid $lock_pid). Using existing index."
      return 1
    else
      # Stale lock — previous process died
      rm -f "$LOCK_FILE"
    fi
  fi
  echo $$ > "$LOCK_FILE"
  return 0
}

release_lock() {
  rm -f "$LOCK_FILE"
}

# Clean up lock on exit
trap release_lock EXIT

if [ ! -f "$DB_PATH" ]; then
  echo "Index not found. Initializing and running first index..."
  bash "$SCRIPT_DIR/chorus-init-db.sh" >/dev/null 2>&1
  if acquire_lock; then
    bash "$CHORUS_INDEX" slack 2>/dev/null
    bash "$CHORUS_INDEX" sessions 2>/dev/null
    release_lock
  fi
fi

MODE="${1:-reconcile}"
shift 2>/dev/null || true
ARGS="$*"

case "$MODE" in
  reconcile)
    # What happened since this role's last session?
    # Detect current role from CWD or argument
    ROLE="${ARGS:-unknown}"
    if [ "$ROLE" = "unknown" ] || [ -z "$ROLE" ]; then
      CWD="$(pwd)"
      case "$CWD" in
        *product-manager*) ROLE="wren" ;;
        *architect*)       ROLE="silas" ;;
        *engineer*)        ROLE="kade" ;;
        *)                 ROLE="unknown" ;;
      esac
    fi

    # First, update the index incrementally (skip if another instance is indexing)
    if acquire_lock; then
      bash "$CHORUS_INDEX" slack 2>/dev/null | grep -v "up to date" || true
      bash "$CHORUS_INDEX" sessions 2>/dev/null | grep -v "^$" || true
      bash "$CHORUS_INDEX" artifacts 2>/dev/null | grep -v "^$" || true
      release_lock
    fi

    python3 -c "
import sqlite3, os, json
from datetime import datetime, timezone, timedelta

db_path = os.environ.get('CHORUS_DB', os.path.expanduser('~/.chorus/index.db'))
conn = sqlite3.connect(db_path)
cur = conn.cursor()

role = '$ROLE'

# Find this role's last session message (approximate session end)
cur.execute('''
    SELECT MAX(timestamp) FROM messages
    WHERE source='claude' AND channel=? AND author='assistant'
''', (f'session:{role}',))
row = cur.fetchone()
last_session_end = row[0] if row and row[0] else None

if not last_session_end:
    # No prior sessions indexed — show last 24h
    cutoff = (datetime.now(timezone.utc) - timedelta(hours=24)).strftime('%Y-%m-%dT%H:%M:%SZ')
    print(f'## Context Reconciliation for {role}')
    print(f'No prior session found in index. Showing last 24 hours.')
    print()
else:
    cutoff = last_session_end
    print(f'## Context Reconciliation for {role}')
    print(f'Since last session ended: {cutoff}')
    print()

# Slack messages since cutoff (excluding this role's bridge messages)
cur.execute('''
    SELECT channel, role, content, timestamp
    FROM messages
    WHERE source='slack' AND timestamp > ? AND NOT (is_bridge=1 AND role=?)
    ORDER BY timestamp ASC
''', (cutoff, role))
slack_msgs = cur.fetchall()

if slack_msgs:
    print(f'### Slack Activity ({len(slack_msgs)} messages)')
    by_channel = {}
    for ch, r, content, ts in slack_msgs:
        by_channel.setdefault(ch, []).append((r, content[:300], ts))
    for ch, msgs in sorted(by_channel.items()):
        print(f'\\n**#{ch}** ({len(msgs)} messages)')
        for r, content, ts in msgs[-5:]:  # Last 5 per channel
            short = content.replace('\\n', ' ')[:150]
            print(f'  [{ts[:16]}] {r}: {short}')
        if len(msgs) > 5:
            print(f'  ... and {len(msgs)-5} more')
else:
    print('### Slack Activity')
    print('No new Slack messages since last session.')

print()

# Other roles' Claude sessions since cutoff
cur.execute('''
    SELECT channel, author, content, timestamp, session_id
    FROM messages
    WHERE source='claude' AND channel != ? AND timestamp > ? AND author='user'
    ORDER BY timestamp ASC
''', (f'session:{role}', cutoff))
other_sessions = cur.fetchall()

if other_sessions:
    sessions_by_role = {}
    for ch, author, content, ts, sid in other_sessions:
        r = ch.replace('session:', '')
        sessions_by_role.setdefault(r, set()).add(sid)
    print(f'### Other Roles\\'s Claude Sessions')
    for r, sids in sorted(sessions_by_role.items()):
        print(f'  **{r}**: {len(sids)} session(s) since your last session')
    print()

    # Show Jeff's messages from other sessions (his direction)
    cur.execute('''
        SELECT channel, content, timestamp
        FROM messages
        WHERE source='claude' AND channel != ? AND timestamp > ? AND author='user'
        ORDER BY timestamp ASC LIMIT 10
    ''', (f'session:{role}', cutoff))
    jeff_msgs = cur.fetchall()
    if jeff_msgs:
        print('### Jeff\\'s Direction (from other sessions)')
        for ch, content, ts in jeff_msgs:
            r = ch.replace('session:', '')
            short = content.replace('\\n', ' ')[:200]
            print(f'  [{ts[:16]}] → {r}: {short}')
        print()
else:
    print('### Other Roles\\'s Claude Sessions')
    print('No other role sessions since your last session.')
    print()

# Summary stats
cur.execute('SELECT COUNT(*) FROM messages')
total = cur.fetchone()[0]
cur.execute('SELECT source, COUNT(*) FROM messages GROUP BY source')
by_source = dict(cur.fetchall())
print(f'### Index Stats')
print(f'Total indexed: {total} messages')
for src, cnt in sorted(by_source.items()):
    print(f'  {src}: {cnt}')

conn.close()
" 2>/dev/null
    ;;

  search)
    if [ -z "$ARGS" ]; then
      echo "Usage: chorus-query.sh search <term>"
      exit 1
    fi

    # Update index first (skip if another instance is indexing)
    if acquire_lock; then
      bash "$CHORUS_INDEX" slack 2>/dev/null | grep -v "up to date" || true
      bash "$CHORUS_INDEX" sessions 2>/dev/null | grep -v "^$" || true
      bash "$CHORUS_INDEX" artifacts 2>/dev/null | grep -v "^$" || true
      release_lock
    fi

    # Try hybrid search via Chorus API (FTS + semantic), fall back to direct SQLite
    CHORUS_API_URL="${CHORUS_API_URL:-http://localhost:3340}"
    ENCODED_Q=$(python3 -c "import urllib.parse, sys; print(urllib.parse.quote(sys.argv[1]))" "$ARGS")
    API_TMP="/tmp/chorus-search-$$.json"
    if curl -sf "${CHORUS_API_URL}/api/chorus/search?q=${ENCODED_Q}&mode=hybrid&limit=20" -o "$API_TMP" 2>/dev/null; then
      python3 - "$API_TMP" "$ARGS" <<'PYEOF'
import json, sys

with open(sys.argv[1]) as f:
    data = json.load(f)
query = sys.argv[2]
mode = data.get('mode', 'fts')
results = data.get('results', [])

print(f'## Search: "{query}" (mode: {mode})')
print()

if results:
    print(f'Found {len(results)} results:')
    print()
    for r in results:
        source = r.get('source', '')
        channel = r.get('channel', '')
        role = r.get('role', '')
        author = r.get('author', '')
        content = r.get('content', '').replace('\n', ' ')[:250]
        ts = r.get('timestamp', '')[:16]
        rrf = r.get('_rrf_score')
        label = f'[{source}] #{channel}' if source == 'slack' else f'[{source}] {channel}'
        score_tag = f' (rrf:{rrf:.4f})' if rrf else ''
        print(f'**{label}** ({ts}) {role}/{author}:{score_tag}')
        print(f'  {content}')
        print()
else:
    print('No results found.')
PYEOF
      rm -f "$API_TMP"
    else
      rm -f "$API_TMP"
      # Fallback: direct SQLite search (API unavailable)
      python3 - "$ARGS" <<'PYEOF'
import sqlite3, os, sys

db_path = os.environ.get('CHORUS_DB', os.path.expanduser('~/.chorus/index.db'))
conn = sqlite3.connect(db_path)
cur = conn.cursor()

query = sys.argv[1]

print(f'## Search: "{query}" (mode: fts-fallback)')
print()

fts_query = query.replace('-', ' ')

cur.execute('''
    SELECT m.source, m.channel, m.role, m.content, m.timestamp, m.author
    FROM messages_fts f
    JOIN messages m ON f.rowid = m.id
    WHERE messages_fts MATCH ?
    ORDER BY m.timestamp DESC
    LIMIT 20
''', (fts_query,))
results = cur.fetchall()

if not results:
    cur.execute('''
        SELECT source, channel, role, content, timestamp, author
        FROM messages
        WHERE content LIKE ?
        ORDER BY timestamp DESC
        LIMIT 20
    ''', (f'%{query}%',))
    results = cur.fetchall()

if results:
    print(f'Found {len(results)} results (showing newest first):')
    print()
    for source, channel, role, content, ts, author in results:
        short = content.replace('\n', ' ')[:250]
        label = f'[{source}] #{channel}' if source == 'slack' else f'[{source}] {channel}'
        print(f'**{label}** ({ts[:16]}) {role}/{author}:')
        print(f'  {short}')
        print()
else:
    print('No results found.')

conn.close()
PYEOF
    fi

    # Touch marker so search-hierarchy-hook knows Chorus was used recently
    touch /tmp/claude-team-scan/chorus-last-search 2>/dev/null || true
    ;;

  role)
    if [ -z "$ARGS" ]; then
      echo "Usage: chorus-query.sh role <name>"
      exit 1
    fi

    python3 -c "
import sqlite3, os

db_path = os.environ.get('CHORUS_DB', os.path.expanduser('~/.chorus/index.db'))
conn = sqlite3.connect(db_path)
cur = conn.cursor()

role = '''$ARGS'''.strip()

print(f'## Activity for {role}')
print()

cur.execute('''
    SELECT source, channel, content, timestamp, author
    FROM messages
    WHERE role = ?
    ORDER BY timestamp DESC
    LIMIT 20
''', (role,))

results = cur.fetchall()
if results:
    print(f'Last {len(results)} messages:')
    print()
    for source, channel, content, ts, author in results:
        short = content.replace('\\n', ' ')[:250]
        label = f'[{source}] #{channel}' if source == 'slack' else f'[{source}] {channel}'
        print(f'**{label}** ({ts[:16]}) {author}:')
        print(f'  {short}')
        print()
else:
    print(f'No messages found for role: {role}')

conn.close()
" 2>/dev/null
    ;;

  stats)
    python3 -c "
import sqlite3, os

db_path = os.environ.get('CHORUS_DB', os.path.expanduser('~/.chorus/index.db'))
conn = sqlite3.connect(db_path)
cur = conn.cursor()

print('## Chorus Index Statistics')
print()

cur.execute('SELECT COUNT(*) FROM messages')
total = cur.fetchone()[0]
print(f'Total messages: {total}')
print()

print('By source:')
cur.execute('SELECT source, COUNT(*) FROM messages GROUP BY source ORDER BY COUNT(*) DESC')
for src, cnt in cur.fetchall():
    print(f'  {src}: {cnt}')
print()

print('By channel:')
cur.execute('SELECT channel, COUNT(*) FROM messages GROUP BY channel ORDER BY COUNT(*) DESC LIMIT 15')
for ch, cnt in cur.fetchall():
    print(f'  {ch}: {cnt}')
print()

print('By role:')
cur.execute('SELECT role, COUNT(*) FROM messages GROUP BY role ORDER BY COUNT(*) DESC')
for role, cnt in cur.fetchall():
    print(f'  {role}: {cnt}')
print()

print('Date range:')
cur.execute('SELECT MIN(timestamp), MAX(timestamp) FROM messages')
mn, mx = cur.fetchone()
print(f'  Oldest: {mn}')
print(f'  Newest: {mx}')
print()

print('Watermarks:')
cur.execute('SELECT source, last_indexed FROM watermarks ORDER BY last_indexed DESC LIMIT 10')
for src, indexed in cur.fetchall():
    print(f'  {src}: last indexed {indexed}')

conn.close()
" 2>/dev/null
    ;;

  tail)
    # Tail a role's recent session activity
    # Usage: chorus-query.sh tail <role> [--lines N] [--follow]
    TAIL_ROLE=""
    TAIL_LINES=20
    TAIL_FOLLOW=false
    for arg in $ARGS; do
      case "$arg" in
        --follow) TAIL_FOLLOW=true ;;
        --lines) :;; # next arg is the count
        [0-9]*) TAIL_LINES="$arg" ;;
        *) TAIL_ROLE="$arg" ;;
      esac
    done

    if [ -z "$TAIL_ROLE" ]; then
      echo "Usage: chorus-query.sh tail <role> [--lines N] [--follow]"
      exit 1
    fi

    tail_query() {
      python3 -c "
import sqlite3, os

db_path = os.environ.get('CHORUS_DB', os.path.expanduser('~/.chorus/index.db'))
conn = sqlite3.connect(db_path)
cur = conn.cursor()

role = '$TAIL_ROLE'
limit = $TAIL_LINES

cur.execute('''
    SELECT author, content, timestamp
    FROM messages
    WHERE source='claude' AND channel=?
    ORDER BY timestamp DESC
    LIMIT ?
''', (f'session:{role}', limit))

results = cur.fetchall()
results.reverse()

for author, content, ts in results:
    short = content.replace('\n', ' ')[:300]
    tag = 'user' if author == 'user' else role
    print(f'[{ts[:19]}] {tag}: {short}')

conn.close()
" 2>/dev/null
    }

    tail_query

    if [ "$TAIL_FOLLOW" = "true" ]; then
      LAST_TS=""
      while true; do
        sleep 5
        NEW=$(python3 -c "
import sqlite3, os

db_path = os.environ.get('CHORUS_DB', os.path.expanduser('~/.chorus/index.db'))
conn = sqlite3.connect(db_path)
cur = conn.cursor()

role = '$TAIL_ROLE'
last = '$LAST_TS'

if last:
    cur.execute('''
        SELECT author, content, timestamp
        FROM messages
        WHERE source='claude' AND channel=? AND timestamp > ?
        ORDER BY timestamp ASC
    ''', (f'session:{role}', last))
else:
    cur.execute('''
        SELECT author, content, timestamp
        FROM messages
        WHERE source='claude' AND channel=?
        ORDER BY timestamp DESC LIMIT 1
    ''', (f'session:{role}',))

results = cur.fetchall()
for author, content, ts in results:
    short = content.replace('\n', ' ')[:300]
    tag = 'user' if author == 'user' else role
    print(f'[{ts[:19]}] {tag}: {short}')

if results:
    print(f'__LAST_TS__={results[-1][2]}')

conn.close()
" 2>/dev/null)

        if [ -n "$NEW" ]; then
          NEW_TS=$(echo "$NEW" | grep '__LAST_TS__=' | sed 's/__LAST_TS__=//')
          echo "$NEW" | grep -v '__LAST_TS__='
          if [ -n "$NEW_TS" ]; then
            LAST_TS="$NEW_TS"
          fi
        fi
      done
    fi
    ;;

  *)
    echo "Usage: chorus-query.sh <mode> [args]"
    echo ""
    echo "Modes:"
    echo "  reconcile [role]  — What happened since your last session (default)"
    echo "  search <term>     — Full-text search across all sources"
    echo "  role <name>       — Recent activity for a specific role"
    echo "  tail <role>       — Tail a role's recent session (--lines N, --follow)"
    echo "  stats             — Index statistics"
    ;;
esac
