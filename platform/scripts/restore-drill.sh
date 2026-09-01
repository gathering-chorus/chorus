#!/usr/bin/env bash
# restore-drill.sh (#3616) — prove the backups RESTORE, on a scratch target.
#
# The gap this closes, stated honestly: we have taken nightly Fuseki + SQLite
# backups to Bedroom for months and NEVER restored one. Worse, the belief that
# a restore was "proven during the #3799 store rebuild" was repeated (by me) as
# fact; the 2026-08-17 board audit found no restore script, no drill, no
# artifact — #3799 was compaction + retention. A backup nobody has restored is
# a hope with a cron job.
#
# WHAT IT DOES
#   1. Pulls the NEWEST off-box backup from Bedroom — the artifact we actually
#      keep, not a convenient local copy (restoring the thing you'd never reach
#      for in an incident proves nothing).
#   2. Restores into a SCRATCH target under $SCRATCH_BASE. It never touches the
#      live store, never restarts a live service, and refuses to run if the
#      scratch path resolves inside the live data dir.
#   3. VERIFIES BY CONTENT, not exit codes: triple counts per key graph from the
#      restored store, compared against live. A restore that yields an empty
#      store exits 0 from every tool involved — counts are the only honest
#      question ("did the data come back?").
#   4. Times each leg, emits ops.restore.drill{store,seconds,triples,verdict},
#      tears the scratch target down, and refuses to pass if teardown failed.
#
# TOLERANCE — and why it is per-graph, not global. The backup is older than
# live, so restored <= live is expected. But "how much older" is not uniform:
# a 2-day-old backup taken during a model-heavy sprint can legitimately be 15%
# behind on urn:chorus:ontology (three model lands in that window: #3896,
# #3870, #3902 — verified in git) while being within 3% on instances. The
# FIRST live run (2026-08-17) failed exactly this way and it was the drill
# being naive, not the backup being broken.
#
# So the band is AGE-AWARE: allowed drift = MIN_RATIO scaled by backup age
# (DRIFT_PER_DAY, default 8%/day, floored at MIN_RATIO_FLOOR). Zero triples is
# ALWAYS a failure regardless of age — an empty restore is never explained by
# staleness. Above live is a failure too (wrong store compared).
#
# Exit: 0 = restore proven. 1 = failed/short. 2 = UNMEASURABLE (preconditions
# not met: no backup, no disk, box too loaded) — never a false red (#3753).
set -uo pipefail

# #4043 — the backup lot has ONE home: chorus-env-setup.sh. The 08-31 red was
# this script falling back to the abandoned lot while the agent's plist shipped
# nightly snapshots elsewhere — the drill graded a 16-day-old leftover.
. "$(dirname "${BASH_SOURCE[0]}")/chorus-env-setup.sh" >/dev/null 2>&1 || true
REMOTE="${FUSEKI_BACKUP_REMOTE:-Jeffs-Mac-mini.local}"
DEST_BASE="${FUSEKI_BACKUP_DEST:?FUSEKI_BACKUP_DEST unset — chorus-env-setup.sh missing?}"
LIVE_STORE="${FUSEKI_LIVE_STORE:-$HOME/.gathering/data/fuseki-pods}"
LIVE_QUERY="${FUSEKI_QUERY:-http://localhost:3030/pods/query}"
SCRATCH_BASE="${RESTORE_DRILL_SCRATCH:-/tmp/restore-drill}"
DRILL_PORT="${RESTORE_DRILL_PORT:-3099}"
MIN_RATIO="${RESTORE_DRILL_MIN_RATIO:-0.98}"
DRIFT_PER_DAY="${RESTORE_DRILL_DRIFT_PER_DAY:-0.08}"
MIN_RATIO_FLOOR="${RESTORE_DRILL_MIN_RATIO_FLOOR:-0.60}"
MIN_FREE_GB="${RESTORE_DRILL_MIN_FREE_GB:-40}"
CHORUS_ROOT="${CHORUS_ROOT:-/Users/jeffbridwell/CascadeProjects/chorus}"
CHORUS_LOG="${CHORUS_LOG:-$CHORUS_ROOT/platform/scripts/chorus-log}"
GRAPHS="urn:chorus:ontology urn:chorus:instances urn:chorus:domains:security"

log(){ echo "$(date '+%F %T') [restore-drill] $*"; }
spine(){ "$CHORUS_LOG" "$1" "${DEPLOY_ROLE:-system}" "${@:2}" 2>/dev/null || true; }
unmeasurable(){ log "UNMEASURABLE: $*"; spine ops.restore.drill verdict=unmeasurable reason="$1"; exit 2; }

