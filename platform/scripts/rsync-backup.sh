#!/usr/bin/env bash
# rsync-backup.sh — nightly Library→Bedroom backup (#1761)
# Backs up irreplaceable data: session history, config, code, keys.
# Runs via LaunchAgent at 2am. Logs to ~/Library/Logs/Chorus/rsync-backup.log
set -euo pipefail

REMOTE="${BACKUP_REMOTE:-Jeffs-Mac-mini.local}"
DEST="${BACKUP_DEST:-/Users/jeffbridwell/Backups/library}"
RSYNC="${BACKUP_RSYNC:-rsync}"
SSH="${BACKUP_SSH:-ssh}"
LOG_TAG="rsync-backup"
CHORUS_ROOT="${CHORUS_ROOT:-/Users/jeffbridwell/CascadeProjects/chorus}"
CHORUS_LOG="${CHORUS_ROOT}/platform/scripts/chorus-log"

log() { echo "$(date '+%Y-%m-%d %H:%M:%S') [$LOG_TAG] $1"; }

# Verify Bedroom is reachable
if ! "$SSH" -o ConnectTimeout=10 "$REMOTE" "true" 2>/dev/null; then
  log "ERROR: Bedroom unreachable — backup skipped"
  "$CHORUS_LOG" ops.backup.failed silas reason="bedroom-unreachable" 2>/dev/null || true
  exit 1
fi

FAILED=0
SYNCED=0

backup() {
  local src="$1"
  local name="$2"
  shift 2
  # #4630 AC3 — key files (relative to the source's parent) whose copy must end
  # at least as big as the source was when the copy began; a copy that stayed
  # behind (the spine froze at 4.40 GB from 10-13 while rsync said OK) is a FAIL.
  local keys=("$@") key before=()
  for key in ${keys[@]+"${keys[@]}"}; do
    before+=("$(stat -f%z "$(dirname "$src")/$key" 2>/dev/null || echo 0)")
  done
  if [ ! -e "$src" ]; then
    log "SKIP: $name ($src not found)"
    return
  fi
  log "START: $name ($src)"
  # #4043 — one retry: live dirs (.chorus especially) mutate under the copy and
  # rsync exits 1/23/24 on "unexpected end of file" / vanished files. A second
  # pass over the mostly-synced tree is fast and usually lands clean; a target
  # that fails TWICE is a real failure and still reports FAIL.
  local attempt rc=1
  for attempt in 1 2; do
    # #4630 — capture rsync's own exit code. `rc=$?` read after an if with no
    # else is the if's status, always 0, so two failed copies logged
    # "RETRY (exit 0)" then "OK" every night from 10-13 while the spine copy
    # on Bedroom stayed frozen at 4.40 GB.
    rc=0
    nice -n 10 "$RSYNC" -az --delete --timeout=300 \
      --exclude='data/loki/' \
      --exclude='node_modules/' \
      --exclude='.git/objects/' \
      --exclude='target/release/' \
      --exclude='target/debug/' \
      -e "ssh -o ConnectTimeout=10" "$src" "${REMOTE}:${DEST}/${name}/" 2>&1 || rc=$?
    [ "$rc" = 0 ] && break
    [ "$attempt" = 1 ] && { log "RETRY: $name (rsync exit $rc — files changed under the copy?)"; sleep 15; }
  done
  if [ "$rc" = 0 ]; then
    local i copied
    for i in ${keys[@]+"${!keys[@]}"}; do
      copied=$("$SSH" -o ConnectTimeout=10 "$REMOTE" "stat -f%z '${DEST}/${name}/${keys[$i]}'" 2>/dev/null || echo 0)
      if [ "${copied:-0}" -lt "${before[$i]}" ]; then
        log "STALE: $name/${keys[$i]} copy is ${copied:-0} bytes, source was ${before[$i]} when the copy began"
        rc=97
      fi
    done
  fi
  if [ "$rc" = 0 ]; then
    log "OK: $name"
    SYNCED=$((SYNCED + 1))
  else
    if [ "$rc" = 97 ]; then log "FAIL: $name (copy is behind its source)"; else log "FAIL: $name (rsync exit $rc after 2 attempts)"; fi
    FAILED=$((FAILED + 1))
  fi
}

log "=== Backup starting ==="

backup "$HOME/.claude"                          "dot-claude"
backup "$HOME/.chorus"                          "dot-chorus"       ".chorus/chorus.log"
backup "$HOME/.ssh"                             "dot-ssh"
backup "$HOME/CascadeProjects"                  "CascadeProjects"
backup "$HOME/Library/LaunchAgents"             "LaunchAgents"

log "=== Backup complete: ${SYNCED} synced, ${FAILED} failed ==="

if [ "$FAILED" -gt 0 ]; then
  "$CHORUS_LOG" ops.backup.partial silas synced="$SYNCED" failed="$FAILED" 2>/dev/null || true
  exit 1
else
  "$CHORUS_LOG" ops.backup.completed silas synced="$SYNCED" 2>/dev/null || true
fi

# --- RESTORE PROCEDURE (#2043) ---
# To restore from Bedroom backup to Library:
#
# 1. ALWAYS dry-run first:
#    rsync -azn --stats -e ssh Jeffs-Mac-mini.local:/Users/jeffbridwell/Backups/library/<target>/ ~/<dest>/
#
# 2. Review dry-run output — check "Number of files transferred"
#
# 3. Remove -n flag to execute the actual restore
#
# 4. Restore order:
#    a. dot-ssh (28K, instant — need SSH keys for everything else)
#    b. LaunchAgents (268K, instant — service configs)
#    c. dot-chorus (1.6GB, ~2 min — Chorus runtime state)
#    d. dot-claude (3.2GB, ~4 min — Claude session state)
#    e. CascadeProjects (26GB, ~30 min — all code repos)
#
# Estimated total restore: ~35-45 minutes over WiFi
# Tested: 2026-04-14, dry-run 2s for dot-claude (3.2GB, 16K files)
