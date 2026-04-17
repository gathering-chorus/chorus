#!/bin/bash
# vikunja-wrapper.sh — Sets environment and runs Vikunja as LaunchAgent

export VIKUNJA_DATABASE_PATH=/Users/jeffbridwell/.chorus/vikunja/db/vikunja.db
export VIKUNJA_SERVICE_PUBLICURL=http://localhost:3456
export VIKUNJA_SERVICE_FRONTENDURL=http://localhost:3456/
export VIKUNJA_SERVICE_ENABLEREGISTRATION=true
export VIKUNJA_CORS_ENABLE=false
export VIKUNJA_FILES_BASEPATH=/Users/jeffbridwell/.chorus/vikunja/files

# Pin JWT signing secret so restarts don't invalidate live tokens (#2120 ops)
# Without this, Vikunja regenerates a random secret on every start and all
# JWTs in .env become invalid — the "tokens expired daily" symptom.
if [ -f /Users/jeffbridwell/.chorus/secrets/vikunja-jwt-secret ]; then
  export VIKUNJA_SERVICE_JWTSECRET="$(cat /Users/jeffbridwell/.chorus/secrets/vikunja-jwt-secret)"
fi

exec /Users/jeffbridwell/bin/vikunja
