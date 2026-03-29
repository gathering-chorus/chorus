# Brief: Deploy stability changes in app-state.sh — know before you deploy

**From:** Silas (Architect)
**To:** Kade (Engineer)
**Date:** 2026-03-02

## What changed in app-state.sh (live now, no deploy needed)

### 1. 3-minute cooldown
After any successful deploy or restart, the next attempt within 180s is skipped. You'll see:
```
Cooldown: last deploy by kade was 45s ago (135s remaining). Skipping.
```
Override with `DEPLOY_FORCE=1` if genuinely needed.

### 2. Healthy-skip on restart
`app-state.sh restart` now checks container health first. If already healthy, it skips:
```
Container already healthy — restart skipped
```
Override with `DEPLOY_FORCE=1`.

### 3. Compose up scoped to app only
`docker compose up -d` → `docker compose up -d app` in deploy and rollback paths. **Fuseki no longer restarts on app deploys.** This cuts deploy downtime from 30-90s to ~5-10s.

Cold start (`app-state.sh start` with no containers) still brings up all services.

## What this means for you

- **Push once per batch**, not per commit. The pre-push hook triggers deploy on push when src/ changes.
- If you push and see "Cooldown... Skipping" — that's working as intended. Wait 3 minutes or batch your changes.
- If you genuinely need to force a deploy: `DEPLOY_FORCE=1 DEPLOY_ROLE=kade ./app-state.sh deploy`
- View/CSS changes still need no deploy at all (bind mounts).

## Also: source sequencing brief

See `2026-03-02-music-source-sequencing.md` — Jeff directed: finish Bedroom MP3 (Source #3), then sequential through remaining sources. One at a time. Separate brief with full details.
