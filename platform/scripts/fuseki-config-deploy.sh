#!/usr/bin/env bash
# fuseki-config-deploy.sh (#4718, #4946) — put the canonical Fuseki configs in
# place: the server config (#4718) and the log4j2 log config (#4946). Fuseki reads
# them only at start, so this never restarts anything itself.
#
#   --check   exit 0 if every runtime file equals its canonical one, 1 if not (diff shown)
#   (none)    back up each runtime file that differs beside itself, then install the canonical one
#
# A restart is a separate, asked-for step (it blips the door for ~5 s):
#   launchctl kickstart -k gui/$(id -u)/com.gathering.fuseki
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SERVER_CANONICAL="${FUSEKI_SERVER_CANONICAL:-$SCRIPT_DIR/launchagents-canonical/fuseki-config-server.ttl.in}"
SERVER_RUNTIME="${FUSEKI_SERVER_RUNTIME:-$HOME/.gathering/data/config.ttl}"
# fuseki-server reads $FUSEKI_HOME/log4j2.properties; FUSEKI_HOME is the dir the
# LaunchAgent's fuseki-server lives in (com.gathering.fuseki.plist).
LOG_CANONICAL="${FUSEKI_LOG_CANONICAL:-$SCRIPT_DIR/launchagents-canonical/fuseki-log4j2.properties}"
LOG_RUNTIME="${FUSEKI_LOG_RUNTIME:-$HOME/.gathering/data/fuseki-5.1.0/log4j2.properties}"

PAIRS=("$SERVER_CANONICAL|$SERVER_RUNTIME|pre-4718" "$LOG_CANONICAL|$LOG_RUNTIME|pre-4946")

for p in "${PAIRS[@]}"; do
  c="${p%%|*}"
  [ -r "$c" ] || { echo "fuseki-config-deploy: canonical $c missing" >&2; exit 2; }
done

if [ "${1:-}" = "--check" ]; then
  rc=0
  for p in "${PAIRS[@]}"; do
    IFS='|' read -r c r _ <<<"$p"
    if cmp -s "$c" "$r"; then
      echo "fuseki-config-deploy: $r matches canonical"
    else
      echo "fuseki-config-deploy: $r differs from canonical" >&2
      diff -u "$r" "$c" >&2 || true   # diff exits 1 on a difference; rc=1 below carries it
      rc=1
    fi
  done
  exit "$rc"
fi

for p in "${PAIRS[@]}"; do
  IFS='|' read -r c r tag <<<"$p"
  if [ -e "$r" ] && ! cmp -s "$c" "$r"; then
    cp -p "$r" "$r.$tag.$(date +%Y%m%dT%H%M%S)"
  fi
  install -m 0644 "$c" "$r"
  echo "fuseki-config-deploy: installed $r (takes effect on the next Fuseki start)"
done
