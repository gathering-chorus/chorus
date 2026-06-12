#!/usr/bin/env bash
# rsync-backup.sh — nightly Library→Bedroom backup (#1761)
# Backs up irreplaceable data: session history, config, code, keys.
# Runs via LaunchAgent at 2am. Logs to ~/Library/Logs/Chorus/rsync-backup.log
set -euo pipefail

REMOTE="Jeffs-Mac-mini.local"
DEST="/Users/jeffbridwell/Backups/library"
LOG_TAG="rsync-backup"
CHORUS_ROOT="${CHORUS_ROOT:-/Users/jeffbridwell/CascadeProjects/chorus}"
CHORUS_LOG="${CHORUS_ROOT}/platform/scripts/chorus-log"

log() { echo "$(date '+%Y-%m-%d %H:%M:%S') [$LOG_TAG] $1"; }

# Verify Bedroom is reachable
if ! ssh -o ConnectTimeout=10 "$REMOTE" "true" 2>/dev/null; then
  log "ERROR: Bedroom unreachable — backup skipped"
  "$CHORUS_LOG" ops.backup.failed silas reason="bedroom-unreachable" 2>/dev/null || true
  exit 1
fi

FAILED=0
SYNCED=0

backup() {
  local src="$1"
  local name="$2"
  if [ ! -e "$src" ]; then
    log "SKIP: $name ($src not found)"
    return
  fi
  log "START: $name ($src)"
  if rsync -az --delete --timeout=300 \
    --exclude='data/loki/' \
    --exclude='node_modules/' \
    --exclude='.git/objects/' \
    --exclude='target/release/' \
    --exclude='target/debug/' \
    -e "ssh -o ConnectTimeout=10" "$src" "${REMOTE}:${DEST}/${name}/" 2>&1; then
    log "OK: $name"
    SYNCED=$((SYNCED + 1))
  else
    log "FAIL: $name (rsync exit $?)"
    FAILED=$((FAILED + 1))
  fi
}

log "=== Backup starting ==="

backup "$HOME/.claude"                          "dot-claude"
backup "$HOME/.chorus"                          "dot-chorus"
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
