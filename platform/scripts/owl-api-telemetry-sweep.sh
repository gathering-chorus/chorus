#!/bin/bash
# owl-api telemetry retention sweep (#3354) — prunes dated telemetry files
# older than RETENTION_DAYS (default 14). Dated files = free day-boundary
# rotation (Kade's design catch); this sweep is the other half.
# Wire into the existing cruft/tmp-reaper cadence or run ad hoc.
set -euo pipefail
CHORUS_HOME="${CHORUS_HOME:-/Users/jeffbridwell/CascadeProjects/chorus}"
RETENTION_DAYS="${RETENTION_DAYS:-14}"
find "$CHORUS_HOME/ops/logs" -name "owl-api-*.jsonl" -mtime +"$RETENTION_DAYS" -print -delete 2>/dev/null | sed 's/^/pruned: /'
echo "owl-api telemetry sweep done (retention ${RETENTION_DAYS}d)"
