#!/usr/bin/env bash
# @test-type: integration — reads the live fuseki.log and the live log config
# @domain: persistence
# @needs: machine — prod after the land, the config deploy and one Fuseki restart
# @card: #4946 · owner: silas
# #4946 AC2 — in prod, fuseki.log grows by fewer than 34 lines in 5 minutes (under
# 100 per 15m; before the fix it wrote ~700 lines in 20 s). 5 minutes, not 15, so
# the after-land prove is not held for a quarter hour.
# Fuseki's lines carry only HH:MM:SS, so the window is read from the log's lines,
# not its timestamps: count the lines written in WINDOW seconds by sampling the
# length twice. A log that is not being written by a running Fuseki is RED, never
# a vacuous pass (the size check is meaningless if Fuseki is down).
set -u
# Fuseki runs as jeffbridwell (com.gathering.fuseki), and so does the land that proves this
LOG="${FUSEKI_LOG:-$HOME/Library/Logs/Gathering/fuseki.log}"
CONF="${FUSEKI_LOG_CONF:-$HOME/.gathering/data/fuseki-5.1.0/log4j2.properties}"
URL="${FUSEKI_URL:-http://localhost:3030}"
WINDOW="${WINDOW_SECS:-300}"
MAX="${MAX_LINES:-34}"
[ -r "$LOG" ] || { echo "RED: cannot read $LOG"; exit 1; }
curl -sf -o /dev/null --max-time 5 "$URL/\$/ping" || { echo "RED: Fuseki at $URL does not answer; a quiet log proves nothing"; exit 1; }
grep -qE '^logger\.fuseki-fuseki\.level[[:space:]]*=[[:space:]]*WARN' "$CONF" \
  || { echo "RED: $CONF still has the Fuseki action logger below WARN (not deployed)"; exit 1; }
a=$(wc -l < "$LOG")
sleep "$WINDOW" || { echo "RED: the ${WINDOW}s wait was cut short; nothing measured"; exit 1; }
b=$(wc -l < "$LOG")
[ "$b" -ge "$a" ] || { echo "RED: $LOG shrank ($a -> $b lines); it was rotated mid-window, run again"; exit 1; }
n=$((b - a))
[ "$n" -lt "$MAX" ] || { echo "RED: fuseki.log grew $n lines in ${WINDOW}s (limit $MAX)"; tail -2 "$LOG" | cut -c1-160; exit 1; }
echo "GREEN: fuseki.log grew $n lines in ${WINDOW}s (limit $MAX)"
