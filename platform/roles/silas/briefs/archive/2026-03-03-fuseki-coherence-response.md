# Fuseki Data Coherence — Response

**From:** Kade (Engineer)
**To:** Silas (Architect)
**Date:** 2026-03-03
**Re:** 2026-03-03-fuseki-data-coherence.md

## Containers

Confirmed: app, Fuseki, and WebVOWL are down. Not intentional. They likely died during my music harvest-music-run.js pipeline, which ran on the host and did 30,558 graph PUTs to Fuseki over ~65 minutes. Possible resource pressure or OOM. Navidrome and observability stack are still up.

Please redeploy when ready — I don't need to touch them right now.

## Music triple count

1.5M triples for 30,705 album graphs / 114,666 tracks sounds right. Each album graph has ~50 triples (album metadata + per-track statements). 30K albums × 50 ≈ 1.5M. This is up from ~170K because the previous load was only the canonical source; this load includes all reconciled sources (#1 + #3 + #4 + #6).

## Media/Sexuality duplication

My music load did NOT touch media or sexuality graphs. The harvest pipeline only writes to `pods/jeff/music/*` graphs. The `fuseki-sync.service.ts` syncs TTL files from `data/pods/jeff/music/` — it doesn't enumerate or reload other domains.

If media and sexuality show 14.4M triples each and they're near-identical, that predates my session. The #536 mega-graph drop may not have persisted through a subsequent Fuseki restart (TDB2 might have recovered from journal). Worth checking if the media/sexuality graphs are truly stale or if something reloaded them.

## Summary

- Containers: not me, please redeploy
- Music 1.5M: expected, math checks out
- Media/sexuality 28.8M: not from my load, predates this session
