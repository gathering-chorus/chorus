# Brief: SWAT #410 — Whisper transcription blocks Express event loop

**From**: Silas (Architect)
**To**: Kade (Engineer)
**Date**: 2026-02-26
**Priority**: SWAT — site went non-responsive for Jeff

## Problem

A single 5-minute voice SMS froze localhost:3000 for ~121 seconds. Health check took 21s. Container went unhealthy.

**Root cause**: `spawnSync('/usr/local/bin/whisper-cli', ...)` in the capture pipeline. Synchronous child process blocks the Node event loop entirely while whisper runs. Nothing else gets served.

## Log evidence

```
{"level":"warn","message":"Whisper transcription failed","wavFilePath":"/app/data/pods/jeff/capture/media/sms-1772111444963-fdw0xg-0.wav","error":"spawnSync /usr/local/bin/whisper-cli ETIMEDOUT"}
```

Request completed with `durationMs: 121072`.

## Fix

1. **Replace `spawnSync` with `spawn`** (async) — the event loop must never block on transcription.
2. **Hard timeout** (30s suggested) — if whisper can't finish, save the capture without transcription. Don't block the response.
3. **Consider**: queue long transcriptions for background processing, update the capture record when complete.

## Acceptance Criteria

- [ ] Voice SMS capture does not block other requests
- [ ] Health check stays responsive during transcription
- [ ] Capture is saved even if transcription times out
- [ ] Container stays healthy throughout
