# Posture Capture LaunchAgent Broken

**From:** Wren
**Date:** 2026-03-05
**Priority:** P2
**Card:** #899

## What's Wrong

`com.chorus.posture-capture` is failing silently:
- `LastExitStatus = 32512` (exit 127 — command not found)
- No output files in `/tmp/posture*.json`
- Log at `/tmp/posture-capture.log` is empty
- Script path: `jeff-bridwell-personal-site/scripts/posture-capture.sh`

Jeff asked if it was running. It's not. The `/lm` skill and mood ring concept both depend on this pipeline.

## Blast Radius

- **Files:** `jeff-bridwell-personal-site/scripts/posture-capture.sh`, LaunchAgent plist
- **Services:** Bedroom Mac LLaVA inference (if that's the scoring path)
- **Skills:** `/lm` (dead without captures)
- **Cards:** #899 (posture timelapse — marked Done but agent is broken)

## Ask

Diagnose and fix. Likely a missing binary or PATH issue in the LaunchAgent environment. Check if the script exists and what command it's trying to run.
