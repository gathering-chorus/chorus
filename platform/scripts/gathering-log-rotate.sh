#!/bin/bash
# gathering-log-rotate.sh (#4718) — rotate the gathering stack's launchd logs
# (Fuseki, the app, Loki, Prometheus, Grafana) over a size line.
# #4994: also promtail, vikunja and node-exporter-host (1.0-1.8 GB each on 11-17, never rotated)
# and chorus-api in Library/Logs/Chorus. Never the spine, ~/.chorus/chorus.log: log-archive
# renames that one (it has no long-lived writer) and a truncate here would lose lines.
#
# Copy then truncate, never mv: launchd holds StandardOutPath open, so after an
# mv the service keeps writing into the renamed file, and gzip then unlinks it.
# That blinded Fuseki and Loki 10-17 to 10-20 (6.97 GB + 32.9 GB written into
# deleted files, nothing in Loki). Truncating in place keeps the open fd valid.
set -u
LOG_DIR="${LOG_ROTATE_DIR:-$HOME/Library/Logs/Gathering}"
# a test that points LOG_ROTATE_DIR at a temp dir must never reach a live log: the Chorus dir
# follows it unless named (Borg Queen 17:34: the live chorus-api.log was 1.64 GB)
CHORUS_DIR="${LOG_ROTATE_CHORUS_DIR:-${LOG_ROTATE_DIR:-$HOME/Library/Logs/Chorus}}"
SPINE="${LOG_ROTATE_SPINE:-$HOME/.chorus/chorus.log}"
LIMIT="${LOG_ROTATE_BYTES:-104857600}"
KEEP_DAYS="${LOG_ROTATE_KEEP_DAYS:-7}"
STAMP="$(date +%Y%m%d)"
for FILE in "$LOG_DIR"/{fuseki,gathering-app,loki,prometheus,grafana,promtail,vikunja,node-exporter-host}.log "$CHORUS_DIR/chorus-api.log" ${LOG_ROTATE_EXTRA:-}; do
  LOG="$(basename "$FILE")"
  # the spine named here is a wiring fault: refuse it and fail the run, never pass (Borg Queen 17:36)
  if [ "$FILE" -ef "$SPINE" ]; then echo "log-rotate: refused $FILE: it is the spine, rotated by log-archive" >&2; RC=1; continue; fi
  [ -f "$FILE" ] || continue
  [ "$(stat -f%z "$FILE" 2>/dev/null || echo 0)" -gt "$LIMIT" ] || continue
  cp "$FILE" "$FILE.$STAMP" && : > "$FILE" && echo "log-rotate: $LOG rotated to $LOG.$STAMP.gz"
  gzip -f "$FILE.$STAMP" 2>/dev/null
  find "$(dirname "$FILE")" -name "$LOG.*" -mtime +"$KEEP_DAYS" -delete 2>/dev/null
done
exit "${RC:-0}"
