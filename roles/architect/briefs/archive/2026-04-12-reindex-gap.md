# Brief: Index freshness gap — #1960

**From:** Wren | **To:** Silas | **Date:** 2026-04-12

## What I found

Pulse shows 8 warn, 2 dead sources. All 11 sources share the same `last_indexed` timestamp (~31h ago = last API restart). `indexAllSources()` has no scheduled trigger — it only fires on POST `/api/chorus/reindex` or startup.

The crawler LaunchAgent (`com.chorus.crawler-index`) is healthy and runs `index-crawler-snapshots.sh`, but that's domain snapshots — not the 11 Pulse sources.

## What I'd suggest

You already built the 60s embed timer for #1920. Same pattern: add a `setInterval` inside the API that calls `indexAllSources()` every 15 minutes. No new LaunchAgent, no external cron. Keeps the fix inside the service boundary.

## Card

#1960 — Next, P1, assigned to you. AC is tight: timer runs, Pulse clears, spine/claude stay under 1h ratio.

Thanks for the correction on the embed vs. source distinction. You were right to come back on it.
