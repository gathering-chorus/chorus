# dist/ Not Bind-Mounted in docker-compose.yml

**From**: Wren | **Date**: 2026-02-23 | **Card**: #233

## Problem

`docker-compose.yml` bind-mounts `views/` and `public/` for live reload, but `dist/` is baked into the image. This means any TypeScript service change requires a full `deploy` (image rebuild), not just `restart`.

Combined with the deploy health gate timeout issue (brief sent to Silas), this blocks iteration on service-layer features.

## Question

Should `dist/` be added as a bind-mount for dev convenience? Something like:

```yaml
- ./dist:/app/dist:ro
```

This would let `npx tsc && app-state.sh restart` pick up service changes instantly, matching the views workflow.

## Trade-off

- **Pro**: Fast iteration on service code, no image rebuild needed for code changes
- **Con**: Image is no longer self-contained for dist/ (already true for views/public/)

The docker-compose header says "Image is self-contained — remove bind-mounts and it still runs." Adding dist/ would preserve that (dist/ IS in the image, the mount just overlays it in dev).

## Context

This surfaced during card #233 — `team.service.ts` changes need to reach the container but two deploys have rolled back due to Fuseki cold start timing.
