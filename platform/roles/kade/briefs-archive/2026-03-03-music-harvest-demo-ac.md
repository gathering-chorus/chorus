# Brief: Music Harvest Demo — Acceptance Criteria

**From:** Wren (PM)
**To:** Kade (Engineer)
**Date:** 2026-03-03
**Card:** #436

## Context

Jeff and I discussed what "done" looks like for the music harvest this morning. He's ready for a demo when the Fuseki load completes. Here's what matters to him.

## Acceptance Criteria

**1. Catalog vs media count (the key number)**
Jeff wants to see two numbers side by side:
- **High water mark**: total distinct tracks in the RDF catalog (metadata, regardless of media)
- **Creek**: tracks that have actual playable media files behind them (`sourceFilePath` populated)

Before today's load: 97,449 catalog / 75,992 with media (78%). Show the new numbers after all sources land.

**2. Source accounting**
Show which sources contributed what. Jeff wants to see the full picture — what came in, what was dedup'd, what was skipped and why. Your `current-work.md` table is close to this already.

**3. Navidrome validation**
Same pattern as Source #3 — search an imported artist, play a track. Jeff liked this as the acceptance test. Do it for a Kirby track and a Legacy iTunes track.

**4. What Jeff does NOT need**
- Technical pipeline details (he trusts the process)
- Error counts unless they're concerning
- Time spent

## Tone

This one matters personally to Jeff. The collection represents decades of his life — loss, recovery, rebuilding. 78% recovery surprised him (he expected 50%). Show him the numbers and let him react. Don't rush past the moment.

## Gemba

Wren is observing live. Jeff is watching. Demo when you're ready.
