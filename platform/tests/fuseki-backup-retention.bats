#!/usr/bin/env bats
# @test-type: unit
# @domain: knowledge — the product domain this suite guards (#4334)
# #3799/#4171 — backup retention.
#
# WHAT CHANGED AND WHY THIS FILE WAS REWRITTEN (2026-09-16):
#
# 1. The old default assertion was `FUSEKI_BACKUP_KEEP:-2`, justified in its own
#    name as bounding Bedroom fill "until compact right-sizes the store". That
#    condition expired when #4171 landed: compaction succeeded 2026-09-14 16:19
#    for the first time since 08-29, the store went 432 GB -> 15 GB, and a
#    backup is now one ~300 MB dump instead of a 450 GB directory copy. KEEP=2
#    was famine rationing; a week of dumps costs about 2 GB. Keeping the old
#    number would hold a recovery-point floor at two for a cost that no longer
#    exists.
#
# 2. The old negative proof re-implemented "the exact prune from step 5" INSIDE
#    the test and asserted against its own copy. That is a test of the test: it
#    passed whether or not the real prune worked, and it kept passing through a
#    rewrite that changed the prune completely. These now drive the real script's
#    real prune via a stub ssh, so a broken prune fails here.
#
# 3. #4336: the rest of the old file still grepped the script's text (the default
#    keep, `sort -r`, no `ls -1t`) and run_prune still re-typed step 5 inside the
#    test. Every case now runs the REAL fuseki-backup.sh end to end, against a
#    fixture: stub ssh/scp run the "remote" side in a temp dir, stub curl plays a
#    Fuseki that produces one fresh dump, and the prune under test is the script's
#    own. Break the prune in the script and these go red.

SCRIPT="${BATS_TEST_DIRNAME}/../scripts/fuseki-backup.sh"

setup() {
  TMP="$BATS_TEST_TMPDIR"
  REMOTE_BASE="$TMP/remote"; DUMPS="$REMOTE_BASE/dumps"; mkdir -p "$DUMPS"
  LOCAL="$TMP/local"; mkdir -p "$LOCAL" "$TMP/bin" "$TMP/scripts"
  # Five dated dumps, oldest -> newest. Names are ISO-dated, which is the whole
  # point: a lexical sort IS chronological, and mtime is not to be trusted
  # (scp carries source times; on 2026-08-28 an `ls -t` prune deleted five days
  # of backups while logging OK).
  for d in 2026-09-10 2026-09-11 2026-09-12 2026-09-13 2026-09-14; do
    printf 'x' > "$DUMPS/pods_${d}_03-00-00.nq.gz"
  done
  # The oldest is the restore-proven one — the copy a drill has actually opened.
  printf '%s\n' "pods_2026-09-10_03-00-00.nq.gz" > "$REMOTE_BASE/restore-proven.txt"
  # Deliberately invert mtimes so anything ordering by time picks wrong.
  touch -t 202601010000 "$DUMPS/pods_2026-09-14_03-00-00.nq.gz"
  touch -t 202612010000 "$DUMPS/pods_2026-09-10_03-00-00.nq.gz"

  # The script, byte for byte, beside empty env/auth helpers so nothing real is sourced.
  cp "$SCRIPT" "$TMP/scripts/fuseki-backup.sh"
  : > "$TMP/scripts/chorus-env-setup.sh"; : > "$TMP/scripts/fuseki-auth.sh"
  # ssh: drop ssh's own flags and the host, run the remote command here
  printf '%s\n' '#!/bin/bash' \
    'while [ $# -gt 0 ]; do case "$1" in -o) shift 2 ;; -*) shift ;; *) break ;; esac; done' \
    'shift' 'bash -c "$*"' > "$TMP/bin/ssh"
  # scp: copy SRC to HOST:DEST as a local copy
  printf '%s\n' '#!/bin/bash' \
    'args=(); while [ $# -gt 0 ]; do case "$1" in -o) shift 2 ;; -*) shift ;; *) args+=("$1"); shift ;; esac; done' \
    'cp "${args[0]}" "${args[1]#*:}"' > "$TMP/bin/scp"
  # curl: a Fuseki whose backup task finishes at once and leaves tonight's dump,
  # stamped later than the run's start mark (a real dump takes minutes; -nt is per-second)
  printf '%s\n' '#!/bin/bash' \
    'case "$*" in' \
    "  *'/\$/backup/'*) head -c 2048 /dev/zero > \"$LOCAL/pods_2026-09-15_03-00-00.nq.gz\"; touch -t 209901010000 \"$LOCAL/pods_2026-09-15_03-00-00.nq.gz\"; echo '{\"taskId\":\"42\"}' ;;" \
    "  *'/\$/tasks'*) echo '[{\"taskId\":\"42\",\"finished\":\"2026-09-15T03:04:25\",\"success\":true}]' ;;" \
    '  *) exit 7 ;;' 'esac' > "$TMP/bin/curl"
  printf '#!/bin/bash\nexit 0\n' > "$TMP/bin/chorus-log"
  chmod +x "$TMP/bin/ssh" "$TMP/bin/scp" "$TMP/bin/curl" "$TMP/bin/chorus-log"
}

