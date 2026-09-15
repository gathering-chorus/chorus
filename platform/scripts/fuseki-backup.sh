#!/usr/bin/env bash
# fuseki-backup.sh — nightly off-machine backup of the Fuseki TDB2 store.
#
# #4171. Jeff, three times since 2026-08-09: "they start at midnight and are
# still running", "the size increases", "fix the backups". Measured 09-14 08:12:
# 377 GB copied per night, finishing 11:53 and 13:19 — into his working day.
#
# WHAT THIS IS NOW: ask Fuseki for a dump, copy one file. That is the whole job.
#
#   2026-09-14 19:32:12 -> 19:36:37   4m25s   299 MB gzipped N-Quads
#
# WHAT IT REPLACED, and why the old shape existed. Until today the native dump
# could not run: the node table was corrupt, so `$/backup` died on its first
# read and left 20-byte .tmp stubs (Jun 25 x2, Jul 1 x2 — still on disk). The
# same corruption killed compaction and tdb2.tdbdump. With the dump door shut,
# the only way out was to copy the STORE, and copying a live TDB2 safely needs
# an APFS snapshot, a mount, and an rsync — ~450 lines of machinery whose real
# purpose was working around a broken database.
#
# The store was rebuilt 2026-09-14 (432 GB -> 15 GB, 39,687,538 triples, exact)
# and compaction succeeded at 16:19 for the first time since 08-29. The dump
# works again, so the machinery goes.
#
# MEASURED ALTERNATIVES, so nobody re-derives them:
#   rsync --link-dest      run added 16.3 GB. --link-dest links only when size
#                          AND mtime match; Fuseki rewrites every index mtime
#                          nightly (GOSP.dat, same 70,908,903,424 bytes, mtime
#                          09-12 23:59:51 -> 09-14 09:36:53).
#   hard-linked snapshots  run added 14.8 GB. rsync's delta saves the WIRE; on
#                          the destination it writes a whole new file and
#                          renames, so any changed file costs its full size.
#   APFS clones+--inplace  run consumed 240 MB, 8m30s. Works, and still copies
#                          index files that no restore needs.
#   native dump            299 MB, 4m25s, and restores into any generation.
#
# A dump is also the only form that does not care about generations, index
# layout, or mtimes — the three things that broke every previous approach.
set -euo pipefail
. "$(dirname "${BASH_SOURCE[0]}")/chorus-env-setup.sh" >/dev/null 2>&1 || true
. "$(dirname "${BASH_SOURCE[0]}")/fuseki-auth.sh" >/dev/null 2>&1 || true

REMOTE="${FUSEKI_BACKUP_REMOTE:-Jeffs-Mac-mini.local}"
DEST_BASE="${FUSEKI_BACKUP_DEST:?FUSEKI_BACKUP_DEST unset — chorus-env-setup.sh missing?}"
DATASET="${FUSEKI_DATASET:-pods}"
FUSEKI="${FUSEKI_URL:-http://localhost:3030}"
BACKUP_DIR="${FUSEKI_BACKUP_LOCAL:-$HOME/.gathering/data/backups}"
KEEP="${FUSEKI_BACKUP_KEEP:-7}"     # a dump is ~300 MB, so a week costs ~2 GB
TIMEOUT="${FUSEKI_BACKUP_TIMEOUT:-3600}"
LOG_TAG="fuseki-backup"
CHORUS_LOG="${CHORUS_LOG:-$HOME/CascadeProjects/chorus/platform/scripts/chorus-log}"

log(){ echo "$(date '+%F %T') [$LOG_TAG] $*"; }
spine(){ "$CHORUS_LOG" "$1" silas "${@:2}" 2>/dev/null || true; }
die(){ log "ERROR: $*"; spine ops.backup.fuseki.failed reason="$2" detail="$1"; exit 1; }

# 0. Bedroom reachable. Read is free; this is the precondition.
ssh -o ConnectTimeout=10 "$REMOTE" true 2>/dev/null \
  || die "$REMOTE unreachable" unreachable

# 1. Ask Fuseki for the dump.
#
# A dump takes a READ lock, so the roles keep writing while it runs — unlike
# compaction, which held every writer for 78 minutes on 09-14 and blocked two
# roles who had no way to know why.
# The POST returns this run's taskId. Keep it: the wait below must follow THIS
# dump, not "the newest Backup task". Measured 2026-09-14 20:37:56 — without the
# id, the loop found the previous run's FINISHED task, skipped waiting entirely,
# and shipped an hour-old dump as tonight's backup, in 28 seconds, reporting OK.
# A backup that can ship yesterday's file is not a backup.
STARTED_MARK="$(mktemp)"; trap 'rm -f "$STARTED_MARK"' EXIT
log "requesting a dump of '$DATASET'"
TASK_ID="$(curl -fsS -X POST --max-time 60 ${FUSEKI_AUTH[@]+"${FUSEKI_AUTH[@]}"} \
  "$FUSEKI/\$/backup/$DATASET" \
  | python3 -c 'import sys,json;print(json.load(sys.stdin).get("taskId",""))')" \
  || die "Fuseki refused the backup request" request-refused