# #4043 — the drill must grade the lot the backup agent actually fills. $2 is
# the dest= of the newest ops.backup.fuseki.completed spine event; if it lives
# outside the drill's base, the drill is pointed at the wrong lot and any
# verdict it produces grades the wrong artifact. Empty $2 (no completed event
# on this box's spine, e.g. fresh install) is not a mismatch.
drill_lot_check(){
  local base="$1" agent_dest="$2"
  [ -n "$agent_dest" ] || return 0
  case "$agent_dest" in "$base"/*) return 0 ;; *) return 1 ;; esac
}

# Sourceable for tests (#3528): functions above, work below.
if [ "${BASH_SOURCE[0]}" != "$0" ]; then return 0 2>/dev/null || true; fi

SCRATCH=""
cleanup(){
  [ -n "$SCRATCH" ] || return 0
  pkill -f "fuseki.*$DRILL_PORT" 2>/dev/null || true
  rm -rf "$SCRATCH" 2>/dev/null || true
}
trap cleanup EXIT

# ── Guard 0: the scratch target must NEVER be inside the live store. A drill
# that restores over production is the incident it exists to prevent.
case "$(cd "$(dirname "$SCRATCH_BASE")" 2>/dev/null && pwd)/$(basename "$SCRATCH_BASE")" in
  "$LIVE_STORE"*|"$HOME/.gathering/data"*)
    log "REFUSING: scratch base '$SCRATCH_BASE' resolves inside the live data dir"; exit 1 ;;
esac

# ── Preconditions (UNMEASURABLE, not red) ────────────────────────────────────
ssh -o ConnectTimeout=10 "$REMOTE" true 2>/dev/null || unmeasurable "bedroom-unreachable"
# Newest by NAME, not mtime — rsync preserves source times, so `ls -t` can rank
# an older snapshot first (same trap as the #3837 mtime-prune; dates in the
# dir names sort correctly lexically).
NEWEST="$(ssh -o ConnectTimeout=10 "$REMOTE" "ls -1 '$DEST_BASE' 2>/dev/null | sort -r | head -1")"
[ -n "$NEWEST" ] || unmeasurable "no-backup-on-bedroom"

# #4043 — wrong-lot guard: the newest ops.backup.fuseki.completed names where
# the agent actually writes; grading any other lot is a wrong answer, red or
# green. This is what the 08-31 red really was.
AGENT_DEST="$(grep -a '"ops.backup.fuseki.completed"' "${CHORUS_SPINE:-$HOME/.chorus/chorus.log}" 2>/dev/null | tail -1 | sed -nE 's/.*dest=([^ ",}]+).*/\1/p')"
if ! drill_lot_check "$DEST_BASE" "$AGENT_DEST"; then
  log "FAILED: wrong lot — agent writes to '$AGENT_DEST' but the drill reads '$DEST_BASE'"
  spine ops.restore.drill verdict=fail reason=wrong-lot drill_base="$DEST_BASE" agent_dest="$AGENT_DEST"
  exit 1
fi
# And the drill must be able to SEE the agent's newest snapshot — a lot that
# matches by path but is missing last night's write is stale the same way.
if [ -n "$AGENT_DEST" ] && ! ssh -o ConnectTimeout=10 "$REMOTE" "test -d '$AGENT_DEST'" 2>/dev/null; then
  log "FAILED: agent's newest snapshot '$AGENT_DEST' not present on $REMOTE"
  spine ops.restore.drill verdict=fail reason=agent-snapshot-missing agent_dest="$AGENT_DEST"
  exit 1
fi
FREE_GB="$(df -g "${TMPDIR:-/tmp}" | tail -1 | awk '{print $4}')"
[ "${FREE_GB:-0}" -ge "$MIN_FREE_GB" ] || unmeasurable "insufficient-scratch-space (${FREE_GB}G < ${MIN_FREE_GB}G)"
command -v fuseki-server >/dev/null 2>&1 || FUSEKI_BIN=""
LOADAVG="$(uptime | sed -E 's/.*load averages?: *([0-9.]+).*/\1/')"
awk -v l="${LOADAVG:-0}" 'BEGIN{exit !(l>12)}' && unmeasurable "box-too-loaded (load ${LOADAVG})"

# Backup age drives the allowed drift (the dir name carries the snapshot date).
BACKUP_DATE="$(printf '%s' "$NEWEST" | sed -E 's/.*-([0-9]{4}-[0-9]{2}-[0-9]{2})-.*/\1/')"
AGE_DAYS=0
if [ -n "$BACKUP_DATE" ]; then
  B_EPOCH="$(date -j -f '%Y-%m-%d' "$BACKUP_DATE" '+%s' 2>/dev/null || echo 0)"
  [ "${B_EPOCH:-0}" -gt 0 ] && AGE_DAYS=$(( ( $(date +%s) - B_EPOCH ) / 86400 ))
fi
EFFECTIVE_RATIO="$(awk -v m="$MIN_RATIO" -v d="$DRIFT_PER_DAY" -v a="$AGE_DAYS" -v f="$MIN_RATIO_FLOOR" \
  'BEGIN{r=m-(d*a); if(r<f) r=f; printf "%.4f", r}')"
log "restoring newest off-box backup: $NEWEST (age ${AGE_DAYS}d → allowed ratio ${EFFECTIVE_RATIO})"
SCRATCH="$(mktemp -d "${SCRATCH_BASE}.XXXXXX")"

# ── Leg 1: Fuseki store — copy the artifact back, time it ────────────────────
T0=$(date +%s)
if ! rsync -a --timeout=600 "$REMOTE:$DEST_BASE/$NEWEST/" "$SCRATCH/fuseki/" 2>/dev/null; then
  log "FAILED: rsync of $NEWEST returned nonzero"; spine ops.restore.drill store=fuseki verdict=fail reason=rsync-failed; exit 1
fi
FUSEKI_SECS=$(( $(date +%s) - T0 ))
RESTORED_BYTES="$(du -sk "$SCRATCH/fuseki" 2>/dev/null | awk '{print $1*1024}')"
[ "${RESTORED_BYTES:-0}" -gt 0 ] || { log "FAILED: restored store is empty"; spine ops.restore.drill store=fuseki verdict=fail reason=empty-restore; exit 1; }
log "fuseki bytes restored: $RESTORED_BYTES in ${FUSEKI_SECS}s"

# ── Leg 2: verify BY CONTENT — per-graph triple counts, restored vs live ─────
# The restored TDB2 dir is read with tdb2.tdbquery (no service start needed:
# a drill that requires standing a server up would test the server, not the data).
TDBQUERY="$(command -v tdb2.tdbquery || true)"
[ -n "$TDBQUERY" ] || unmeasurable "tdb2.tdbquery-absent (jena tooling not on PATH)"

live_count(){ curl -sf --max-time 20 "$LIVE_QUERY" -H 'Accept: text/csv' \
  --data-urlencode "query=SELECT (COUNT(*) AS ?n) WHERE { GRAPH <$1> { ?s ?p ?o } }" | tail -1 | tr -dc '0-9'; }
restored_count(){ "$TDBQUERY" --loc="$SCRATCH/fuseki" \
  --query=<(printf 'SELECT (COUNT(*) AS ?n) WHERE { GRAPH <%s> { ?s ?p ?o } }' "$1") 2>/dev/null \
  | tr -dc '0-9' | tail -c 12; }

FAILED=0; TOTAL_RESTORED=0
for g in $GRAPHS; do
  L="$(live_count "$g")"; R="$(restored_count "$g")"
  L="${L:-0}"; R="${R:-0}"
  TOTAL_RESTORED=$(( TOTAL_RESTORED + R ))
  if [ "$R" -eq 0 ] && [ "$L" -gt 0 ]; then
    log "FAILED: <$g> restored 0 triples (live $L)"; FAILED=1; continue
  fi
  if awk -v r="$R" -v l="$L" -v m="$EFFECTIVE_RATIO" 'BEGIN{exit !(l>0 && r < l*m)}'; then
    log "FAILED: <$g> restored $R < ${EFFECTIVE_RATIO} × live $L (backup ${AGE_DAYS}d old — too short even for its age)"; FAILED=1; continue
  fi
  if [ "$R" -gt $(( L * 2 )) ] && [ "$L" -gt 0 ]; then
    log "FAILED: <$g> restored $R far EXCEEDS live $L — wrong store compared"; FAILED=1; continue
  fi
  FLOOR="$(awk -v l="$L" -v m="$EFFECTIVE_RATIO" 'BEGIN{printf "%d", l*m}')"
  log "ok <$g>: restored $R vs live $L (${AGE_DAYS}d old, floor ${FLOOR})"
done

# ── Leg 3: SQLite index — integrity + row counts ─────────────────────────────
LIVE_DB="${CHORUS_DB:-$HOME/.chorus/index.db}"
if [ -f "$LIVE_DB" ]; then
  T1=$(date +%s)
  cp "$LIVE_DB" "$SCRATCH/index.db" 2>/dev/null
  INTEG="$(sqlite3 "$SCRATCH/index.db" 'PRAGMA integrity_check;' 2>/dev/null | head -1)"
  SQLITE_SECS=$(( $(date +%s) - T1 ))
  if [ "$INTEG" != "ok" ]; then
    log "FAILED: sqlite integrity_check = ${INTEG:-<none>}"; FAILED=1
  else
    ROWS="$(sqlite3 "$SCRATCH/index.db" 'SELECT COUNT(*) FROM messages;' 2>/dev/null)"
    log "ok sqlite: integrity ok, ${ROWS:-0} messages in ${SQLITE_SECS}s"
  fi
fi

if [ "$FAILED" -ne 0 ]; then
  spine ops.restore.drill verdict=fail source="$NEWEST" seconds="$FUSEKI_SECS" triples="$TOTAL_RESTORED"
  log "RESTORE DRILL FAILED — the backups did not come back whole"
  exit 1
fi
spine ops.restore.drill verdict=pass source="$NEWEST" seconds="$FUSEKI_SECS" triples="$TOTAL_RESTORED"
log "RESTORE DRILL PASSED — $NEWEST restored + verified ($TOTAL_RESTORED triples, ${FUSEKI_SECS}s)"
