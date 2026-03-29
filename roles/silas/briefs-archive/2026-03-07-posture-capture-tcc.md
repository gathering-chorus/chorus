# Brief: Posture capture LaunchAgent — camera TCC permission

**From:** Wren
**Card:** #1105 (Jeff operations dashboard)
**Priority:** P2 — not blocking, but data is sparse without it

## Issue

`com.chorus.posture-capture` is loaded and running (exit 0) but only produced 1 automatic frame today (7:57am). Manual runs from terminal work perfectly — MX Brio captures, LLaVA scores, scores.jsonl gets written.

The likely cause: macOS TCC (Transparency, Consent, and Control) camera privacy. When `imagesnap` runs via launchd, it may not have camera access granted. The 7:57am frame may have worked due to a fresh login session grant that expired.

## Evidence

```
# LaunchAgent loaded, exit 0
launchctl list | grep posture → 0  com.chorus.posture-capture

# Only 1 automatic capture all day (8am-6pm window = should be ~120 frames)
ls /tmp/posture-timelapse/2026-03-07/*.jpg → 07-57.jpg only

# Manual run works fine
POSTURE_TEST=1 bash posture-capture.sh → scored: 18-27.jpg (hunched, moderate tension)
```

## What needs fixing

1. Grant camera TCC access to the process that launchd spawns (likely `/usr/local/bin/imagesnap` or `/bin/bash` needs to be in System Settings > Privacy & Security > Camera)
2. Verify with a few automatic cycles that frames are being produced every 5 minutes
3. Consider: should the log (`/tmp/posture-capture.log`) capture failures? Currently empty — the script exits silently on `imagesnap` failure.

## Context

`/jeff` dashboard is being built now and will consume `scores.jsonl`. The more data points per day, the better the posture/mood timeline. Current: 1 frame/day. Expected: ~120 frames/day.
