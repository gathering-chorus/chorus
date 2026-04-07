#!/bin/bash
# app-wrapper.sh — Loads .env and runs the Express app as a LaunchAgent
# Replaces Docker container for the personal site app (#1390)

set -euo pipefail

APP_DIR="/Users/jeffbridwell/CascadeProjects/jeff-bridwell-personal-site"

# Load environment from .env (same vars Docker compose injected)
if [ -f "$APP_DIR/.env" ]; then
  set -a
  source "$APP_DIR/.env"
  set +a
fi

cd "$APP_DIR"

# Override Docker-specific paths from .env with host paths
export POD_STORAGE_PATH="$APP_DIR/data/pods"
export POD_BASE_URL="/pods"

# Use nvm's node — matches the version node_modules were built with
export NVM_DIR="$HOME/.nvm"
[ -s "$NVM_DIR/nvm.sh" ] && . "$NVM_DIR/nvm.sh"

# Reconcile seeds that arrived during downtime (#1400)
/Users/jeffbridwell/CascadeProjects/platform/scripts/seed-reconcile.sh 2>/dev/null || true

exec node dist/app.js
