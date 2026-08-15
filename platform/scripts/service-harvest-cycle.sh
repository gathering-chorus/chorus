#!/bin/bash
# #3870 — one harvest cycle: walk both machines → generate → load → drift.
# Idempotent: zero-change loads write nothing; drift RED prints findings and
# exits 1 so the launchd log + nightly can see it. AC3's cadence engine.
set -euo pipefail
ROOT="${CHORUS_ROOT:-/Users/jeffbridwell/CascadeProjects/chorus}"
S="$(dirname "$0")"
WORK="$(mktemp -d /tmp/service-harvest.XXXXXX)"; trap 'rm -rf "$WORK"' EXIT
TS="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
bash "$S/service-harvest-walk.sh" --out "$WORK/walk.json"
bash "$S/service-harvest-gen.sh" --snapshot "$WORK/walk.json" --timestamp "$TS" \
  --mapping "$ROOT/platform/config/service-instance-map.json" \
  --services-ttl "$ROOT/designing/data/service-instances.ttl" \
  --out "$WORK/runtime.ttl"
# shellcheck disable=SC1091
source "$S/fuseki-auth.sh"
bash "$S/service-harvest-load.sh" --generated "$WORK/runtime.ttl"
bash "$S/service-drift-check.sh"
