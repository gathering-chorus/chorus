#!/usr/bin/env bash
# files-backup.sh — nightly off-machine copy of the file-tree systems of record
# that the store map marks `"backup": "tar"` (#4518).
#
# Found 2026-10-13: the store map named Community Solid Server's files (every
# account and signing key behind id.lightlifeurbangardens.com) as a system of
# record, and nothing backed them up. Adding wine-diary's own sign-in server
# would have doubled that. Which trees are backed up is a store-map fact, the
# same way fuseki-backup.sh picks its datasets, so a new app's store is one
# entry, not a new script.
#
# Shape follows fuseki-backup.sh, whose comments record why each step exists:
# a floor (a 20-byte archive is not a backup), copy then compare sizes, and
# retention by NAME (never mtime; 2026-08-28 lost five days to an `ls -t`).
#
# A tar of a live CSS tree can catch a file mid-write. CSS writes each account
# and key as its own small file, so a torn file costs that one record, and the
# night before is kept. Measured 2026-10-13: 16 MB and 20 KB.
set -euo pipefail
. "$(dirname "${BASH_SOURCE[0]}")/chorus-env-setup.sh" >/dev/null 2>&1 || true

REMOTE="${FILES_BACKUP_REMOTE:-Jeffs-Mac-mini.local}"
DEST_BASE="${FILES_BACKUP_DEST:-/Volumes/VideosNew/backups/library/files}"
STORE_MAP="${STORE_MAP:-$(dirname "${BASH_SOURCE[0]}")/../config/store-map.json}"
LOCAL="${FILES_BACKUP_LOCAL:-$HOME/.gathering/data/backups/files}"
KEEP="${FILES_BACKUP_KEEP:-7}"
CHORUS_LOG="${CHORUS_LOG:-$HOME/CascadeProjects/chorus/platform/scripts/chorus-log}"
STAMP="$(date '+%Y-%m-%d_%H-%M-%S')"

log(){ echo "$(date '+%F %T') [files-backup] $*"; }
spine(){ "$CHORUS_LOG" "$1" silas "${@:2}" 2>/dev/null || true; }
fail(){ log "ERROR $1: $2"; spine ops.backup.files.failed store="$1" reason="$3"; rc=1; }

ENTRIES="$(python3 -c '
import json,sys
for s in json.load(open(sys.argv[1]))["stores"]:
    if s.get("backup")=="tar" and s.get("role")=="systemOfRecord" and s["match"].get("kind")=="path":
        print(s["name"], s["match"]["value"], s.get("backupMinBytes",1000))' "$STORE_MAP")" \
  || { log "store map unreadable: $STORE_MAP"; exit 1; }
[ -n "$ENTRIES" ] || { log "store map marks no file tree for backup — refusing to back up nothing"; exit 1; }

ssh -n -o ConnectTimeout=10 "$REMOTE" true 2>/dev/null || { log "$REMOTE unreachable"; spine ops.backup.files.failed reason=unreachable; exit 1; }
mkdir -p "$LOCAL"
rc=0
while read -r name path floor; do
  [ -d "$path" ] || { fail "$name" "$path is not a directory" missing; continue; }
  out="$LOCAL/${name}_${STAMP}.tar.gz"
  tar -czf "$out" -C "$(dirname "$path")" "$(basename "$path")" || { fail "$name" "tar failed" tar-failed; continue; }
  size=$(stat -f %z "$out")
  [ "$size" -ge "$floor" ] || { fail "$name" "archive is $size bytes (floor $floor) — refusing to ship a stub" too-small; rm -f "$out"; continue; }
  dest="$DEST_BASE/$name"
  ssh -n -o ConnectTimeout=10 "$REMOTE" "mkdir -p '$dest'" || { fail "$name" "remote mkdir" remote-mkdir; continue; }
  scp -q -o ConnectTimeout=10 "$out" "${REMOTE}:${dest}/" || { fail "$name" "scp failed" copy-failed; continue; }
  rsize="$(ssh -n -o ConnectTimeout=10 "$REMOTE" "stat -f %z '$dest/$(basename "$out")'" | tr -d '\r')"
  [ "$rsize" = "$size" ] || { fail "$name" "copy is $rsize bytes, source $size" size-mismatch; continue; }
  doomed="$(ssh -n -o ConnectTimeout=10 "$REMOTE" "cd '$dest' && ls -1 ${name}_*.tar.gz 2>/dev/null | sort -r | tail -n +$((KEEP+1))" | tr -d '\r' || true)"
  for f in $doomed; do ssh -n -o ConnectTimeout=10 "$REMOTE" "rm -f '$dest/$f'" && log "pruned $name/$f"; done
  ls -1 "$LOCAL"/${name}_*.tar.gz 2>/dev/null | sort -r | tail -n +3 | xargs -I{} rm -f {} 2>/dev/null || true
  held="$(ssh -n -o ConnectTimeout=10 "$REMOTE" "ls -1 '$dest'/${name}_*.tar.gz 2>/dev/null | wc -l" | tr -d ' \r')"
  log "OK: $name — $size bytes on $REMOTE, $held held"
  spine ops.backup.files.completed store="$name" bytes="$size" held="$held"
done <<< "$ENTRIES"
exit $rc
