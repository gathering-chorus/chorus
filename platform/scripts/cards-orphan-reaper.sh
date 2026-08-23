#!/usr/bin/env bash
# cards-orphan-reaper.sh — reap orphaned cards-CLI node processes
# Card #3989 | LaunchAgent: com.chorus.cards-orphan-reaper
#
# Backstop for the #3989 spawner fix: any cards-CLI node/ts-node process that
# has been reparented to launchd (ppid=1) is by definition abandoned — the
# caller that spawned it is gone, nobody will read its output, and overnight
# 2026-08-23 an accumulation of 313 of them (38GB swap, load 150) took both
# apps down and forced a hard power-cycle.
#
# Match predicate (ALL must hold — deliberately narrow, see the negative-proof
# tests in tests/cards-orphan-reaper.bats):
#   1. ppid == 1  (orphaned to launchd)
#   2. command contains a cards-CLI marker:
#        directing/products/cards/src/cli.ts
#        directing/products/cards/dist/cli.js
#        or both "ts-node" and "products/cards"
#   3. elapsed age > MIN_AGE_SECS (default 120 — never races a live invocation;
#      the CLI's own #3347 watchdog exits at 30s, so 120s alive = provably wedged)
#
# Usage: cards-orphan-reaper.sh [--dry-run] [--ps-file FILE] [--min-age-secs N]
#   --ps-file lets tests inject a captured snapshot (hermetic, #3528 — a test
#   brings its own world); format = `ps -axo pid=,ppid=,etime=,command=` lines.
# Logs structured JSON to stdout (Promtail via LaunchAgent, same contract as
# tmp-reaper.sh); emits cards.orphan.reaped spine events via chorus-log.

set -uo pipefail

DRY_RUN="${DRY_RUN:-0}"
PS_FILE=""
MIN_AGE_SECS="${CARDS_REAPER_MIN_AGE_SECS:-120}"

while [ $# -gt 0 ]; do
  case "$1" in
    --dry-run) DRY_RUN=1 ;;
    --ps-file) PS_FILE="$2"; shift ;;
    --min-age-secs) MIN_AGE_SECS="$2"; shift ;;
    *) echo "unknown arg: $1" >&2; exit 2 ;;
  esac
  shift
done

log() {
  local level="$1" message="$2"
  local ts
  ts=$(TZ=America/New_York date '+%Y-%m-%d %H:%M:%S')
  echo "{\"timestamp\":\"$ts\",\"level\":\"$level\",\"appName\":\"cards-orphan-reaper\",\"message\":\"$message\"}"
}

# etime ([[dd-]hh:]mm:ss) → seconds
etime_secs() {
  local e="$1" days=0 rest
  case "$e" in
    *-*) days="${e%%-*}"; rest="${e#*-}" ;;
    *)   rest="$e" ;;
  esac
  local IFS=':' parts=() secs=0
  read -ra parts <<< "$rest"
  for p in "${parts[@]}"; do secs=$(( secs * 60 + 10#$p )); done
  echo $(( days * 86400 + secs ))
}

matches_cards_cli() {
  local cmd="$1"
  case "$cmd" in
    *directing/products/cards/src/cli.ts*) return 0 ;;
    *directing/products/cards/dist/cli.js*) return 0 ;;
    *ts-node*products/cards*) return 0 ;;
  esac
  return 1
}

snapshot() {
  if [ -n "$PS_FILE" ]; then
    cat "$PS_FILE"
  else
    ps -axo pid=,ppid=,etime=,command=
  fi
}

REAPED=0
SKIPPED=0

while IFS= read -r line; do
  [ -n "$line" ] || continue
  # columns: pid ppid etime command...
  set -- $line
  [ $# -ge 4 ] || continue
  pid="$1"; ppid="$2"; etime="$3"; shift 3
  cmd="$*"
  [ "$ppid" = "1" ] || continue
  matches_cards_cli "$cmd" || continue
  age=$(etime_secs "$etime")
  if [ "$age" -le "$MIN_AGE_SECS" ]; then
    SKIPPED=$((SKIPPED+1))
    log info "skip pid=$pid age=${age}s <= min-age ${MIN_AGE_SECS}s"
    continue
  fi
  if [ "$DRY_RUN" = "1" ]; then
    log info "DRY RUN: would reap pid=$pid age=${age}s cmd=${cmd:0:120}"
    REAPED=$((REAPED+1))
    continue
  fi
  if kill -9 "$pid" 2>/dev/null; then
    REAPED=$((REAPED+1))
    log info "reaped pid=$pid age=${age}s cmd=${cmd:0:120}"
    if command -v chorus-log >/dev/null 2>&1; then
      chorus-log cards.orphan.reaped system "pid=$pid" "age_s=$age" >/dev/null 2>&1 || true
    fi
  else
    log warn "kill failed pid=$pid (already gone?)"
  fi
done < <(snapshot)

log info "done reaped=$REAPED skipped_young=$SKIPPED dry_run=$DRY_RUN"
echo "REAPED=$REAPED"
