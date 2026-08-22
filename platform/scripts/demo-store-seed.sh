#!/usr/bin/env bash
# demo-store-seed.sh (#3381 D1(b)) — seed a per-werk demo index.db from prod.
#
# The design gather with Kade (2026-08-22) chose (b) fixture-from-prod over an
# empty schema: the seed is GENERATED from prod by THIS versioned script
# (regenerable, never hand-curated — hand fixtures rot), and the SAME seed feeds
# runner tests deterministically.
#
# Two parts:
#   1. SCHEMA — replicate prod's full schema into the demo db, so the variant
#      boots against every table/index/trigger it expects (chorus-api only
#      self-ensures rcas+traces; the rest must pre-exist or reads throw).
#   2. DATA   — a BOUNDED, deterministic recent slice (ORDER BY rowid DESC LIMIT
#      N per table) so the demo shows real data without copying prod's 2M rows
#      (the 2026-06-12 wedge root). Best-effort per table (virtual/FTS tables
#      skip cleanly).
#
# This runs at env_up PROVISION time and reads prod read-only. It is NOT the
# running variant opening prod — the variant, once booted, opens only the demo
# db (AC3: zero prod-store opens by the variant). Idempotent: rebuilds the demo
# db from scratch each call.
#
# Usage: demo-store-seed.sh <demo_db_path> [prod_db_path] [row_limit]
set -euo pipefail

DEMO_DB="${1:?usage: demo-store-seed.sh <demo_db_path> [prod_db] [limit]}"
PROD_DB="${2:-$HOME/.chorus/index.db}"
LIMIT="${3:-200}"

if ! command -v sqlite3 >/dev/null 2>&1; then
  echo "demo-store-seed: sqlite3 not found" >&2; exit 3
fi
if [ ! -r "$PROD_DB" ]; then
  echo "demo-store-seed: prod db not readable at $PROD_DB" >&2; exit 4
fi

mkdir -p "$(dirname "$DEMO_DB")"
rm -f "$DEMO_DB" "$DEMO_DB-wal" "$DEMO_DB-shm"

# 1) schema — every table/index/trigger, empty. Deterministic (prod .schema).
sqlite3 "$PROD_DB" .schema | sqlite3 "$DEMO_DB"

# 2) bounded recent slice, best-effort per real (non-virtual) table.
tables=$(sqlite3 "$PROD_DB" \
  "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%';")
copied=0
for t in $tables; do
  # FTS shadow tables (name_data, name_idx, …) and virtual tables reject a plain
  # INSERT SELECT — the || true drops them; the base rows still seed via the
  # content table. rowid-desc keeps the newest N and stays deterministic.
  if sqlite3 "$DEMO_DB" \
      "ATTACH '$PROD_DB' AS prod; INSERT INTO \"$t\" SELECT * FROM prod.\"$t\" ORDER BY rowid DESC LIMIT $LIMIT; DETACH prod;" \
      2>/dev/null; then
    copied=$((copied+1))
  fi
done

echo "demo-store-seed: $DEMO_DB seeded (schema + <=$LIMIT rows across $copied tables) from $PROD_DB"
