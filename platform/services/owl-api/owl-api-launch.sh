#!/usr/bin/env bash
# owl-api launch wrapper — canonical source (#3446).
#
# Two jobs:
#   1. (#3402) Source the gitignored realm secret so CHORUS_SERVICE_TOKEN_SECRET
#      reaches owl-api's env WITHOUT the value landing in a checked-in file or the
#      plist. Shared HS256 now; asymmetric public-key verify is the open-source-gate
#      end-state (ADR-042).
#   2. (#3446) Wait for Fuseki to be ready before exec. owl-api queries Fuseki
#      (CHORUS_FUSEKI, default localhost:3030) at startup; at boot it races ahead of
#      Fuseki, the query fails, owl-api exits, and KeepAlive crash-loops it (4 runs
#      observed 2026-06-16 after a clean reboot). Gating on Fuseki readiness turns the
#      crash-loop into a graceful wait.
#
# Deploy: this is the source of truth. Install copies it to ~/.chorus/bin/owl-api-launch.sh
# (the plist's ProgramArguments points there). Silas/ADR-012, DEC-022.

set -uo pipefail

# --- Fuseki readiness gate (#3446) ---
FUSEKI_BASE="${CHORUS_FUSEKI:-http://localhost:3030/pods}"
# Derive scheme://host:port from CHORUS_FUSEKI and ping Fuseki's admin endpoint.
FUSEKI_ORIGIN="$(printf '%s' "$FUSEKI_BASE" | sed -E 's#(https?://[^/]+).*#\1#')"
FUSEKI_PING="${FUSEKI_ORIGIN}/\$/ping"

wait_secs=0
max_secs=120
until curl -sf -o /dev/null --max-time 3 "$FUSEKI_PING"; do
  if [ "$wait_secs" -ge "$max_secs" ]; then
    echo "owl-api-launch: Fuseki not ready after ${max_secs}s at ${FUSEKI_PING}; starting anyway" >&2
    break
  fi
  echo "owl-api-launch: waiting for Fuseki at ${FUSEKI_PING} (${wait_secs}s)" >&2
  sleep 2
  wait_secs=$((wait_secs + 2))
done

# --- secret sourcing (#3402) ---
set -a
[ -f "$HOME/.chorus/secrets/chorus-realm.env" ] && . "$HOME/.chorus/secrets/chorus-realm.env"
set +a

exec "$HOME/.chorus/bin/owl-api" "$@"
