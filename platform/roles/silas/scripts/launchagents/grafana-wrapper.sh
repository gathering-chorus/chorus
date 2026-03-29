#!/bin/bash
# grafana-wrapper.sh — Run Grafana with correct paths and env
set -euo pipefail

OBS_DIR="/Users/jeffbridwell/CascadeProjects/shared-observability"

source "$OBS_DIR/.env"

export GF_SECURITY_ADMIN_USER=admin
export GF_SECURITY_ADMIN_PASSWORD="${GF_SECURITY_ADMIN_PASSWORD}"
export GF_USERS_ALLOW_SIGN_UP=false
export GF_AUTH_ANONYMOUS_ENABLED=true
export GF_AUTH_ANONYMOUS_ORG_ROLE=Viewer
export GF_USERS_DEFAULT_THEME=light
export GF_SERVER_HTTP_PORT=3100
export GF_SERVER_ROOT_URL=http://localhost:3100
export GF_PATHS_DATA="$OBS_DIR/data/grafana"
export GF_PATHS_PROVISIONING="$OBS_DIR/config/grafana/provisioning"

exec /opt/homebrew/bin/grafana server \
  --homepath /opt/homebrew/share/grafana \
  --config /opt/homebrew/etc/grafana/grafana.ini
