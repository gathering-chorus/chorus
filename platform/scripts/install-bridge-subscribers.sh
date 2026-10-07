#!/usr/bin/env bash
# install-bridge-subscribers.sh — #4111
#
# The three bridge subscribers have been running BARE since 2026-08-28: real
# node processes, no launchd supervision, nothing to restart them. That is why
# they flapped 17 times in 400 log lines on 2026-09-06 and why nudges sent
# during a disconnect never reached Jeff's phone.
#
# The plists were authored and committed at config/launchagents/ and never
# installed to ~/Library/LaunchAgents/. Landed, not running — the same class the
# team has hit on plists, deploys and config-guards before.
#
# This does the cutover in the only safe order. It is deliberately a script and
# not an automatic step: replacing a running message path is an operation
# someone should watch.
set -uo pipefail

ROLES=(kade wren silas)
SRC="$(cd "$(dirname "$0")/.." && pwd)/../config/launchagents"
SRC="$(cd "$(dirname "$0")/../../config/launchagents" && pwd)"
DEST="$HOME/Library/LaunchAgents"
UID_N="$(id -u)"

echo "bridge-subscriber install — source $SRC"
for role in "${ROLES[@]}"; do
  label="com.chorus.bridge-subscriber-$role"
  plist="$SRC/$label.plist"
  if [ ! -f "$plist" ]; then
    echo "  REFUSED: $plist missing — nothing to install for $role" >&2
    exit 1
  fi
  # 1. Install the definition.
  cp "$plist" "$DEST/$label.plist"
  echo "  installed  $DEST/$label.plist"

  # 2. Retire any managed copy first, so bootstrap cannot double-start.
  launchctl bootout "gui/$UID_N/$label" >/dev/null 2>&1 || true

  # 3. Stop the UNMANAGED copy. This is the one piece that needs a human: the
  #    bare processes are launchd orphans, so no lifecycle verb reaches them and
  #    the infra guard (rightly) blocks an agent from killing a process. Left
  #    running they would double-deliver every message.
  bare=$(pgrep -f "bridge-subscriber.js $role" 2>/dev/null | tr '\n' ' ')
  if [ -n "$bare" ]; then
    echo "  ACTION NEEDED: bare $role subscriber still running (pid $bare)."
    echo "                 Stop it, then re-run: kill $bare"
    echo "                 Not bootstrapping $label — two subscribers on one"
    echo "                 bridge would deliver every message twice."
    continue
  fi

  # 4. Only with the bare copy gone is it safe to hand the role to launchd.
  launchctl bootstrap "gui/$UID_N" "$DEST/$label.plist" 2>/dev/null \
    && echo "  bootstrapped $label" \
    || echo "  bootstrap failed for $label (already loaded?)"
done

echo
echo "Verify:  bash platform/tests/bridge-subscriber-health.test.sh"
