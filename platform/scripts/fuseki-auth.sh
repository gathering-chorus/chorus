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

# #3630 — when the credential isn't already in-env, read it from its EXISTING
# canonical home: the gathering app .env (the same file chorus-api-wrapper.sh and
# the fuseki service source — no new secret home, no duplication). Targeted
# extract of just the two keys (never source the whole .env, never echo the value).
# Empty (→ anon → current behavior) until reachable, so safe to land ahead of the flip.
if [ -z "${FUSEKI_ADMIN_PASSWORD:-}" ]; then
  _appenv="${GATHERING_APP_ENV:-${CHORUS_ROOT:-$HOME/CascadeProjects/chorus}/../jeff-bridwell-personal-site/.env}"
  if [ -r "$_appenv" ]; then
    FUSEKI_ADMIN_PASSWORD="$(grep -E '^FUSEKI_ADMIN_PASSWORD=' "$_appenv" | head -1 | cut -d= -f2-)"
    : "${FUSEKI_ADMIN_USER:=$(grep -E '^FUSEKI_ADMIN_USER=' "$_appenv" | head -1 | cut -d= -f2-)}"
  fi
fi

FUSEKI_AUTH=()
if [ -n "${FUSEKI_ADMIN_PASSWORD:-}" ]; then
  FUSEKI_AUTH=(-u "${FUSEKI_ADMIN_USER:-admin}:${FUSEKI_ADMIN_PASSWORD}")
  # #3641 — EXPORT so a sourcing shell's chorus-model spawns (a subprocess, which
  # reads FUSEKI_ADMIN_PASSWORD to add its own -u) inherit the credential. Without
  # export it's a shell-local var: bash curl writers in-shell work (FUSEKI_AUTH),
  # but chorus-model from the same shell 401s (Wren's bare-CLI catch on #3641).
  export FUSEKI_ADMIN_PASSWORD
  export FUSEKI_ADMIN_USER="${FUSEKI_ADMIN_USER:-admin}"
fi