[ -n "$TASK_ID" ] || die "Fuseki accepted the request but returned no taskId" no-task-id
log "dump task $TASK_ID"

# 2. Wait for the task, and READ ITS SUCCESS FLAG.
#
# The compaction bug this repeats: our old weekly script POSTed to $/compact and
# never read `success`, so tasks that failed in 90 seconds with a node-table
# exception were logged as done for eleven weeks. A task that is finished is not
# a task that worked.
log "waiting for the dump task"
deadline=$(( $(date +%s) + TIMEOUT ))
while :; do
  read -r fin ok < <(curl -fsS --max-time 30 ${FUSEKI_AUTH[@]+"${FUSEKI_AUTH[@]}"} "$FUSEKI/\$/tasks" \
    | TASK_ID="$TASK_ID" python3 -c '
import sys, json, os
want = os.environ["TASK_ID"]
t = [x for x in json.load(sys.stdin) if str(x.get("taskId")) == want]
t = t[0] if t else {}
print(t.get("finished") or "-", t.get("success"))') || die "could not read the task list" task-unreadable
  [ "$fin" != "-" ] && break
  [ "$(date +%s)" -lt "$deadline" ] || die "dump did not finish within ${TIMEOUT}s" timeout
  sleep 10
done
[ "$ok" = "True" ] || die "Fuseki reported the dump FAILED (success=$ok)" dump-failed

# 3. Find it, and require it to be a plausible size.
#
# The four backups before today's were 20 BYTES each — a gzip header and
# nothing else, because the read died immediately. Every one of them sat in the
# backup directory looking like a backup. A floor is what separates "a file
# exists" from "the data is in it".
# Newest AND newer than this run started. "Newest" alone is how the 20:37 run
# shipped an hour-old file: the newest dump is not necessarily OUR dump.
DUMP="$(ls -1t "$BACKUP_DIR"/${DATASET}_*.nq.gz 2>/dev/null | head -1)"
[ -n "$DUMP" ] && [ -f "$DUMP" ] || die "no dump file appeared in $BACKUP_DIR" no-dump
[ "$DUMP" -nt "$STARTED_MARK" ] || die "newest dump $(basename "$DUMP") predates this run — the dump we asked for never landed" stale-dump
SIZE=$(stat -f %z "$DUMP")
MIN=$(( ${FUSEKI_BACKUP_MIN_MB:-50} * 1024 * 1024 ))
[ "$SIZE" -ge "$MIN" ] || die "dump is only $SIZE bytes (floor $MIN) — refusing to ship a stub" dump-too-small

# 4. Ship it off the machine.
log "copying $(basename "$DUMP") ($(( SIZE / 1024 / 1024 )) MB) to $REMOTE"
ssh -o ConnectTimeout=10 "$REMOTE" "mkdir -p '$DEST_BASE/dumps'" || die "could not create the remote directory" remote-mkdir
scp -q -o ConnectTimeout=10 "$DUMP" "${REMOTE}:${DEST_BASE}/dumps/" || die "scp failed" copy-failed
RSIZE="$(ssh -o ConnectTimeout=10 "$REMOTE" "stat -f %z '$DEST_BASE/dumps/$(basename "$DUMP")'" | tr -d '\r')"
[ "$RSIZE" = "$SIZE" ] || die "copy is $RSIZE bytes, source is $SIZE — incomplete" size-mismatch

# 5. Retention, by NAME. Never by mtime: scp and rsync carry source times, and
# on 2026-08-28 an `ls -t` prune ranked five fresh snapshots as older than
# 08-22 and deleted every backup from 08-24 through 08-28 while logging "OK".
# Names are ISO-dated, so a lexical sort IS chronological.
ssh -o ConnectTimeout=10 "$REMOTE" \
  "ls -1 '$DEST_BASE/dumps'/${DATASET}_*.nq.gz 2>/dev/null | sort -r | tail -n +$((KEEP+1)) | xargs -I{} rm -f '$DEST_BASE/dumps/'{}" 2>/dev/null || true
ls -1t "$BACKUP_DIR"/${DATASET}_*.nq.gz 2>/dev/null | tail -n +3 | xargs -I{} rm -f {} 2>/dev/null || true
# The stubs from the corrupt era are not backups; clear them so the directory
# stops advertising four recovery points that never held a triple.
find "$BACKUP_DIR" -name "${DATASET}_*.nq.gz*.tmp" -size -1k -delete 2>/dev/null || true

HELD="$(ssh -o ConnectTimeout=10 "$REMOTE" "ls -1 '$DEST_BASE/dumps'/${DATASET}_*.nq.gz 2>/dev/null | wc -l" | tr -d ' \r')"
log "OK: $(basename "$DUMP") — $(( SIZE / 1024 / 1024 )) MB on $REMOTE, $HELD dumps held"
spine ops.backup.fuseki.completed bytes="$SIZE" dest="$DEST_BASE/dumps/$(basename "$DUMP")" held="$HELD"
