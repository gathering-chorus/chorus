#!/usr/bin/env bash
# @domain: tests
# after-land-tests.sh (#4419) — the launchd program behind com.chorus.after-land-tests.
# Drains ~/.chorus/after-land/queue oldest first, until it is empty (an entry
# queued while a run is going is picked up by this same run). For each landed
# card it checks out the LANDED commit detached (canonical keeps moving under
# later lands) and runs werk-test there with the land's own diff: every
# registered test in the domains the land touched. A red nudges the card's
# owner with the suites that failed. It waits while the 03:00 nightly holds its
# lock and never takes that lock, so the nightly is never refused.
# Silas's review, 2026-10-02: loop until empty; a detached checkout; the nightly.
set -u
ROOT="${CHORUS_ROOT:-$HOME/CascadeProjects/chorus}"
QUEUE="${AFTER_LAND_QUEUE:-$HOME/.chorus/after-land/queue}"
BASE="$(dirname "$QUEUE")"
DONE="$BASE/done"
TREES="$BASE/trees"
WERK_TEST="${AFTER_LAND_WERK_TEST:-$HOME/.chorus/bin/werk-test}"
NUDGE="${AFTER_LAND_NUDGE:-$ROOT/platform/scripts/ops-nudge}"
TMP_="${TMPDIR:-/tmp}"
LOCK="${NIGHTLY_LOCKDIR:-${TMP_%/}/chorus-nightly-suites.lock.d}"
WAIT_TICK="${AFTER_LAND_WAIT_TICK:-60}"
mkdir -p "$QUEUE" "$DONE" "$TREES"

nightly_running() {
  [ -d "$LOCK" ] || return 1
  local p; p="$(cat "$LOCK/pid" 2>/dev/null)"
  [ -n "$p" ] && kill -0 "$p" 2>/dev/null
}

while :; do
  entry="$(ls "$QUEUE"/*.env 2>/dev/null | sort | head -1)"
  [ -n "$entry" ] || break
  while nightly_running; do
    echo "after-land: the nightly holds its lock — waiting ${WAIT_TICK}s"
    sleep "$WAIT_TICK"
  done
  CARD=""; ROLE=""; COMMIT=""
  # shellcheck disable=SC1090
  . "$entry"
  name="$(basename "$entry" .env)"
  echo "after-land: $(date '+%Y-%m-%dT%H:%M:%S%z') #$CARD ($ROLE) at ${COMMIT:0:9} — start"
  tree="$TREES/$name"
  rm -rf "$tree"
  if ! git -C "$ROOT" worktree add --detach "$tree" "$COMMIT" >/dev/null 2>&1; then
    echo "after-land: #$CARD — could not check out ${COMMIT:0:9}; UNMEASURED, nothing ran"
    mv "$entry" "$DONE/$name-unmeasured.env"
    continue
  fi
  out="$(mktemp)"
  WERK_TEST_AFTER_LAND=1 WERK_TEST_TREE="$tree" WERK_TEST_REPLAY="$COMMIT" CHORUS_ROOT="$ROOT" \
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
  git -C "$ROOT" worktree remove --force "$tree" >/dev/null 2>&1 || rm -rf "$tree"
  mv "$entry" "$DONE/$name-rc$rc.env"
  cp "$out" "$DONE/$name-rc$rc.log"; rm -f "$out"
done
