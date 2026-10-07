#!/usr/bin/env bash
# fuseki-restore-dump.sh — prove a backup DUMP restores into a working store.
#
# #4171 AC5/AC6. Jeff's bar is "a store that answers a query", not "files
# copied". The distance between those two is the whole reason this card exists:
# the nightly logged "OK: N files" for weeks over a destination whose newest
# real recovery point was 30 days old, and four native dumps in June and July
# were 20-byte stubs sitting in the backup directory looking like backups.
#
# This drill pulls the dump BACK from Bedroom — not a convenient local copy.
# Restoring the artifact you would never reach for in an incident proves
# nothing about the one you would.
#
#   fuseki-restore-dump.sh              drill the oldest dump held
#   fuseki-restore-dump.sh <name>       drill a specific one
#   FUSEKI_DRILL_TRUNCATE=1 ...         negative proof: corrupt it first, and
#                                       the drill must FAIL (#3734)
#   FUSEKI_DRILL_NEWEST=1 ...           drill the newest dump instead of the oldest
#   FUSEKI_DRILL_KEEP_AT=<dir> ...      #4399: on PASS keep the restored store at
#                                       <dir> (the nightly demo store) instead of
#                                       discarding it; one restore, two uses
# #4446 — a run that exits non-zero is logged as service.failed (com.chorus.restore-drill).
. "$(dirname "${BASH_SOURCE[0]}")/lib/service-lifecycle.sh"
service_lifecycle_job com.chorus.restore-drill "$@"
set -uo pipefail
. "$(dirname "${BASH_SOURCE[0]}")/chorus-env-setup.sh" >/dev/null 2>&1 || true

REMOTE="${FUSEKI_BACKUP_REMOTE:-Jeffs-Mac-mini.local}"
DEST_BASE="${FUSEKI_BACKUP_DEST:?FUSEKI_BACKUP_DEST unset}"
SCRATCH="${FUSEKI_DRILL_SCRATCH:-$HOME/.chorus/fuseki-drill}"
DATASET="${FUSEKI_DATASET:-pods}"
MIN="${FUSEKI_DRILL_MIN:-1000000}"
WANT="${1:-}"
log(){ echo "$(date '+%F %T') [restore-dump] $*"; }
CHORUS_LOG="${CHORUS_LOG:-${CHORUS_ROOT:-$HOME/CascadeProjects/chorus}/platform/scripts/chorus-log}"
spine(){ "$CHORUS_LOG" "$1" "${DEPLOY_ROLE:-system}" "${@:2}" 2>/dev/null || true; }
fail(){ log "FAIL: $*"; spine ops.restore.drill store=fuseki-dump verdict=fail reason="$*"; exit 1; }
# #4399 — the newest dump must be from the last night. The backup runs at 00:00;
# a newest dump older than this (hours) means a night was missed. Measured from
# the dump's own name, not the file time, so a re-copy cannot make it look fresh.
MAX_AGE_H="${FUSEKI_DRILL_MAX_AGE_H:-26}"

# OLDEST by default, not newest. The newest dump is the one most likely to work
# and least likely to be the one you need; retention (AC4) turns on whether the
# copy about to age out is still restorable.
if [ -z "$WANT" ]; then
  PICK=head; [ "${FUSEKI_DRILL_NEWEST:-0}" = "1" ] && PICK=tail
  WANT="$(ssh -o ConnectTimeout=10 "$REMOTE" "ls -1 '$DEST_BASE/dumps'/${DATASET}_*.nq.gz 2>/dev/null | sort | $PICK -1" | tr -d '\r')"
  WANT="$(basename "${WANT:-}")"
fi
[ -n "$WANT" ] || fail "no dump found under ${REMOTE}:${DEST_BASE}/dumps"
if [ "${FUSEKI_DRILL_NEWEST:-0}" = "1" ]; then
  stamp=$(printf '%s' "$WANT" | grep -oE '[0-9]{4}-[0-9]{2}-[0-9]{2}_[0-9]{2}-[0-9]{2}-[0-9]{2}')
  taken=$(date -j -f '%Y-%m-%d_%H-%M-%S' "$stamp" +%s 2>/dev/null || echo 0)
  age_h=$(( ( $(date +%s) - taken ) / 3600 ))
  [ "$age_h" -le "$MAX_AGE_H" ] || fail "no backup since $WANT (${age_h}h old, limit ${MAX_AGE_H}h): a night was missed"
