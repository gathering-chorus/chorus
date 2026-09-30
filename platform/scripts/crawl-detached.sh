#!/usr/bin/env bash
# crawl-detached.sh (#4192, #4199) — start the on-land crawl OFF the land's
# critical path, owned by launchd.
#
# Jeff, 2026-09-16 18:11: "i did not want an extra 10 minutes on every werk."
# #4192 started the crawler with nohup from the step; under act the step's end
# killed it (Wren's #4195 land left an EMPTY crawl log, 2026-09-17 07:56). A
# process the job owns dies with the job. launchd owns com.chorus.crawl-nightly,
# so the land KICKSTARTS that unit: one process owner, one log (the nightly's),
# one set of alerts reading it (crawler-stale, crawler-error).
#
# #4185 — a pass already running is LEFT TO FINISH. This used `kickstart -k`,
# which killed a running pass on every land: on 2026-09-30 a full pass ran
# 14:31 → 15:27, was SIGTERMed by a land, wrote no summary line, and started
# over from the top. A long pass never finished while lands kept coming. A land
# while a pass runs is said here and in the crawl log; the next pass (the next
# land, or the nightly) walks the newer tree.
#
# Seams (tests hand stubs): CRAWL_KICKSTART_CMD replaces the launchctl start,
# CRAWL_STATE_CMD the launchctl print that says whether a pass is running,
# CRAWL_LOG the crawl log.
set -u
UNIT="com.chorus.crawl-nightly"
LOG="${CRAWL_LOG:-$HOME/Library/Logs/Chorus/crawl-nightly.log}"
STATE_CMD="${CRAWL_STATE_CMD:-launchctl print gui/$(id -u)/$UNIT}"
CMD="${CRAWL_KICKSTART_CMD:-launchctl kickstart gui/$(id -u)/$UNIT}"
state=$($STATE_CMD 2>/dev/null || true)
if grep -qE '^[[:space:]]*state = running' <<<"$state"; then
  pid=$(sed -nE 's/^[[:space:]]*pid = ([0-9]+).*/\1/p' <<<"$state" | head -1)
  msg="a pass is already running (pid ${pid:-?}) — left to finish, not restarted (#4185)"
  echo "crawl-detached: $msg"
  echo "chorus-crawl: land at $(date '+%Y-%m-%dT%H:%M:%S%z'): $msg" >> "$LOG" 2>/dev/null || true
  exit 0
fi
if out=$($CMD 2>&1); then
  echo "crawl-detached: kickstarted $UNIT (launchd owns the pass; log ~/Library/Logs/Chorus/crawl-nightly.log) ${out}"
else
  echo "crawl-detached: kickstart of $UNIT failed rc=$? — ${out}. The nightly full pass repairs the graph; the reconcile names what it missed."
fi
exit 0
