#!/bin/bash
# chorus-api-wrapper.sh — runs the Chorus API server as a LaunchAgent.
# Sources secrets from jeff-bridwell-personal-site/.env (same creds the app uses)
# so Cost aggregation and Twilio-backed endpoints work without duplicating .env.

set -euo pipefail

# #3197 — source the ONE env file. CHORUS_ROOT/HOME/WERK_BASE/BIN +
# WERK_<ROLE>_BIN land in this process's env (and so in the node app's
# process.env), instead of each consumer re-deriving them. This is also what
# lets werk-targeted deploys spawned by the API resolve a role's bin slot.
source "$(dirname "${BASH_SOURCE[0]}")/chorus-env-setup.sh"

API_DIR="$CHORUS_ROOT/platform/api"
APP_ENV="$CHORUS_ROOT/../jeff-bridwell-personal-site/.env"

if [ -f "$APP_ENV" ]; then
  set -a
  source "$APP_ENV"
  set +a
fi

# #3611 UNTANGLE — chorus-api's ~14 Fuseki writers (fusekiWriteAuthFromEnv, #3566)
# read the write credential from the shared-infra home beside the store, NOT from
# gathering's .env. The app .env sourcing above remains ONLY for the non-Fuseki
# residual (Twilio, Cost aggregation) — declared in the #3611 boundary map. The
# shared-infra file wins over any same-named keys the .env happened to carry.
FUSEKI_CRED_ENV="${FUSEKI_WRITE_ENV:-$HOME/.gathering/data/fuseki-write.env}"
if [ -r "$FUSEKI_CRED_ENV" ]; then
  set -a
  source "$FUSEKI_CRED_ENV"
  set +a
fi

# #3619 step 0 — source the realm secret so chorus-api can VERIFY service tokens
# at the security envelope's allow path (CHORUS_SERVICE_TOKEN_SECRET; same env
# file owl-api-launch.sh sources). Reference-only: the gitignored file holds the
# value, never this script. Without it the envelope 401s every token (refuse
# path works; allow path can't verify). Optional so non-realm boxes still boot.
REALM_ENV="$HOME/.chorus/secrets/chorus-realm.env"
if [ -f "$REALM_ENV" ]; then
  set -a
  source "$REALM_ENV"
  set +a
fi

cd "$API_DIR"

exec /Users/jeffbridwell/.nvm/versions/node/v20.11.1/bin/node dist/server.js
