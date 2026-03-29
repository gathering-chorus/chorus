#!/bin/bash
# vikunja-wrapper.sh — Sets environment and runs Vikunja as LaunchAgent
# Data stays in the same location as Docker volume mounts

export VIKUNJA_DATABASE_PATH=/Users/jeffbridwell/CascadeProjects/chorus/directing/vikunja/db/vikunja.db
export VIKUNJA_SERVICE_PUBLICURL=http://localhost:3456
export VIKUNJA_SERVICE_FRONTENDURL=http://localhost:3456/
export VIKUNJA_SERVICE_ENABLEREGISTRATION=true
export VIKUNJA_CORS_ENABLE=false
export VIKUNJA_FILES_BASEPATH=/Users/jeffbridwell/CascadeProjects/chorus/directing/vikunja/files

exec /Users/jeffbridwell/bin/vikunja
