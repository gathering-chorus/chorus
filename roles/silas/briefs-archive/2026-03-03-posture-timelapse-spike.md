# Brief: Posture Timelapse Spike (#899)

**From:** Wren
**To:** Silas
**Date:** 2026-03-03
**Card:** #899
**Priority:** P2

## Request

Build a webcam capture + LLaVA scoring pipeline for Jeff's daily posture observation.

## Pipeline

1. **Capture** — LaunchAgent on Library Mac, every 5 minutes. `imagesnap` or `ffmpeg` to grab a webcam frame. Store to `/tmp/posture-timelapse/YYYY-MM-DD/HH-MM.jpg`.
2. **Score** — Send each frame to Bedroom Mac Ollama (`http://192.168.86.242:11434`) using LLaVA. Prompt: structured posture assessment (upright/hunched/leaning, shoulder tension, inferred breath depth from chest/shoulder position).
3. **Log** — Append structured JSON to `/tmp/posture-timelapse/YYYY-MM-DD/scores.jsonl`: `{timestamp, posture, tension, breath, notes}`.
4. **Summary** — End-of-day script: posture trend, worst windows, total hunched time.

## Constraints

- All data local. Never leaves the Macs.
- LaunchAgent domain — your call on plist design.
- Check if `imagesnap` is installed, or use `ffmpeg` with avfoundation.
- LLaVA model: check what's available on Bedroom Ollama. Pull if needed.
- Privacy: frames can be ephemeral (delete after scoring) or kept for timelapse playback. Jeff's call — default to keeping for spike.

## Spike Exit

One full day of data. Jeff reviews timelapse + scores. Continue or kill.

## AC

1. Capture runs unattended via LaunchAgent
2. Each frame scored with structured JSON
3. End-of-day summary generated
4. All data stays local
