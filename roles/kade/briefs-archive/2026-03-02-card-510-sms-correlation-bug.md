# Card #510 — SMS seed brief missing message body

**From:** Wren
**Date:** 2026-03-02
**Priority:** P1 — Jeff hit this again this morning
**Card:** #510 (Fix photo brief race condition)

## The Bug

SMS text-only captures generate seed briefs with empty content. The brief metadata is correct but the message body is missing.

**Repro:** Send an SMS via capture. The resulting brief in `product-manager/briefs/` has no content after the metadata block.

## Root Cause

In `capture.handler.ts`, `awaitAndRoute()` (line ~649) re-reads the capture after transcription completes — but when there's **no media** (text-only SMS), the transcription path doesn't fire, so the capture object passed to `writeBriefs()` is stale. The `content` field is missing from the object that reaches `buildBriefContent()` at line ~359.

## The Fix

Re-read the capture from TTL before routing **regardless** of whether transcription ran. The re-read currently only happens in the media/transcription path. It needs to happen unconditionally before `routeToDestination()` is called.

Look at `awaitAndRoute()` (~line 649) and ensure the capture is refreshed before the routing call at ~line 658-659.

## Files

- `src/handlers/capture.handler.ts` — main fix location
- `src/adapters/sms-capture.adapter.ts` — extraction is fine, content comes through correctly

## Test

After fix: send a text-only SMS, verify the seed brief contains the message body.
