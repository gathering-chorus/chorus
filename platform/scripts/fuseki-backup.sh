#!/usr/bin/env bash
# fuseki-backup.sh — consistent, recurring off-machine backup of the shared Fuseki TDB2 store (#3560 AC#1).
#
# Proven path (2026-06-22): APFS local snapshot (atomic point-in-time) → rsync FROM the frozen
# snapshot → bedroom. This is the repeatable form of the by-hand backup that was verified restorable
# (62 GB copied, opened with tdb2.tdbquery, counted 31,032,717 triples, recovery-on-open clean).
#
# Why this and not the alternatives:
#   - Fuseki `$/backup` (Thrift dump): FAILS on this store ("Unrecognized type 0", NodeTableTRDF) — broken path.
#   - tdb2.tdbbackup on the live dir: a 2nd process opening the live TDB2 fights Fuseki's lock — corruption risk.
#   - raw rsync of the LIVE dir: multi-minute copy of a store being written = a smeared/torn copy.
#   - APFS snapshot: atomic point-in-time; rsync just copies bytes from a frozen, read-only view — no lock,
#     no torn copy, no contact with the running store. TDB2 recovers-on-open from a point-in-time snapshot.
set -euo pipefail

# OFF-MACHINE target = BEDROOM. `Jeffs-Mac-mini.local` resolves to 192.168.86.242 (bedroom, the M2 Pro) —
# NOT this box (Library = `Jeffs-Mac-Mini-M1-3` / 192.168.86.36, where Fuseki + this script run). The two
# hostnames are near-identical; verify with `ping Jeffs-Mac-mini.local` (→ .242) before assuming it's local.
REMOTE="${FUSEKI_BACKUP_REMOTE:-Jeffs-Mac-mini.local}"
STORE_VOL="/System/Volumes/Data"
SNAP_REL="Users/jeffbridwell/.gathering/data/fuseki-pods"   # path to the TDB2 dir within the snapshot
DEST_BASE="${FUSEKI_BACKUP_DEST:-/Users/jeffbridwell/Backups/library/fuseki}"
KEEP="${FUSEKI_BACKUP_KEEP:-3}"
MNT="$(mktemp -d /tmp/fuseki-backup-snap.XXXXXX)"
LOG_TAG="fuseki-backup"
CHORUS_LOG="${CHORUS_LOG:-/Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/chorus-log}"

log(){ echo "$(date '+%F %T') [$LOG_TAG] $*"; }
spine(){ "$CHORUS_LOG" "$1" silas "${@:2}" 2>/dev/null || true; }
cleanup(){ umount "$MNT" 2>/dev/null || true; rmdir "$MNT" 2>/dev/null || true; }
trap cleanup EXIT

# 0. bedroom reachable (read is free; this is the precondition)
if ! ssh -o ConnectTimeout=10 "$REMOTE" true 2>/dev/null; then
  log "ERROR: bedroom ($REMOTE) unreachable — backup skipped"; spine ops.backup.fuseki.failed reason=bedroom-unreachable; exit 1
fi

# 1. atomic point-in-time snapshot of the volume
SNAP_DATE="$(tmutil localsnapshot / 2>/dev/null | sed -n 's/.*date: *//p' | tr -d ' ')"
[ -n "$SNAP_DATE" ] || { log "ERROR: tmutil localsnapshot produced no date"; spine ops.backup.fuseki.failed reason=snapshot-failed; exit 1; }
SNAP="com.apple.TimeMachine.${SNAP_DATE}.local"
log "snapshot: $SNAP"

# 2. mount it read-only and locate the TDB2 dir
mount_apfs -o ro -s "$SNAP" "$STORE_VOL" "$MNT" 2>/dev/null || { log "ERROR: mount_apfs failed for $SNAP"; spine ops.backup.fuseki.failed reason=mount-failed; exit 1; }
SRC="$MNT/$SNAP_REL"
[ -d "$SRC/Data-0003" ] || { log "ERROR: snapshot missing TDB2 dir at $SRC"; spine ops.backup.fuseki.failed reason=no-tdb2-in-snapshot; exit 1; }

# 3. rsync the frozen store to bedroom (dated dir)
DEST="$DEST_BASE/fuseki-pods-${SNAP_DATE}"
ssh -o ConnectTimeout=10 "$REMOTE" "mkdir -p '$DEST'"
log "rsync → ${REMOTE}:${DEST}"
rsync -a --partial -e "ssh -o ConnectTimeout=10" "$SRC/" "${REMOTE}:${DEST}/"

# 4. completeness check — every file landed (not just rsync exit 0; #3560 lesson: copied != restorable)
SRC_N="$(find "$SRC" -type f | wc -l | tr -d ' ')"
DST_N="$(ssh -o ConnectTimeout=10 "$REMOTE" "find '$DEST' -type f | wc -l" | tr -d ' ')"
if [ "$SRC_N" != "$DST_N" ]; then
  log "ERROR: incomplete copy — src=$SRC_N dst=$DST_N"; spine ops.backup.fuseki.failed reason=incomplete src="$SRC_N" dst="$DST_N"; exit 1
fi

# 5. release the snapshot + prune old remote backups (keep last $KEEP)
umount "$MNT" 2>/dev/null || true
tmutil deletelocalsnapshots "$SNAP_DATE" 2>/dev/null || true
ssh -o ConnectTimeout=10 "$REMOTE" "ls -1dt '$DEST_BASE'/fuseki-pods-* 2>/dev/null | tail -n +$((KEEP+1)) | xargs -I{} rm -rf {}" 2>/dev/null || true

log "OK: $SRC_N files → ${REMOTE}:${DEST}"
spine ops.backup.fuseki.completed files="$SRC_N" dest="$DEST"

# --- RESTORE (proven 2026-06-22) ---
# 1. Pull the backup back to a scratch (or the live path while Fuseki is stopped):
#      rsync -a Jeffs-Mac-mini.local:/Users/jeffbridwell/Backups/library/fuseki/fuseki-pods-<date>/ <target>/
# 2. Open-and-count to prove restorable (TDB2 recovers-on-open; a clean count = restorable):
#      tdb2.tdbquery --loc=<target> 'SELECT (COUNT(*) AS ?n) WHERE { GRAPH ?g { ?s ?p ?o } }'   # expect ~31M
# 3. To go live: stop com.gathering.fuseki, replace ~/.gathering/data/fuseki-pods with <target>, restart.
