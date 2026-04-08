# Brief: Self-hosted CSS as local OIDC provider

**From:** Silas (Architect)
**To:** Kade (Engineer)
**Date:** 2026-03-02
**Card:** #685 (Next, P1)

## Why

Pivot (solidcommunity.net) is too slow — 3.5s OIDC discovery timeouts, login loops, broken PKCE flow. Jeff hit it again today. Every deploy requires re-authentication, and every re-auth is a coin flip on whether Pivot responds in time.

## What

Add Community Solid Server (CSS) v7.1.8 to docker-compose as a local OIDC provider. Login goes from 3.5s to <100ms.

## Spike brief

Full spike at `architect/briefs/2026-03-02-spike-css-local-oidc-provider.md`. Read it — it has the compose definition, seed script, and all files that need changing.

## Quick summary

1. **Add CSS to docker-compose** — port 3001, file-based persistence, health check
2. **Write seed script** — creates Jeff's account + pod via CSS JSON API at `/.account/`
3. **Update 5-6 files** — login form (add local provider), callback handler (add issuer mapping), authorized-users (add local WebID), CSP headers, env var for `SOLID_OIDC_ISSUER`
4. **Test** — login via local CSS, verify session sticks, verify solidcommunity.net still works as fallback
5. **Default to local** once stable

## Key gotcha

`CSS_BASE_URL` must exactly match what the browser sees. If it's `https://localhost:3001/` in the compose file but the browser hits `http://localhost:3001/`, OIDC discovery breaks silently.

## Silas reviews

I'll review the compose/infra side. Ping me when the compose definition is ready.
