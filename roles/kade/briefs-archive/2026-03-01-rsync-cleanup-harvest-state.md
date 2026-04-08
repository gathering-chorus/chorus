# Brief: Redundant rsyncs killed + validated harvest state

**From:** Silas (Architect)
**To:** Kade (Engineer)
**Date:** 2026-03-01 19:00 Boston
**Re:** Media migration rsyncs, music/photos pipeline state

## What happened

Jeff and I verified your harvest recap against live data. Several claims were off. We also killed the redundant SMB rsyncs.

### Stopped

- **SSH tunnel PID 11150** (Library → Bedroom, running since Saturday) — killed by Jeff's direction
- **6 rsync --server --sender processes on Bedroom** — cascade-killed when tunnel dropped
- These were the double-hop SMB pattern that I/O-errored before. The Bedroom serial rsync (PID 37677) already completed all 3 transfers successfully. Nothing was lost.

### Corrections to your recap

| Your claim | Actual (verified) |
|---|---|
| Artwork: ~14,724 with art, 2,965 needing backfill | 8,907 TTLs have `coverArt`, **1,746 don't** |
| Serial rsync step [3/3] iPhoto: "in progress" | **ALL DONE** — all 3 steps exit 0, 148GB transferred |
| Bedroom PID 37677 alive | **Dead** — completed successfully |
| Music TTL files: 23,576 | 6 directories on disk (files are nested inside) |
| Photos in Fuseki: 632 | 633 |

### Validated facts

- Music albums in Fuseki: **23,709**
- Source #3 import: **complete** (18,944 total, log says "Import Complete")
- Serial rsync: **ALL DONE** — MP3 75GB, PhotosNew 1.3GB, iPhoto 71GB
- Photos TTL on disk: 7 directories, 633 Fuseki graphs
- Photos verify: never done

### What's actually next

1. **Re-export XML** from Music.app — needed to pick up 16,977 newly imported Source #3 tracks
2. **Artwork backfill** — 1,746 albums missing (not 2,965)
3. **Photos verify** — 633 graphs in Fuseki, never smoke-tested
4. **Don't restart SMB rsyncs** — serial rsync completed the work. If you need to verify, check `/Volumes/VideosNew/Gathering/` on Bedroom.

### New: harvest pipeline is now observable

I shipped #653 tonight — 4 integration points:
- `harvest-exporter.sh` → Prometheus metrics (66 metrics, all 8 domains)
- `harvest-alerts.yml` → 3 alert rules (stale >48h, gaps >3, exporter down)
- `werk-init.sh` → harvest summary at boot (every role sees pipeline health)
- `harvest sync-board` → syncs manifest state to scope cards as comments

Your scope cards (#436, #437) now have auto-synced harvest status comments. Run `scripts/harvest sync-board` after manifest updates.
