#!/usr/bin/env bash
# @domain: principles
# principles-api launch wrapper (#5017), the pulse-api wrapper's pattern (#4490). The
# LaunchAgent com.chorus.principles-api runs this from the repo. It hands principles-api
# the store credential by NAME, as its settings rows say (store.user_var /
# store.password_var): the values come from the shared-infra file beside the store,
# extracted key by key, never sourced whole and never printed. Every other setting comes
# from the model (env PRINCIPLES_<KEY> overrides, then principles.toml, then the row).
set -uo pipefail
_credenv="${FUSEKI_WRITE_ENV:-$HOME/.gathering/data/fuseki-write.env}"
if [ -r "$_credenv" ]; then
  export FUSEKI_ADMIN_PASSWORD="$(grep -E '^FUSEKI_ADMIN_PASSWORD=' "$_credenv" | head -1 | cut -d= -f2-)"
  export FUSEKI_ADMIN_USER="$(grep -E '^FUSEKI_ADMIN_USER=' "$_credenv" | head -1 | cut -d= -f2-)"
  [ -z "$FUSEKI_ADMIN_USER" ] && export FUSEKI_ADMIN_USER=admin
fi
# reads need no credential; every write does, so say so at start rather than at the first 503
[ -n "${FUSEKI_ADMIN_PASSWORD:-}" ] || echo "principles-api-launch: no FUSEKI_ADMIN_PASSWORD from $_credenv — reads work, writes will be refused by the store" >&2
# spine events through the team's logger on this box
export PRINCIPLES_EVENTS_COMMAND="${PRINCIPLES_EVENTS_COMMAND:-$HOME/CascadeProjects/chorus/platform/scripts/chorus-log}"
exec "${PRINCIPLES_API_BIN:-$HOME/.chorus/bin/principles-api}" "$@"
