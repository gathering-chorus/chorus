#!/usr/bin/env bash
# crawl-detached.sh (#4192) — start the crawler's on-land delta OFF the land's
# critical path.
#
# Jeff, 2026-09-16 18:11: "i did not want an extra 10 minutes on every werk."
# The land step used to run `chorus-crawl` inline; a pass that had to touch the
# whole registry held #4186 in WIP for 11 minutes after its code was live. This
# starts the crawler in its own process group with its own log and returns at
# once. The land reports landed; the crawl reports in its log and on the
# nightly's TOTAL line (#4180 crawl_line reads the same log shape).
#
# Env: CARD_ID, ROLE (names the log); CHORUS_ROLE defaults to crawler (the door
# stamps ownedBy from the caller — #4178). CHORUS_CRAWL_BIN overrides the binary
# (tests hand a stub). CHORUS_CRAWL_LOG_DIR overrides ~/.chorus/werk-runs.
set -u
BIN="${CHORUS_CRAWL_BIN:-$(command -v chorus-crawl || true)}"
if [ -z "$BIN" ] || [ ! -x "$BIN" ]; then
  echo "crawl-detached: chorus-crawl not installed — skipped (the nightly full pass repairs the graph)"
  exit 0
fi
LOG_DIR="${CHORUS_CRAWL_LOG_DIR:-$HOME/.chorus/werk-runs}"
mkdir -p "$LOG_DIR"
LOG="$LOG_DIR/${CARD_ID:-nocard}-${ROLE:-norole}-crawl-$(date +%s).log"
export CHORUS_ROLE="${CHORUS_ROLE:-crawler}"
# a subshell + nohup + & puts the crawler outside this step's job control so the
# step (and the act runner behind it) ending does not end the crawl
( nohup "$BIN" >"$LOG" 2>&1 & echo $! >"$LOG.pid" )
echo "crawl-detached: started chorus-crawl (pid $(cat "$LOG.pid")) → $LOG"
exit 0
