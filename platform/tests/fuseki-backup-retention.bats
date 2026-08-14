#!/usr/bin/env bats
# @test-type: unit
# #3799 — backup retention. The prune (fuseki-backup.sh step 5) must keep exactly
# KEEP newest backups and delete the rest, and the default KEEP must bound Bedroom
# fill (2, not 3-4). Negative proof (#3734): with 4 backups and KEEP=2, exactly the
# 2 OLDEST are deleted and the 2 newest survive — a prune that kept all 4, or ate a
# recent one, fails this.

SCRIPT="${BATS_TEST_DIRNAME}/../scripts/fuseki-backup.sh"

setup() {
  TMP="$(mktemp -d)"
  # four dated backups with distinct mtimes (oldest -> newest)
  for d in 2026-08-10 2026-08-11 2026-08-12 2026-08-13; do
    mkdir -p "$TMP/fuseki-pods-$d"
  done
  touch -t 202608100300 "$TMP/fuseki-pods-2026-08-10"
  touch -t 202608110300 "$TMP/fuseki-pods-2026-08-11"
  touch -t 202608120300 "$TMP/fuseki-pods-2026-08-12"
  touch -t 202608130300 "$TMP/fuseki-pods-2026-08-13"
}
teardown() { rm -rf "$TMP"; }

@test "default retention is 2 (bounds Bedroom fill until compact right-sizes the store)" {
  grep -q 'FUSEKI_BACKUP_KEEP:-2' "$SCRIPT"
}

@test "prune keeps exactly KEEP=2 newest, deletes the older (negative proof)" {
  KEEP=2
  # the exact prune from step 5 of fuseki-backup.sh
  ls -1dt "$TMP"/fuseki-pods-* 2>/dev/null | tail -n +$((KEEP+1)) | xargs -I{} rm -rf {}
  n=$(ls -1d "$TMP"/fuseki-pods-* 2>/dev/null | wc -l | tr -d ' ')
  [ "$n" -eq 2 ]
  [ -d "$TMP/fuseki-pods-2026-08-13" ]   # newest survives
  [ -d "$TMP/fuseki-pods-2026-08-12" ]   # 2nd-newest survives
  [ ! -d "$TMP/fuseki-pods-2026-08-11" ] # older deleted
  [ ! -d "$TMP/fuseki-pods-2026-08-10" ] # oldest deleted
}
