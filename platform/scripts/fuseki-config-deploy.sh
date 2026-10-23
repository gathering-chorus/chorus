#!/usr/bin/env bash
# fuseki-config-deploy.sh (#4718) — put the canonical Fuseki server config in
# place. Fuseki reads it only at start, so this never restarts anything itself.
#
#   --check   exit 0 if the runtime file equals the canonical one, 1 if not (diff shown)
#   (none)    back up the runtime file beside itself, then install the canonical one
#
# A restart is a separate, asked-for step (it blips the door for ~5 s):
#   launchctl kickstart -k gui/$(id -u)/com.gathering.fuseki
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CANONICAL="${FUSEKI_SERVER_CANONICAL:-$SCRIPT_DIR/launchagents-canonical/fuseki-config-server.ttl.in}"
RUNTIME="${FUSEKI_SERVER_RUNTIME:-$HOME/.gathering/data/config.ttl}"

[ -r "$CANONICAL" ] || { echo "fuseki-config-deploy: canonical $CANONICAL missing" >&2; exit 2; }

if [ "${1:-}" = "--check" ]; then
  if cmp -s "$CANONICAL" "$RUNTIME"; then
    echo "fuseki-config-deploy: $RUNTIME matches canonical"
    exit 0
  fi
  echo "fuseki-config-deploy: $RUNTIME differs from canonical" >&2
  diff -u "$RUNTIME" "$CANONICAL" >&2 || true
  exit 1
fi

if [ -e "$RUNTIME" ] && ! cmp -s "$CANONICAL" "$RUNTIME"; then
  cp -p "$RUNTIME" "$RUNTIME.pre-4718.$(date +%Y%m%dT%H%M%S)"
fi
install -m 0644 "$CANONICAL" "$RUNTIME"
echo "fuseki-config-deploy: installed $RUNTIME (takes effect on the next Fuseki start)"
