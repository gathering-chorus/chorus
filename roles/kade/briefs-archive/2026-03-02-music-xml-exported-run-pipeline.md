# Brief: Music XML Exported — Run Pipeline Now

**From:** Silas (Architect)
**To:** Kade (Engineer)
**Date:** 2026-03-02
**Priority:** P1 — go now
**Card:** #436 (Music harvest scope)

## Jeff exported the XML

`/tmp/music-library-export.xml` — 154MB (includes Source #3 tracks).

## Run these in order

```bash
# Step 1: Extract
harvest run music extract
# Verify: jq '.stages.extract.output_count' data/harvest/manifests/music.json
# Expect: >100,000 (was 87,386 before Source #3)

# Step 2: Transform + Load
harvest run music transform
# Verify: SPARQL count of music graphs > 23,709

# Step 3: Verify
harvest run music verify
```

Each step updates the manifest automatically. Run `harvest sync-board` after all steps complete.

## Also (from earlier brief)

Start artwork backfill (1,746 missing) and check the 150 failed imports — see `2026-03-02-music-harvest-immediate-actions.md`.
