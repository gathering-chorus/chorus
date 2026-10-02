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
WERK_TEST="${AFTER_LAND_WERK_TEST:-$HOME/.chorus/bin/werk-test}"
NUDGE="${AFTER_LAND_NUDGE:-$ROOT/platform/scripts/ops-nudge}"
# The nightly's lock lives in its $TMPDIR, which launchd sets for every user
# agent to the per-user temp dir (getconf DARWIN_USER_TEMP_DIR; measured on a
# running com.chorus agent 2026-10-02). Resolve it the same way here, so the
# two agents agree whatever this one's own environment says.
TMP_="$(getconf DARWIN_USER_TEMP_DIR 2>/dev/null || true)"
TMP_="${TMP_:-${TMPDIR:-/tmp}}"
LOCK="${NIGHTLY_LOCKDIR:-${TMP_%/}/chorus-nightly-suites.lock.d}"
WAIT_TICK="${AFTER_LAND_WAIT_TICK:-60}"
mkdir -p "$QUEUE" "$DONE"

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
  # ONE persistent checkout, moved to each landed commit. A fresh checkout per
  # run had no installed deps, no built binaries and a cold jest cache, and
  # scored those as reds (10-02: eslint, athena-deploy not built, hook
  # timeouts). This tree keeps its node_modules, target/ and caches between
  # runs, the way the nightly's canonical tree does. It sits one level down
  # (tree/chorus) so a suite writing beside the repo writes into tree/, which
  # is cleared of everything but the checkout each run.
  box="$BASE/tree"
  tree="$box/chorus"
  mkdir -p "$box"
  find "$box" -mindepth 1 -maxdepth 1 ! -name chorus -exec rm -rf {} + 2>/dev/null
  git -C "$ROOT" worktree prune >/dev/null 2>&1 || true
  if [ ! -e "$tree/.git" ]; then
    rm -rf "$tree"
    git -C "$ROOT" worktree add --detach "$tree" "$COMMIT" >/dev/null 2>&1
  fi
  if ! git -C "$tree" checkout --detach --force -q "$COMMIT" >/dev/null 2>&1; then
    echo "after-land: #$CARD — could not check out ${COMMIT:0:9}; UNMEASURED, nothing ran"
    mv "$entry" "$DONE/$name-unmeasured.env"
    continue
  fi
  # tracked files exactly at the commit; untracked leftovers go, except the
  # installed deps and build output that make this tree warm
  git -C "$tree" clean -fdq -e node_modules -e target -e .jest-cache >/dev/null 2>&1 || true
  # first use: link canonical's installed deps where this tree has none yet
  ( cd "$ROOT" && find . -maxdepth 4 -name node_modules -type d -not -path '*/node_modules/*' -not -path './.werk*' 2>/dev/null ) |
  while read -r nm; do
    d="$(dirname "$nm")"
    [ -f "$tree/$d/package.json" ] || continue
    [ -e "$tree/$nm" ] && continue
    ln -s "$ROOT/$nm" "$tree/$nm"
  done
  out="$(mktemp)"
  WERK_TEST_AFTER_LAND=1 WERK_TEST_TREE="$tree" WERK_TEST_REPLAY="$COMMIT" CHORUS_ROOT="$ROOT" \
    "$WERK_TEST" "$CARD" "$ROLE" >"$out" 2>&1
  rc=$?
  grep -E '^domain-select:|^-- werk-test| FAIL$|^!! |UNMEASURED' "$out" | head -40
  # The exit code alone is not the verdict: a run that touches werk-test is
  # "self-modifying → advisory" and exits 0 with FAILs, and a unit with no
  # node or cargo scores "deps unavailable" (the first live run, 2026-10-02:
  # 22 such units, exit 0, printed green). Green means rc 0 AND no FAIL line
  # AND nothing unmeasured for want of a toolchain.
  nofail=$(grep -cE '… FAIL$' "$out")
  nodeps=$(grep -cE 'deps unavailable|cargo absent|nextest-probe-spawn-failed' "$out")
  if [ "$nodeps" -gt 0 ]; then
    echo "after-land: #$CARD UNMEASURED — $nodeps unit(s) had no toolchain (deps unavailable / cargo absent); nothing is proven"
    [ -x "$NUDGE" ] && "$NUDGE" "$ROLE" "after-land #$CARD: UNMEASURED — $nodeps unit(s) had no toolchain; see ~/Library/Logs/Chorus/after-land-tests.log" system >/dev/null 2>&1 || true
    [ "$rc" -eq 0 ] && rc=2
  elif [ "$rc" -eq 0 ] && [ "$nofail" -eq 0 ]; then
    echo "after-land: #$CARD green — every test in the touched domains passed"
  else
    [ "$rc" -eq 0 ] && rc=1
    reds="$(grep -E '… FAIL$' "$out" | sed -E 's/^[[:space:]|]*//' | head -5 | tr '\n' ';')"
    echo "after-land: #$CARD RED rc=$rc — $reds"
    [ -x "$NUDGE" ] && "$NUDGE" "$ROLE" "after-land #$CARD: red in the domains it touched (rc=$rc): ${reds:-see ~/Library/Logs/Chorus/after-land-tests.log}. Reopen the card and fix it there." system >/dev/null 2>&1 || true
  fi
  mv "$entry" "$DONE/$name-rc$rc.env"
  cp "$out" "$DONE/$name-rc$rc.log"; rm -f "$out"
done
