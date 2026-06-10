#!/bin/bash
# chorus-eventloop-probe-worker.sh — run the eventloop detector OUTSIDE chorus-api (#3082)
#
# Launched by the com.chorus.eventloop-probe LaunchAgent as a PERSISTENT process
# (KeepAlive=true, NOT StartInterval). Unlike chorus-reindex-worker.sh — a single
# pass per interval — this is a continuous detector: it probes chorus-api's response
# latency every ~2s forever and fires a block alert when the loop stops answering.
#
# WHY a separate process: the in-process detector (eventloop-alert.ts, the `blocked`
# library) rides the very loop it measures — a hard block starves its own timer, so it
# UNDER-reports its own blocks. This worker's loop is idle, so it measures chorus-api's
# loop accurately from outside via probe latency (#3080 Track A / ADR-034).
#
# ACTIVATE-BEFORE-RETIRE: this worker ships and is verified LIVE before the in-process
# startEventloopAlert() in server.ts is retired — never a window with no detector.

set -euo pipefail

WORKER_JS="${CHORUS_EVENTLOOP_PROBE_JS:-$HOME/CascadeProjects/chorus/platform/api/dist/eventloop-probe.js}"
# Pin node by ABSOLUTE path, not via PATH (same discipline as the reindex worker —
# a wrong node off PATH silently broke #3085). Single source of the node version.
NODE_BIN="${CHORUS_NODE_BIN:-/Users/jeffbridwell/.nvm/versions/node/v20.11.1/bin/node}"
LOGFILE="${HOME}/.chorus/eventloop-probe.log"

log() { echo "$(date '+%Y-%m-%d %H:%M:%S') $1" >> "$LOGFILE"; }

if [ ! -f "$WORKER_JS" ]; then
  log "worker not built ($WORKER_JS) — skipping (deploy chorus-api to build dist/)"
  exit 0
fi

if [ ! -x "$NODE_BIN" ]; then
  log "node not found/executable at $NODE_BIN (set CHORUS_NODE_BIN) — refusing to run on a wrong node"
  exit 1
fi

log "starting eventloop-probe worker (persistent)"
# exec so the LaunchAgent supervises the node process directly (KeepAlive restarts it
# if it ever exits). The worker loops forever — it never returns under normal operation.
exec "$NODE_BIN" "$WORKER_JS" >> "$LOGFILE" 2>&1
