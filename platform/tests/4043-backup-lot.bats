#!/usr/bin/env bats
# @test-type: unit
# #4043 — the drill must grade the lot the backup agent actually fills.
#
# The 2026-08-31 incident: fuseki-backup's plist shipped nightly snapshots to
# /Volumes/VideosNew/... while restore-drill.sh fell back to the abandoned
# /Users/jeffbridwell/Backups/... lot — 24 minutes restoring a 16-day-old
# leftover, then a red that read as "backups dead". Two fixes under test here:
#   1. chorus-env-setup.sh is the ONE home for FUSEKI_BACKUP_DEST/REMOTE.
#   2. restore-drill.sh's drill_lot_check refuses when the newest
#      ops.backup.fuseki.completed dest= is outside the drill's DEST_BASE.
#
# #3734: the guard ships with its negative proof — the violation fixture FAILS.

SCRIPTS="$BATS_TEST_DIRNAME/../scripts"

setup() {
  # source the drill for its functions only (source-guard returns before work)
  source "$SCRIPTS/restore-drill.sh"
}

@test "env-setup exports FUSEKI_BACKUP_DEST (single home)" {
  run bash -c "unset FUSEKI_BACKUP_DEST; source '$SCRIPTS/chorus-env-setup.sh' >/dev/null 2>&1; printf %s \"\$FUSEKI_BACKUP_DEST\""
  [ "$status" -eq 0 ]
  [ -n "$output" ]
  [[ "$output" != "/Users/jeffbridwell/Backups/library/fuseki" ]]
}

@test "NEGATIVE PROOF: agent dest outside the drill's base → check FAILS" {
  run drill_lot_check "/Users/jeffbridwell/Backups/library/fuseki" \
    "/Volumes/VideosNew/backups/library/fuseki/fuseki-pods-2026-08-31-030001"
  [ "$status" -eq 1 ]
}

@test "control: agent dest inside the drill's base → check passes" {
  run drill_lot_check "/Volumes/VideosNew/backups/library/fuseki" \
    "/Volumes/VideosNew/backups/library/fuseki/fuseki-pods-2026-08-31-030001"
  [ "$status" -eq 0 ]
}

@test "control: no completed event ever (empty dest) → check passes" {
  run drill_lot_check "/Volumes/VideosNew/backups/library/fuseki" ""
  [ "$status" -eq 0 ]
}

@test "NEGATIVE PROOF: prefix trick (base as string prefix, not a dir) → FAILS" {
  # /Volumes/VideosNew/backups/library/fuseki-old is NOT inside .../fuseki
  run drill_lot_check "/Volumes/VideosNew/backups/library/fuseki" \
    "/Volumes/VideosNew/backups/library/fuseki-old/fuseki-pods-2026-08-31-030001"
  [ "$status" -eq 1 ]
}

@test "NEGATIVE PROOF: spine's newest backup completed 3 days ago → freshness FAILS" {
  source "$SCRIPTS/test-restore-drill.sh"
  f="$BATS_TEST_TMPDIR/spine.log"
  old="$(date -j -v-3d '+%Y-%m-%dT%H:%M:%S')"
  printf '{"timestamp":"%s","event":"ops.backup.fuseki.completed","payload":"dest=/x/y"}\n' "$old" > "$f"
  run backup_fresh_check "$f" "$(date +%s)" 48
  [ "$status" -eq 1 ]
}

@test "control: backup completed 1h ago → freshness passes" {
  source "$SCRIPTS/test-restore-drill.sh"
  f="$BATS_TEST_TMPDIR/spine.log"
  recent="$(date -j -v-1H '+%Y-%m-%dT%H:%M:%S')"
  printf '{"timestamp":"%s","event":"ops.backup.fuseki.completed","payload":"dest=/x/y"}\n' "$recent" > "$f"
  run backup_fresh_check "$f" "$(date +%s)" 48
  [ "$status" -eq 0 ]
}

@test "control: no completed event ever → freshness passes (no-backup path owns it)" {
  source "$SCRIPTS/test-restore-drill.sh"
  f="$BATS_TEST_TMPDIR/empty.log"; : > "$f"
  run backup_fresh_check "$f" "$(date +%s)" 48
  [ "$status" -eq 0 ]
}