fi
log "drilling $WANT"

rm -rf "$SCRATCH"; mkdir -p "$SCRATCH/store"
scp -q -o ConnectTimeout=10 "${REMOTE}:${DEST_BASE}/dumps/$WANT" "$SCRATCH/dump.nq.gz" \
  || fail "could not pull $WANT back from $REMOTE"
log "pulled $(du -h "$SCRATCH/dump.nq.gz" | cut -f1)"

# The negative proof. A drill that only ever sees good input cannot show it is
# able to reject bad input, and "it passed" then means nothing. Lopping the tail
# off leaves a file that still looks like a gzip and still has a plausible size.
if [ "${FUSEKI_DRILL_TRUNCATE:-0}" = "1" ]; then
  full=$(stat -f %z "$SCRATCH/dump.nq.gz")
  keep=$(( full / 2 ))
  log "NEGATIVE PROOF: truncating $full -> $keep bytes; this run MUST fail"
  # bs=1m, not bs=1. bs=1 took 7m16s to cut 150 MB (measured 20:50:36-20:57:52)
  # and the wait was the whole run.
  dd if="$SCRATCH/dump.nq.gz" of="$SCRATCH/t.gz" bs=1m count=$(( keep / 1048576 )) 2>/dev/null
  mv "$SCRATCH/t.gz" "$SCRATCH/dump.nq.gz"
fi

log "loading into a scratch store (this is the slow leg)"
if ! gunzip -c "$SCRATCH/dump.nq.gz" > "$SCRATCH/dump.nq" 2>"$SCRATCH/gunzip.err"; then
  fail "the dump did not decompress — $(tail -1 "$SCRATCH/gunzip.err")"
fi
if ! tdb2.tdbloader --loc="$SCRATCH/store" "$SCRATCH/dump.nq" >"$SCRATCH/load.log" 2>&1; then
  fail "tdbloader refused the dump — $(grep -iE 'error|exception' "$SCRATCH/load.log" | head -1)"
fi

# Count by CONTENT, not by exit code. An empty restore exits 0 from every tool
# in this pipeline. And GRAPH ?g, never a bare { ?s ?p ?o }: every triple lives
# in a named graph, and the bare form reads the empty default graph — which
# reported 0 against two healthy stores earlier today.
N=$(tdb2.tdbquery --loc="$SCRATCH/store" 'SELECT (COUNT(*) AS ?n) WHERE { GRAPH ?g { ?s ?p ?o } }' 2>/dev/null \
    | grep -oE '[0-9]+' | tail -1)
[ -n "$N" ] || fail "the restored store did not answer a query"
[ "$N" -ge "$MIN" ] || fail "restored store holds only $N triples (floor $MIN) — not a recovery point"

G=$(tdb2.tdbquery --loc="$SCRATCH/store" 'SELECT (COUNT(DISTINCT ?g) AS ?n) WHERE { GRAPH ?g { ?s ?p ?o } }' 2>/dev/null \
    | grep -oE '[0-9]+' | tail -1)
log "PASS: $WANT restored — $N triples across ${G:-?} graphs"
spine ops.restore.drill store=fuseki-dump verdict=pass dump="$WANT" triples="$N" graphs="${G:-0}"
ssh -o ConnectTimeout=10 "$REMOTE" "printf '%s\n' '$WANT' > '$DEST_BASE/restore-proven.txt'"
rm -f "$SCRATCH/dump.nq"
# #4399 — keep the proven store as the demo store. Moved only after PASS, so a
# failed restore never replaces a working demo store.
if [ -n "${FUSEKI_DRILL_KEEP_AT:-}" ]; then
  rm -rf "$FUSEKI_DRILL_KEEP_AT.new"; mv "$SCRATCH/store" "$FUSEKI_DRILL_KEEP_AT.new" \
    || fail "could not keep the restored store at $FUSEKI_DRILL_KEEP_AT"
  log "kept at $FUSEKI_DRILL_KEEP_AT.new ($(du -sh "$FUSEKI_DRILL_KEEP_AT.new" | cut -f1))"
fi
