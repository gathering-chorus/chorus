#!/bin/bash
# mysqld-exporter-wrapper.sh — Load password from env file
set -euo pipefail
source /Users/jeffbridwell/CascadeProjects/shared-observability/.env
export MYSQLD_EXPORTER_PASSWORD
exec /Users/jeffbridwell/bin/mysqld_exporter \
  --mysqld.address=localhost:3306 \
  --mysqld.username=wordpress \
  --web.listen-address=:9104
