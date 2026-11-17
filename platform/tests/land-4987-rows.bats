#!/usr/bin/env bats
# @test-type: unit — runs land-4987-rows against a temp messages.db fixture; never the live store
# @domain: messages
# @card: #4987
# #4987 (AC6): the land rewrites ISO created_at rows to the store's form, with a backup and a proof
#
# Covers: platform/scripts/land-4987-rows

setup() {
  SCRIPT="${BATS_TEST_DIRNAME}/../scripts/land-4987-rows"
  W="$(mktemp -d)"
  export MESSAGES_DB="$W/messages.db" BACKUP_DIR="$W/backups"
  python3 - "$MESSAGES_DB" <<'PY'
import sqlite3, sys
db = sqlite3.connect(sys.argv[1])
db.execute("CREATE TABLE messages (id INTEGER PRIMARY KEY, created_at TEXT, content TEXT)")
db.executemany("INSERT INTO messages (created_at, content) VALUES (?, ?)", [
  ("2026-11-07 00:50:00", "sql"), ("2026-11-07T00:53:24.406Z", "iso z"),
  ("2026-11-06T19:56:59-05:00", "iso offset"), ("2026-11-07 01:00:00", "sql 2")])
db.commit()
PY
}

teardown() { rm -rf "$W"; }

rows() { python3 -c 'import sqlite3,sys; print("|".join(r[0] for r in sqlite3.connect(sys.argv[1]).execute("SELECT created_at FROM messages ORDER BY id")))' "$MESSAGES_DB"; }

@test "AC6: --apply rewrites only the ISO rows to the same instant in the store's form, after a backup" {
  run python3 "$SCRIPT" --apply
  [ "$status" -eq 0 ]
  [[ "$output" == *"DONE — 2 rows rewritten"* ]] || return 1
  [ "$(rows)" = "2026-11-07 00:50:00|2026-11-07 00:53:24|2026-11-07 00:56:59|2026-11-07 01:00:00" ]
  ls "$BACKUP_DIR"/messages-4987-*.db
}

@test "AC6: a second land finds nothing to do (SAME), and the dry run writes nothing" {
  run python3 "$SCRIPT"
  [[ "$output" == *"DRY RUN — 2 of 4 rows"* ]] || return 1
  [[ "$(rows)" == *"T00:53:24.406Z"* ]] || return 1
  python3 "$SCRIPT" --apply
  run python3 "$SCRIPT" --apply
  [ "$status" -eq 0 ]
  [[ "$output" == *"SAME"* ]] || return 1
}

@test "AC6 NEGATIVE PROOF: an ISO row with no zone is refused and nothing is written" {
  python3 -c 'import sqlite3,sys; d=sqlite3.connect(sys.argv[1]); d.execute("INSERT INTO messages (created_at, content) VALUES (?, ?)", ("2026-11-07T02:00:00", "no zone")); d.commit()' "$MESSAGES_DB"
  run python3 "$SCRIPT" --apply
  [ "$status" -eq 1 ]
  [[ "$output" == *"no zone"* ]] || return 1
  [[ "$(rows)" == *"T00:53:24.406Z"* ]] || return 1
}
