#!/usr/bin/env bash
# crawl-detached.sh (#4192, #4199) — start the on-land crawl OFF the land's
# critical path, owned by launchd.
#
# Jeff, 2026-09-16 18:11: "i did not want an extra 10 minutes on every werk."
# #4192 started the crawler with nohup from the step; under act the step's end
# killed it (Wren's #4195 land left an EMPTY crawl log, 2026-09-17 07:56). A
# process the job owns dies with the job. launchd owns com.chorus.crawl-nightly,
# so the land KICKSTARTS that unit: one process owner, one log (the nightly's),
# one set of alerts reading it (crawler-stale, crawler-error). `-k` restarts a
# pass already running, so two lands in a row still end with a pass that walked
# the latest tree; the killed pass never advanced its watermark.
#
# Seam: CRAWL_KICKSTART_CMD replaces the launchctl call (tests hand a stub).
set -u
UNIT="com.chorus.crawl-nightly"
CMD="${CRAWL_KICKSTART_CMD:-launchctl kickstart -k gui/$(id -u)/$UNIT}"
if out=$($CMD 2>&1); then
  echo "crawl-detached: kickstarted $UNIT (launchd owns the pass; log ~/Library/Logs/Chorus/crawl-nightly.log) ${out}"
else
  echo "crawl-detached: kickstart of $UNIT failed rc=$? — ${out}. The nightly full pass repairs the graph; the reconcile names what it missed."
fi
exit 0
