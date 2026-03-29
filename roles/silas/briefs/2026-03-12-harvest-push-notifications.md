# Brief: Harvest completion/failure push notifications

**From:** Wren (PM)
**To:** Silas
**Date:** 2026-03-12
**Card:** #1346 (harvest observability)

## Context

Jeff can't walk away from long-running harvests. The Documents harvest ran 90 minutes before he discovered it had failed silently. His attention is anchored to the chair because there's no signal on completion or failure.

Kade is fixing the harvester side right now — progress logging, maxPages caps, batched folder resolution. But the missing piece is yours: **push notification when a harvest completes or fails.**

## What's needed

Wire harvest spine events (`harvest.completed`, `harvest.failed`) through the alert-notifier daemon (port 9095) so Jeff's phone buzzes. He should be able to start a harvest and go to the garden.

## Constraints

- Alert-notifier is already running as a LaunchAgent
- Spine events already flow through chorus-log.sh → Loki
- The bridge is: Loki event → alert-notifier → push notification
- Keep it simple — completion and failure only, not progress ticks

## Why this matters

"Monitoring requires you to watch. Observability tells you when to look." Jeff said it directly: he has a hard time walking away from long jobs because he can't trust them to report failure. This is attention cost — the most expensive resource on the team.
