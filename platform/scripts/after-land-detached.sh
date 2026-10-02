#!/usr/bin/env bash
# @domain: tests
# after-land-detached.sh (#4419) — queue the landed card's domain tests and
# start the after-land runner OFF the land's critical path, owned by launchd.
#
# Jeff, 2026-10-02, picking the shape: keep the werk fast (~7 files), and right
# after each land run every test in the domains the land touched, in the
# background; a red reaches the card's owner in about 30 minutes, not at 03:00.
# Replayed: #4353's land would have run discover-pages / domain-page /
# chorus-domain-pipeline, #4417's tunnel-auth.integration — the reds the
# nightly found hours later.
#
# One queue entry per land (card, role, landed commit). The unit drains the
# queue oldest first, so a second land while a run is going waits its turn.
# Seams: AFTER_LAND_QUEUE, AFTER_LAND_KICKSTART_CMD, AFTER_LAND_STATE_CMD.
set -u
UNIT="com.chorus.after-land-tests"
QUEUE="${AFTER_LAND_QUEUE:-$HOME/.chorus/after-land/queue}"
STATE_CMD="${AFTER_LAND_STATE_CMD:-launchctl print gui/$(id -u)/$UNIT}"
CMD="${AFTER_LAND_KICKSTART_CMD:-launchctl kickstart gui/$(id -u)/$UNIT}"
CARD="${CARD_ID:?CARD_ID unset}"
ROLE_="${ROLE:?ROLE unset}"
COMMIT="${LANDED_COMMIT:-$(git -C "${CHORUS_HOME:-$HOME/CascadeProjects/chorus}" rev-parse HEAD 2>/dev/null)}"
if [ -z "$COMMIT" ]; then
  echo "after-land: no landed commit to test — nothing queued"
  exit 0
fi
mkdir -p "$QUEUE"
entry="$QUEUE/$(date +%s)-${CARD}.env"
printf 'CARD=%s\nROLE=%s\nCOMMIT=%s\n' "$CARD" "$ROLE_" "$COMMIT" > "$entry"
echo "after-land: queued #$CARD ($ROLE_) at ${COMMIT:0:9} → $entry"
state=$($STATE_CMD 2>/dev/null || true)
if grep -qE '^[[:space:]]*state = running' <<<"$state"; then
  echo "after-land: the runner is already going; this land waits its turn in the queue"
  exit 0
fi
if out=$($CMD 2>&1); then
  echo "after-land: kickstarted $UNIT ${out}"
else
  echo "after-land: kickstart of $UNIT failed — ${out}. The entry stays queued for the next land; the nightly still runs everything."
fi
exit 0
