#!/usr/bin/env bash
# athena-make launch wrapper — canonical source (#3446).
#
# Two jobs:
#   1. (#3719) Export the identity-door env (CSS issuer + local JWKS URL). The
#      HS256 realm secret this wrapper used to source is RETIRED — the door
#      verifies CSS ES256 identity tokens; scope is model data (chorus:hasScope).
#   2. (#3446) Wait for Fuseki to be ready before exec. athena-make queries Fuseki
#      (CHORUS_FUSEKI, default localhost:3030) at startup; at boot it races ahead of
#      Fuseki, the query fails, athena-make exits, and KeepAlive crash-loops it (4 runs
#      observed 2026-06-16 after a clean reboot). Gating on Fuseki readiness turns the
#      crash-loop into a graceful wait.
#
# Deploy: this is the source of truth. Install copies it to ~/.chorus/bin/athena-make-launch.sh
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
    echo "athena-make-launch: Fuseki not ready after ${max_secs}s at ${FUSEKI_PING}; starting anyway" >&2
    break
  fi
  echo "athena-make-launch: waiting for Fuseki at ${FUSEKI_PING} (${wait_secs}s)" >&2
  sleep 2
  wait_secs=$((wait_secs + 2))
done

# --- identity-door env (#3719; JWKS derives from the issuer in-process) ---
set -a
export CSS_ISSUER="${CSS_ISSUER:-https://id.lightlifeurbangardens.com/}"

# #3611 UNTANGLE (was #3641) — carry the Fuseki write credential into athena-make's env
# so its chorus-model spawns can -u their :3030 writes. The credential's home is the
# shared-infra file beside the store ($FUSEKI_BASE/fuseki-write.env, 0600, provisioned
# by fuseki-shiro-deploy.sh) — athena-make no longer reads gathering's repo tree. Targeted
# extract of just the two keys, never a full source. Empty until provisioned →
# chorus-model writes anon → a 401 at the door, fail-visible.
_credenv="${FUSEKI_WRITE_ENV:-$HOME/.gathering/data/fuseki-write.env}"
if [ -r "$_credenv" ]; then
  export FUSEKI_ADMIN_PASSWORD="$(grep -E '^FUSEKI_ADMIN_PASSWORD=' "$_credenv" | head -1 | cut -d= -f2-)"
  export FUSEKI_ADMIN_USER="$(grep -E '^FUSEKI_ADMIN_USER=' "$_credenv" | head -1 | cut -d= -f2-)"
  [ -z "$FUSEKI_ADMIN_USER" ] && export FUSEKI_ADMIN_USER=admin
fi
set +a

exec "$HOME/.chorus/bin/athena-make" "$@"
