#!/bin/bash
# gathering-log-rotate.sh (#4718) — rotate the gathering stack's launchd logs
# (Fuseki, the app, Loki, Prometheus, Grafana) over a size line.
#
# Copy then truncate, never mv: launchd holds StandardOutPath open, so after an
# mv the service keeps writing into the renamed file, and gzip then unlinks it.
# That blinded Fuseki and Loki 10-17 to 10-20 (6.97 GB + 32.9 GB written into
# deleted files, nothing in Loki). Truncating in place keeps the open fd valid.
set -u
LOG_DIR="${LOG_ROTATE_DIR:-$HOME/Library/Logs/Gathering}"
LIMIT="${LOG_ROTATE_BYTES:-104857600}"
KEEP_DAYS="${LOG_ROTATE_KEEP_DAYS:-7}"
STAMP="$(date +%Y%m%d)"
for LOG in fuseki.log gathering-app.log loki.log prometheus.log grafana.log; do
  FILE="$LOG_DIR/$LOG"
  [ -f "$FILE" ] || continue
  [ "$(stat -f%z "$FILE" 2>/dev/null || echo 0)" -gt "$LIMIT" ] || continue
  cp "$FILE" "$FILE.$STAMP" && : > "$FILE" && echo "log-rotate: $LOG rotated to $LOG.$STAMP.gz"
  gzip -f "$FILE.$STAMP" 2>/dev/null
  find "$LOG_DIR" -name "$LOG.*" -mtime +"$KEEP_DAYS" -delete 2>/dev/null
done
exit 0
