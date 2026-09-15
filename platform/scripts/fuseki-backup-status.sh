#!/usr/bin/env bash
# fuseki-backup-status.sh — "is the backup healthy?" in one command.
#
# #4171 AC7. Jeff's four questions, in his order: when did the last run finish,
# how much did it cost, how many recovery points are held, and when did one of
# them last PROVE it restores. Anything else is decoration.
#
# Every line is measured from the artifact or the spine, never from a config
# file describing what should happen. The backup logged "OK: N files" for weeks
# over a destination whose newest real recovery point was 30 days old; a status
# that reads intent instead of evidence would have said healthy the whole time.
set -uo pipefail
. "$(dirname "${BASH_SOURCE[0]}")/chorus-env-setup.sh" >/dev/null 2>&1 || true

REMOTE="${FUSEKI_BACKUP_REMOTE:-Jeffs-Mac-mini.local}"
DEST_BASE="${FUSEKI_BACKUP_DEST:?FUSEKI_BACKUP_DEST unset}"
DATASET="${FUSEKI_DATASET:-pods}"
STALE_H="${FUSEKI_BACKUP_STALE_HOURS:-36}"
PROVE_D="${FUSEKI_RESTORE_STALE_DAYS:-14}"
verdict=0
say(){ printf '  %-22s %s\n' "$1" "$2"; }

echo "fuseki backup — $(date '+%F %T')"

if ! ssh -o ConnectTimeout=10 "$REMOTE" true 2>/dev/null; then
  say "destination" "UNREACHABLE — $REMOTE"
  echo "VERDICT: UNMEASURED"; exit 2      # not the same as unhealthy, and never reported as healthy
fi

listing="$(ssh -o ConnectTimeout=10 "$REMOTE" "ls -1 '$DEST_BASE/dumps'/${DATASET}_*.nq.gz 2>/dev/null | sort" | tr -d '\r')"
held=$(printf '%s' "$listing" | grep -c . || true)
newest="$(printf '%s' "$listing" | tail -1)"
oldest="$(printf '%s' "$listing" | head -1)"

if [ "$held" -eq 0 ]; then
  say "recovery points" "NONE"
  echo "VERDICT: RED — nothing to restore from"; exit 1
fi

# The timestamp comes out of the FILENAME, not mtime. scp carries source times,
# and on 2026-08-28 an mtime-ordered prune deleted five days of backups while
# logging OK. The name is ISO-dated; it cannot drift.
stamp="$(basename "$newest" | sed -E "s/^${DATASET}_([0-9-]+)_([0-9]{2})-([0-9]{2})-([0-9]{2}).*/\1 \2:\3:\4/")"
age_h=$(( ( $(date +%s) - $(date -j -f '%Y-%m-%d %H:%M:%S' "$stamp" +%s 2>/dev/null || echo 0) ) / 3600 ))
size="$(ssh -o ConnectTimeout=10 "$REMOTE" "stat -f %z '$newest'" | tr -d '\r')"

say "last backup" "$stamp  (${age_h}h ago)"
say "size" "$(( size / 1024 / 1024 )) MB"
say "recovery points" "$held held  (oldest $(basename "$oldest"))"

proven="$(ssh -o ConnectTimeout=10 "$REMOTE" "cat '$DEST_BASE/restore-proven.txt' 2>/dev/null" | tr -d '\r')"
if [ -n "$proven" ]; then
  pstamp="$(basename "$proven" | sed -E "s/^${DATASET}_([0-9-]+)_.*/\1/")"
  pdays=$(( ( $(date +%s) - $(date -j -f '%Y-%m-%d' "$pstamp" +%s 2>/dev/null || echo 0) ) / 86400 ))
  say "last proven restore" "$(basename "$proven")  (${pdays}d ago)"
  [ "$pdays" -le "$PROVE_D" ] || { say "" "STALE — no restore proven in ${pdays} days"; verdict=1; }
else
  say "last proven restore" "NEVER — no drill has passed"
  verdict=1
fi

[ "$age_h" -le "$STALE_H" ] || { say "" "STALE — newest backup is ${age_h}h old"; verdict=1; }

if [ "$verdict" -eq 0 ]; then echo "VERDICT: GREEN"; else echo "VERDICT: RED"; fi
exit "$verdict"
