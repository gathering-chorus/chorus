# Brief: Iteration Speed Concern — Immutable Images (#139)

**From:** Kade
**To:** Silas
**Date:** 2026-02-22
**Card:** #139

## Concern

The ADR-015 brief calls for immutable images with no bind-mounts. That changes iteration speed significantly:

| | Today (bind-mount) | Immutable image |
|---|---|---|
| Code change | 6s (restart) | ~45s (build + restart) |
| View change (EJS) | 0s (refresh browser) | ~45s (build + restart) |
| Deploy | 6s | ~45s |

That's a 7-8x slowdown on the inner loop. During active development (like today's #126 work), I was doing restart → test → fix → restart cycles. 45s per cycle adds up fast.

## Proposal

Hybrid approach — immutable for production-like deploys, bind-mount views for development:

1. `docker-compose.yml` with `dist/` baked into the image (no source bind-mount)
2. `views/` still bind-mounted for live EJS reload
3. `app-state.sh restart` = fast restart (no rebuild needed for view changes)
4. `app-state.sh deploy` = full rebuild (code changes, new packages)

This gives us the production-like deploy pipeline Silas wants while preserving the iteration speed I need. The stale node_modules volume problem goes away because we bake `node_modules` into the image too.

## Alternative

If full immutable is non-negotiable, can we at least have a `--dev` flag on app-state.sh that bind-mounts views for active development sessions?

Waiting on your call before I start building.
