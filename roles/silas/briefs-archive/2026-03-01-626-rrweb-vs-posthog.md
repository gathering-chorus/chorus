# Brief: #626 — rrweb standalone vs PostHog self-hosted

**From:** Kade
**Date:** 2026-03-01
**Card:** #626

## Finding

PostHog self-hosted is 20+ containers (ClickHouse, Kafka, Postgres, Redis, MinIO, Temporal, Elasticsearch, ~6 Rust microservices). Official minimum: 4 vCPU, 16GB RAM, 30GB disk. Even on Bedroom (15GB free), this is a large footprint for session replay.

## Alternative: rrweb standalone

rrweb is the library PostHog uses under the hood for session recording. We can use it directly:

- **Zero new containers.** npm package + Express route in the existing app on Library.
- **Recording:** `rrweb.record()` captures DOM mutations as JSON, batches to `/api/sessions` endpoint.
- **Storage:** JSON files on disk. ~1-5MB per 5-min session.
- **Playback:** `rrweb-player` renders sessions in a viewer page at `/admin/replay`.
- **Build time:** 2-3 hours. All in the existing app.

## Trade-off

| | PostHog | rrweb standalone |
|---|---|---|
| Session replay | Yes | Yes |
| Page analytics / funnels | Yes | No |
| Heatmaps | Yes | No |
| New containers | 20+ | 0 |
| RAM cost | ~4GB | ~0 |
| Build time | Install + configure | 2-3 hours code |
| Maintenance | Significant | Minimal |

## Recommendation

Start with rrweb standalone. Jeff's stated need is "replay my session so you can see what I experience." That's pure session replay — no funnels, no heatmaps needed yet. If we want analytics later, PostHog Cloud free tier (1M events/month) is an option without self-hosting.

## Question for Silas

Is there an infra reason to prefer Bedroom-hosted PostHog over app-embedded rrweb? The capacity is there, but the maintenance burden is real.
