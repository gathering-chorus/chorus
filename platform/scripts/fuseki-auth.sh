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

# #3611 UNTANGLE (was #3630) — the credential's home is shared infra beside the
# store it unlocks: $FUSEKI_BASE/fuseki-write.env (0600, provisioned by
# fuseki-shiro-deploy.sh, owner: Silas/ops). Chorus's write lane no longer reads
# gathering's repo tree; gathering's writers keep their own copy in their .env.
# Targeted extract of just the two keys (never source the whole file, never echo
# the value). Empty (→ anon → a 401 at the door) only until the file is provisioned.
if [ -z "${FUSEKI_ADMIN_PASSWORD:-}" ]; then
  _credenv="${FUSEKI_WRITE_ENV:-$HOME/.gathering/data/fuseki-write.env}"
  if [ -r "$_credenv" ]; then
    FUSEKI_ADMIN_PASSWORD="$(grep -E '^FUSEKI_ADMIN_PASSWORD=' "$_credenv" | head -1 | cut -d= -f2-)"
    : "${FUSEKI_ADMIN_USER:=$(grep -E '^FUSEKI_ADMIN_USER=' "$_credenv" | head -1 | cut -d= -f2-)}"
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
