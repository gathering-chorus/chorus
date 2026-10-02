#!/usr/bin/env bash
# @domain: tests
# after-land-tests.sh (#4419) — the launchd program behind com.chorus.after-land-tests.
# Drains ~/.chorus/after-land/queue oldest first. For each landed card it runs
# werk-test against the LANDED tree with the land's own diff (WERK_TEST_TREE +
# WERK_TEST_REPLAY): every registered test in the domains the land touched.
# A red nudges the card's owner with the suites that failed; green says so in
# the log. Entries move to done/ either way, with the run's exit code.
set -u
ROOT="${CHORUS_ROOT:-$HOME/CascadeProjects/chorus}"
QUEUE="${AFTER_LAND_QUEUE:-$HOME/.chorus/after-land/queue}"
DONE="$(dirname "$QUEUE")/done"
WERK_TEST="${AFTER_LAND_WERK_TEST:-$HOME/.chorus/bin/werk-test}"
NUDGE="${AFTER_LAND_NUDGE:-$ROOT/platform/scripts/ops-nudge}"
mkdir -p "$QUEUE" "$DONE"
for entry in $(ls "$QUEUE"/*.env 2>/dev/null | sort); do
  CARD=""; ROLE=""; COMMIT=""
  # shellcheck disable=SC1090
  . "$entry"
  echo "after-land: $(date '+%Y-%m-%dT%H:%M:%S%z') #$CARD ($ROLE) at ${COMMIT:0:9} — start"
  out="$(mktemp)"
  WERK_TEST_AFTER_LAND=1 WERK_TEST_TREE="$ROOT" WERK_TEST_REPLAY="$COMMIT" CHORUS_ROOT="$ROOT" \
    "$WERK_TEST" "$CARD" "$ROLE" >"$out" 2>&1
  rc=$?
  grep -E '^domain-select:|^-- werk-test| FAIL$|^!! ' "$out" | head -40
  if [ "$rc" -eq 0 ]; then
    echo "after-land: #$CARD green — every test in the touched domains passed"
  else
    reds="$(grep -E ' FAIL$' "$out" | sed -E 's/^[[:space:]|]*//' | head -5 | tr '\n' ';')"
    echo "after-land: #$CARD RED rc=$rc — $reds"
    [ -x "$NUDGE" ] && "$NUDGE" "$ROLE" "after-land #$CARD: red in the domains it touched (rc=$rc): ${reds:-see ~/Library/Logs/Chorus/after-land-tests.log}. Reopen the card and fix it there." system >/dev/null 2>&1 || true
  fi
  mv "$entry" "$DONE/$(basename "$entry" .env)-rc$rc.env"
  cp "$out" "$DONE/$(basename "$entry" .env)-rc$rc.log"; rm -f "$out"
done
