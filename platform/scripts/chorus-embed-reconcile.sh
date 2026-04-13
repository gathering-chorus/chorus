#!/bin/bash
# chorus-embed-reconcile.sh — restore embedded flags from LanceDB state (#1978)
# Silas reset 289K embedded flags by accident. LanceDB still has the vectors
# with msg_ids. This script reads LanceDB msg_ids and re-marks them in SQLite.
#
# ALWAYS run with --dry-run first. No writes without seeing the numbers.

set -euo pipefail

DB_PATH="${HOME}/.chorus/index.db"
API="${CHORUS_API:-http://localhost:3340}"
MODE="${1:---dry-run}"

if [ "$MODE" != "--dry-run" ] && [ "$MODE" != "--execute" ]; then
  echo "Usage: $0 [--dry-run|--execute]"
  echo "  --dry-run   Show what would change (default)"
  echo "  --execute   Actually update SQLite"
  exit 1
fi

echo "=== Embed Reconcile ==="
echo "Mode: $MODE"
echo ""

echo "--- Current SQLite state ---"
sqlite3 "$DB_PATH" "SELECT source, SUM(CASE WHEN embedded=0 THEN 1 ELSE 0 END) as unembedded, SUM(CASE WHEN embedded=1 THEN 1 ELSE 0 END) as embedded, COUNT(*) as total FROM messages WHERE LENGTH(content) >= 100 GROUP BY source ORDER BY total DESC"

echo ""

LANCE_COUNT=$(curl -sf "${API}/api/chorus/health" 2>/dev/null | python3 -c "import sys,json; print(json.load(sys.stdin).get('vectors',0))" 2>/dev/null || echo "0")
echo "LanceDB vectors: ${LANCE_COUNT}"

UNEMBEDDED=$(sqlite3 "$DB_PATH" "SELECT COUNT(*) FROM messages WHERE embedded = 0 AND LENGTH(content) >= 100")
echo "SQLite unembedded: ${UNEMBEDDED}"

HIGHEST_EMBEDDED=$(sqlite3 "$DB_PATH" "SELECT MAX(id) FROM messages WHERE embedded = 1")
echo "Highest embedded ID in SQLite: ${HIGHEST_EMBEDDED}"

SHOULD_RESTORE=$(sqlite3 "$DB_PATH" "SELECT COUNT(*) FROM messages WHERE embedded = 0 AND id <= ${HIGHEST_EMBEDDED} AND LENGTH(content) >= 100")
echo ""
echo "--- Reconciliation ---"
echo "Messages to restore (id <= ${HIGHEST_EMBEDDED}, currently unembedded): ${SHOULD_RESTORE}"

GENUINELY_NEW=$(sqlite3 "$DB_PATH" "SELECT COUNT(*) FROM messages WHERE embedded = 0 AND id > ${HIGHEST_EMBEDDED} AND LENGTH(content) >= 100")
echo "Genuinely unembedded (id > ${HIGHEST_EMBEDDED}): ${GENUINELY_NEW}"

echo ""
echo "--- Restore breakdown by source ---"
sqlite3 "$DB_PATH" "SELECT source, COUNT(*) FROM messages WHERE embedded = 0 AND id <= ${HIGHEST_EMBEDDED} AND LENGTH(content) >= 100 GROUP BY source ORDER BY COUNT(*) DESC"

if [ "$MODE" = "--execute" ]; then
  echo ""
  echo "--- Executing ---"
  sqlite3 "$DB_PATH" "UPDATE messages SET embedded = 1 WHERE embedded = 0 AND id <= ${HIGHEST_EMBEDDED} AND LENGTH(content) >= 100"
  REMAINING=$(sqlite3 "$DB_PATH" "SELECT COUNT(*) FROM messages WHERE embedded = 0 AND LENGTH(content) >= 100")
  echo "Done. Remaining unembedded: ${REMAINING}"
else
  echo ""
  echo "--- Dry run complete. Run with --execute to apply. ---"
fi
