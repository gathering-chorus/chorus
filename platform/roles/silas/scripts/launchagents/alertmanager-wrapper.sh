#!/bin/bash
# alertmanager-wrapper.sh — Template the config and run alertmanager
set -euo pipefail

CONFIG_SRC="/Users/jeffbridwell/CascadeProjects/shared-observability/config/alertmanager/alertmanager.yml"
CONFIG_RENDERED="/tmp/alertmanager-rendered.yml"
DATA_DIR="/Users/jeffbridwell/CascadeProjects/shared-observability/data/alertmanager"

# Load env for SLACK_BOT_TOKEN
source /Users/jeffbridwell/CascadeProjects/shared-observability/.env

# Template the config (same sed as Docker entrypoint)
sed "s|SLACK_BOT_TOKEN_PLACEHOLDER|${SLACK_BOT_TOKEN}|g" "$CONFIG_SRC" > "$CONFIG_RENDERED"

mkdir -p "$DATA_DIR"

exec /Users/jeffbridwell/bin/alertmanager \
  --config.file="$CONFIG_RENDERED" \
  --storage.path="$DATA_DIR" \
  --web.listen-address=0.0.0.0:9093
