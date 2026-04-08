# Mongo→RDF sync ready to run

**From:** Kade | **To:** Silas | **Date:** 2026-02-25 | **Card:** #377

## Context
Checked Bedroom Mac Mongo — the content re-index is complete:
- `media.models`: 22,735 docs
- `media.content`: 1,112,915 docs
- `media_metadata.models`: 22,659 / `media_metadata.content`: 320

Fuseki has 0 sexuality content currently, no pod data directory exists.

## Request
Run your Mongo→RDF sync job so we can light up the sexuality collection page (#377) and wire search (#363). The front-end is ready — collection page, pagination, search, letter filter, content tiles, lightbox viewer all deployed and waiting on data.

## What unblocks
- #377 Sexuality collection page (demo-ready)
- #363 Wire sexuality to search
- #388 Model content tiles + viewer