# run_backup [KEEP] — the real script, end to end. No KEEP = the script's own default.
run_backup() {
  local keep=()
  [ -n "${1:-}" ] && keep=(FUSEKI_BACKUP_KEEP="$1")
  run env -i PATH="$TMP/bin:/usr/bin:/bin" HOME="$TMP" \
    FUSEKI_BACKUP_REMOTE=stub FUSEKI_BACKUP_DEST="$REMOTE_BASE" FUSEKI_BACKUP_LOCAL="$LOCAL" \
    FUSEKI_BACKUP_MIN_MB=0 FUSEKI_URL=http://127.0.0.1:9 CHORUS_LOG="$TMP/bin/chorus-log" \
    ${keep[@]+"${keep[@]}"} bash "$TMP/scripts/fuseki-backup.sh"
}

held() { ls -1 "$DUMPS" | wc -l | tr -d ' '; }

@test "the backup ships tonight's dump to the remote and says so" {
  run_backup 9
  [ "$status" -eq 0 ]
  [ -f "$DUMPS/pods_2026-09-15_03-00-00.nq.gz" ]
  echo "$output" | grep -q "OK: pods_2026-09-15_03-00-00.nq.gz"
}

@test "the default keep is a week of dumps, not the 2 that rationed a 450GB store" {
  # the proven copy is out of this picture (a name no dump carries). Not an absent file:
  # the script dies under pipefail when restore-proven.txt is missing (reported on #4336).
  printf '%s\n' "pods_2000-01-01_03-00-00.nq.gz" > "$REMOTE_BASE/restore-proven.txt"
  for d in 2026-09-07 2026-09-08 2026-09-09; do printf 'x' > "$DUMPS/pods_${d}_03-00-00.nq.gz"; done
  # 8 on the remote + tonight's = 9; the script's own default must leave 7
  run_backup
  [ "$status" -eq 0 ]
  [ "$(held)" = "7" ]
  [ ! -f "$DUMPS/pods_2026-09-07_03-00-00.nq.gz" ]
  [ ! -f "$DUMPS/pods_2026-09-08_03-00-00.nq.gz" ]
  [ -f "$DUMPS/pods_2026-09-09_03-00-00.nq.gz" ]
  echo "$output" | grep -q "7 dumps held"
}

@test "the prune is name-ordered, never mtime-ordered" {
  # mtimes say 09-10 is the newest and 09-14 the oldest; names say the reverse
  # the proven copy is out of this picture (a name no dump carries). Not an absent file:
  # the script dies under pipefail when restore-proven.txt is missing (reported on #4336).
  printf '%s\n' "pods_2000-01-01_03-00-00.nq.gz" > "$REMOTE_BASE/restore-proven.txt"
  run_backup 2
  [ "$status" -eq 0 ]
  [ -f "$DUMPS/pods_2026-09-15_03-00-00.nq.gz" ]
  [ -f "$DUMPS/pods_2026-09-14_03-00-00.nq.gz" ]
  [ ! -f "$DUMPS/pods_2026-09-10_03-00-00.nq.gz" ]
  [ "$(held)" = "2" ]
}

@test "NEGATIVE PROOF — over the limit, the oldest go and the newest stay" {
  run_backup 3
  [ "$status" -eq 0 ]
  [ -f "$DUMPS/pods_2026-09-15_03-00-00.nq.gz" ]
  [ -f "$DUMPS/pods_2026-09-14_03-00-00.nq.gz" ]
  [ -f "$DUMPS/pods_2026-09-13_03-00-00.nq.gz" ]
  [ ! -f "$DUMPS/pods_2026-09-12_03-00-00.nq.gz" ]
  [ ! -f "$DUMPS/pods_2026-09-11_03-00-00.nq.gz" ]
  # the prune speaks: every deletion is named in the log
  echo "$output" | grep -q "pruned pods_2026-09-12_03-00-00.nq.gz"
  echo "$output" | grep -q "pruned pods_2026-09-11_03-00-00.nq.gz"
}

@test "NEGATIVE PROOF — the restore-proven copy survives even when it is the oldest" {
  # Retention by count alone deletes the oldest first, which is exactly the one
  # copy anyone has verified. Proven live 2026-09-15 05:31:22: KEEP=2 against
  # 4 dumps pruned the two NEWER and kept 2026-09-14_19-32-12 because a drill
  # had opened it.
  run_backup 2
  [ "$status" -eq 0 ]
  [ -f "$DUMPS/pods_2026-09-10_03-00-00.nq.gz" ]
  echo "$output" | grep -q "kept pods_2026-09-10_03-00-00.nq.gz (restore-proven)"
}

@test "NEGATIVE PROOF — under the limit, nothing is deleted" {
  # A prune that always deletes something is as broken as one that never does.
  run_backup 9
  [ "$status" -eq 0 ]
  [ "$(held)" = "6" ]
  echo "$output" | grep -q "nothing over the keep limit of 9"
}
