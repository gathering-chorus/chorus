#!/bin/bash
# chorus-api-wrapper.sh — runs the Chorus API server as a LaunchAgent.
# Sources secrets from jeff-bridwell-personal-site/.env (same creds the app uses)
# so Cost aggregation and Twilio-backed endpoints work without duplicating .env.

set -euo pipefail

API_DIR="/Users/jeffbridwell/CascadeProjects/chorus/platform/api"
APP_ENV="/Users/jeffbridwell/CascadeProjects/jeff-bridwell-personal-site/.env"

if [ -f "$APP_ENV" ]; then
  set -a
  source "$APP_ENV"
  set +a
fi

cd "$API_DIR"

exec /Users/jeffbridwell/.nvm/versions/node/v20.11.1/bin/node dist/server.js
