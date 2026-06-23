#!/usr/bin/env bash
# fuseki-auth.sh — #3566 LOCK: the script-side write door.
#
# Source this in any bash script that writes to Fuseki, then pass "${FUSEKI_AUTH[@]}"
# to the curl write call:
#
#   source "$SCRIPTS/fuseki-auth.sh"
#   curl "${FUSEKI_AUTH[@]+"${FUSEKI_AUTH[@]}"}" -X POST ... "$FUSEKI_UPDATE"
#
# Use the "${FUSEKI_AUTH[@]+"${FUSEKI_AUTH[@]}"}" form EXACTLY — macOS /bin/bash is
# 3.2, where a plain "${FUSEKI_AUTH[@]}" on an EMPTY array under `set -u` aborts with
# "unbound variable". The [@]+ guard expands to nothing when empty, the args when set.
#
# One place owns the credential logic (the "one door" principle, same as the TS
# client factories). When FUSEKI_ADMIN_PASSWORD is unset, FUSEKI_AUTH is empty →
# the write is unauthenticated → CURRENT behavior. So sourcing this is safe to land
# before the shared lock is flipped: nothing changes until the credential deploys.
#
# Reads are NOT affected — only writers source this, and only on their write call.

FUSEKI_AUTH=()
if [ -n "${FUSEKI_ADMIN_PASSWORD:-}" ]; then
  FUSEKI_AUTH=(-u "${FUSEKI_ADMIN_USER:-admin}:${FUSEKI_ADMIN_PASSWORD}")
fi
