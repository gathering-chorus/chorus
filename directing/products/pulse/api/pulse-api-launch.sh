#!/usr/bin/env bash
# @domain: awareness
# pulse-api launch wrapper (#4490). The LaunchAgent (com.chorus.pulse-api, Silas) runs
# this. It hands pulse-api the store credential by NAME, the way its Property rows say
# (store.user_var / store.password_var): the values come from the shared-infra file
# beside the store, extracted key by key, never sourced whole and never printed.
# Settings otherwise come from the model (env PULSE_<KEY> overrides, then pulse.toml).
set -uo pipefail
_credenv="${FUSEKI_WRITE_ENV:-$HOME/.gathering/data/fuseki-write.env}"
if [ -r "$_credenv" ]; then
  export FUSEKI_ADMIN_PASSWORD="$(grep -E '^FUSEKI_ADMIN_PASSWORD=' "$_credenv" | head -1 | cut -d= -f2-)"
  export FUSEKI_ADMIN_USER="$(grep -E '^FUSEKI_ADMIN_USER=' "$_credenv" | head -1 | cut -d= -f2-)"
  [ -z "$FUSEKI_ADMIN_USER" ] && export FUSEKI_ADMIN_USER=admin
fi
# spine events through the team's logger on this box
export PULSE_EVENTS_COMMAND="${PULSE_EVENTS_COMMAND:-$HOME/CascadeProjects/chorus/platform/scripts/chorus-log}"
# #4716 — pulse writes through the door as principal-pulse (its own identity,
# seeded 2026-10-24), never with the store password; reads stay plain SPARQL.
export PULSE_STORE_KIND="${PULSE_STORE_KIND:-door}"
exec "$HOME/.chorus/bin/pulse-api" "$@"
