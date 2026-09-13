#!/usr/bin/env bash
# test-mcp-config-drift.sh (#4149) — the nightly's canonical-tree guard: the
# role .mcp.json files committed on main must be exactly what gen-role-mcp.sh
# emits. A hand-edit into a generated file reads fine in review and is erased
# by the next generator run — that is precisely how every role lost its
# Grafana/Loki tools at 03:26 on 2026-09-13, twelve hours after #4149 landed.
#
# This lives OUTSIDE the bats file on purpose. The bats proves the generator
# and the drift check behave (negative proofs included) and runs in the werk,
# where the fix exists. This one measures CANONICAL, so before a fix lands it
# is honestly red — which is the state it exists to report.
set -uo pipefail
ROOT="${CHORUS_HOME:-/Users/jeffbridwell/CascadeProjects/chorus}"
if bash "$ROOT/platform/scripts/check-mcp-config-drift.sh" "$ROOT"; then
  echo "GREEN"
  exit 0
fi
echo "RED"
exit 1
