#!/usr/bin/env bats
# @test-type: integration — exercises the real demo-store-seed.sh against a
# synthetic source db (hermetic; never reads prod ~/.chorus/index.db).
# #3381 D1(b) — the seed must (1) replicate prod's full schema so the variant
# boots against every table, and (2) BOUND the data so it never copies prod's
# 2M-row store (the 2026-06-12 wedge). Both are proven here, and the bound is
# proven the #3734 way: a source with MORE rows than the limit must leave the
# demo capped — a seed that copied everything would fail test 3.

SEED="${BATS_TEST_DIRNAME}/../scripts/demo-store-seed.sh"
# Covers: platform/scripts/demo-store-seed.sh  (literal path for the #3917 linker)

setup() {
  command -v sqlite3 >/dev/null 2>&1 || skip "sqlite3 not installed"
  SRC="$BATS_TEST_TMPDIR/src.db"
  DEMO="$BATS_TEST_TMPDIR/demo/index.db"
  # cards uses AUTOINCREMENT so the source .schema emits `CREATE TABLE
  # sqlite_sequence` — the exact real-prod shape that broke the live seed (sqlite
  # refuses to re-create that internal table). The synthetic fixture must carry it
  # or the negative proof is hollow.
  sqlite3 "$SRC" "CREATE TABLE cards(id INTEGER PRIMARY KEY AUTOINCREMENT, title TEXT); CREATE TABLE streams(id INTEGER PRIMARY KEY, ev TEXT); CREATE INDEX ix ON cards(title); INSERT INTO cards(title) SELECT 'c'||value FROM (WITH RECURSIVE n(value) AS (SELECT 1 UNION ALL SELECT value+1 FROM n WHERE value<500) SELECT value FROM n); INSERT INTO streams(ev) VALUES('a'),('b');"
}

@test "NEGATIVE PROOF: an AUTOINCREMENT source (sqlite_sequence in .schema) seeds cleanly" {
  # This is the live #3381 failure: a real prod db has AUTOINCREMENT tables, so
  # .schema emits `CREATE TABLE sqlite_sequence`, which sqlite refuses on import
  # → the seed aborted with "Parse error ... reserved for internal use" (exit 1).
  # Without the sed filter this test reds; with it, green, and sqlite_sequence
  # still auto-exists in the demo after the inserts.
  run bash "$SEED" "$DEMO" "$SRC" 200
  [ "$status" -eq 0 ]
  [ "$(sqlite3 "$DEMO" "SELECT count(*) FROM sqlite_master WHERE name='sqlite_sequence';")" -eq 1 ]
}

@test "schema is fully replicated — every source table exists in the demo" {
  run bash "$SEED" "$DEMO" "$SRC" 200
  [ "$status" -eq 0 ]
  local src_t demo_t
  src_t=$(sqlite3 "$SRC" "SELECT count(*) FROM sqlite_master WHERE type='table';")
  demo_t=$(sqlite3 "$DEMO" "SELECT count(*) FROM sqlite_master WHERE type='table';")
  [ "$src_t" -eq "$demo_t" ]
}

@test "a small table copies entirely (streams: 2 rows <= limit)" {
  bash "$SEED" "$DEMO" "$SRC" 200
  [ "$(sqlite3 "$DEMO" "SELECT count(*) FROM streams;")" -eq 2 ]
}

@test "NEGATIVE PROOF: a table larger than the limit is BOUNDED, not fully copied" {
  # src cards has 500 rows; limit 200. A seed that copied everything (no bound)
  # would put 500 here and this assertion would fail — that is the proof the
  # LIMIT clause is load-bearing, not decorative.
  bash "$SEED" "$DEMO" "$SRC" 200
  local n; n=$(sqlite3 "$DEMO" "SELECT count(*) FROM cards;")
  [ "$n" -eq 200 ]
  [ "$n" -lt 500 ]
}

@test "isolation: the demo db is a separate file; seeding never mutates the source" {
  local before after
  before=$(sqlite3 "$SRC" "SELECT count(*) FROM cards;")
  bash "$SEED" "$DEMO" "$SRC" 50
  after=$(sqlite3 "$SRC" "SELECT count(*) FROM cards;")
  [ "$before" -eq 500 ]
  [ "$after" -eq 500 ]
  [ -f "$DEMO" ]
  [ "$DEMO" != "$SRC" ]
}

@test "REFUSE: missing source db is a typed non-zero exit, never a silent empty demo" {
  run bash "$SEED" "$DEMO" "$BATS_TEST_TMPDIR/nope.db" 200
  [ "$status" -ne 0 ]
}

@test "seeded db is WAL — a readonly handle's journal_mode=WAL pragma is a no-op, not a write" {
  bash "$SEED" "$DEMO" "$SRC" 50
  [ "$(sqlite3 "$DEMO" 'PRAGMA journal_mode;')" = "wal" ]
  # the exact chorus-api boot shape (server.ts:1236): readonly open + WAL pragma.
  # Against a delete-mode db this is a write → SQLITE_READONLY (the run -11 crash);
  # against the WAL-seeded db it must succeed.
  run sqlite3 "file:$DEMO?mode=ro" "PRAGMA journal_mode=WAL;"
  [ "$status" -eq 0 ]
  [ "$output" = "wal" ]
}
