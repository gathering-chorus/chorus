# Deploy Health Gate Timeout — Value Stream Page Blocked

**From**: Wren | **Date**: 2026-02-23 | **Card**: #233

## Problem

Card #233 (card-centric Value Stream page) has service-layer changes in `team.service.ts` that need a full `deploy` to reach the container (dist/ is baked into the image, not bind-mounted). Two consecutive deploys have failed: build succeeds, but the 30s health gate times out during Fuseki fullSyncAll cold start, triggering auto-rollback.

The previous deploy (92s, successful) hit a warm Fuseki. These hit cold Fuseki after container recreation.

## What's Stuck

- `team.service.ts` adds `getCardViews()` method + Docker-aware Vikunja URL (`VIKUNJA_BASE`)
- `VIKUNJA_TOKEN` is now in `.env` and reaches the container on deploy
- `value.ejs` is card-centric (bind-mounted, already live) but gets zero cards because the running service code is the old image

## Ask

1. **Extend the deploy health gate** from 30s to 60-90s for cold starts? Or make it configurable?
2. **Alternative**: Two-phase health — pass basic health (Express listening) immediately, defer fullSyncAll readiness to a separate check?
3. The `wait_for_healthy` at line 438 of `app-state.sh` is the gate. Line 326 (start command) already uses 60s — should deploy match?

## Context

`app-state.sh` line 438: `wait_for_healthy "$APP_CONTAINER" 30`
`app-state.sh` line 326: `wait_for_healthy "$APP_CONTAINER" 60` (start command uses 60s)

The app itself starts fine — `/health` returns OK within seconds. The issue is Fuseki `depends_on: service_healthy` in docker-compose means Fuseki must be healthy before the app container even starts, adding to the total time.
