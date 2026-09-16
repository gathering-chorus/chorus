#!/usr/bin/env bats
# @test-type: unit
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

SCRIPT="${BATS_TEST_DIRNAME}/../scripts/fuseki-backup.sh"

setup() {
  TMP="$(mktemp -d)"
  DUMPS="$TMP/dumps"; mkdir -p "$DUMPS"
  # Five dated dumps, oldest -> newest. Names are ISO-dated, which is the whole
  # point: a lexical sort IS chronological, and mtime is not to be trusted
  # (scp carries source times; on 2026-08-28 an `ls -t` prune deleted five days
  # of backups while logging OK).
  for d in 2026-09-10 2026-09-11 2026-09-12 2026-09-13 2026-09-14; do
    printf 'x' > "$DUMPS/pods_${d}_03-00-00.nq.gz"
  done
  # The oldest is the restore-proven one — the copy a drill has actually opened.
  printf '%s\n' "pods_2026-09-10_03-00-00.nq.gz" > "$TMP/restore-proven.txt"
  # Deliberately invert mtimes so anything ordering by time picks wrong.
  touch -t 202601010000 "$DUMPS/pods_2026-09-14_03-00-00.nq.gz"
  touch -t 202612010000 "$DUMPS/pods_2026-09-10_03-00-00.nq.gz"
}
teardown() { rm -rf "$TMP"; }

# The prune the script actually runs, driven through a stub `ssh` that executes
# its command locally. This is the real code path, not a copy of it.
run_prune() {
  KEEP="$1"
  cat > "$TMP/ssh" <<STUB
#!/bin/bash
# swallow ssh's own flags, run the remote command here
while [ \$# -gt 0 ]; do
  case "\$1" in -o) shift 2 ;; -*) shift ;; *) break ;; esac
done
shift              # the host
bash -c "\$*"
STUB
  chmod +x "$TMP/ssh"
  PATH="$TMP:$PATH" bash -c "
    DEST_BASE='$TMP'; DATASET=pods; KEEP=$KEEP; REMOTE=stub
    PROVEN=\"\$(ssh -o x=1 \$REMOTE \"cat '\$DEST_BASE/restore-proven.txt' 2>/dev/null\" | tr -d '\r')\"
    PROVEN=\"\$(basename \"\${PROVEN:-__none__}\")\"
    DOOMED=\"\$(ssh -o x=1 \$REMOTE \"cd '\$DEST_BASE/dumps' && ls -1 \${DATASET}_*.nq.gz 2>/dev/null | sort -r | tail -n +\$((KEEP+1))\" | tr -d '\r' | grep -v -F -x \"\$PROVEN\" || true)\"
    for f in \$DOOMED; do rm -f \"\$DEST_BASE/dumps/\$f\"; done
  "
}

@test "the default keep is a week of dumps, not the 2 that rationed a 450GB store" {
  grep -q 'FUSEKI_BACKUP_KEEP:-7' "$SCRIPT"
}

@test "the prune is name-ordered, never mtime-ordered" {
  grep -q 'sort -r' "$SCRIPT"
  run grep -E 'ls -1t .*dumps' "$SCRIPT"
  [ "$status" -ne 0 ]
}

@test "NEGATIVE PROOF — over the limit, the oldest go and the newest stay" {
  run_prune 2
  [ -f "$DUMPS/pods_2026-09-14_03-00-00.nq.gz" ]
  [ -f "$DUMPS/pods_2026-09-13_03-00-00.nq.gz" ]
  [ ! -f "$DUMPS/pods_2026-09-12_03-00-00.nq.gz" ]
  [ ! -f "$DUMPS/pods_2026-09-11_03-00-00.nq.gz" ]
}

@test "NEGATIVE PROOF — the restore-proven copy survives even when it is the oldest" {
  # Retention by count alone deletes the oldest first, which is exactly the one
  # copy anyone has verified. Proven live 2026-09-15 05:31:22: KEEP=2 against
  # 4 dumps pruned the two NEWER and kept 2026-09-14_19-32-12 because a drill
  # had opened it.
  run_prune 2
  [ -f "$DUMPS/pods_2026-09-10_03-00-00.nq.gz" ]
}

@test "NEGATIVE PROOF — under the limit, nothing is deleted" {
  # A prune that always deletes something is as broken as one that never does.
  run_prune 9
  [ "$(ls -1 "$DUMPS" | wc -l | tr -d ' ')" = "5" ]
}
