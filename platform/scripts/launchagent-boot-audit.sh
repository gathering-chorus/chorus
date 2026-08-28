#!/usr/bin/env bash
# launchagent-boot-audit.sh — #4027 boot-herd guard.
#
# An interval job (StartInterval / StartCalendarInterval, no KeepAlive) with
# RunAtLoad=true fires at boot on top of every long-running service. 18 of
# them did on 2026-08-28 — load 527 at 5 min uptime, third hard crash in two
# days. Interval jobs lose nothing without RunAtLoad: launchd fires them at
# +interval. Services (KeepAlive) legitimately keep RunAtLoad.
#
# Usage: launchagent-boot-audit.sh [--fix] [dir]   (default dir: ~/Library/LaunchAgents)
#   exit 0 clean · 1 violations (one line each: label + file) · 2 dir missing/empty
#   --fix  strips RunAtLoad from the offending plists in place (plutil).
set -uo pipefail
FIX=0
[ "${1:-}" = "--fix" ] && { FIX=1; shift; }
DIR="${1:-$HOME/Library/LaunchAgents}"
[ -d "$DIR" ] || { echo "boot-audit: no such dir: $DIR" >&2; exit 2; }
shopt -s nullglob
files=("$DIR"/*.plist)
[ ${#files[@]} -gt 0 ] || { echo "boot-audit: no plists in $DIR" >&2; exit 2; }

bad=0
for f in "${files[@]}"; do
  ral=$(plutil -extract RunAtLoad raw "$f" 2>/dev/null) || continue
  [ "$ral" = "true" ] || continue
  ka=$(plutil -extract KeepAlive raw "$f" 2>/dev/null) || ka=""
  # KeepAlive true, or a KeepAlive dict (SuccessfulExit/PathState…) = a service
  [ -z "$ka" ] || [ "$ka" = "false" ] || continue
  si=$(plutil -extract StartInterval raw "$f" 2>/dev/null) || si=""
  sc=$(plutil -extract StartCalendarInterval raw "$f" 2>/dev/null) || sc=""
  [ -n "$si$sc" ] || continue
  label=$(plutil -extract Label raw "$f" 2>/dev/null || basename "$f" .plist)
  if [ $FIX -eq 1 ]; then
    # textual removal, not plutil -remove: plutil re-serializes the whole file
    # (key reorder + tabs) and buries a 2-line change in a 40-line diff.
    perl -0pi -e 's/[ \t]*<key>RunAtLoad<\/key>\s*<(true|false)\/>[ \t]*\n//' "$f" \
      && plutil -lint -s "$f" && echo "fixed: $label ($f)"
  else
    echo "boot-herd: $label RunAtLoad=true with interval=${si:-calendar} ($f)"
    bad=$((bad+1))
  fi
done
[ $bad -eq 0 ] && exit 0
echo "boot-audit: $bad interval agent(s) fire at boot — run: $0 --fix $DIR" >&2
exit 1
