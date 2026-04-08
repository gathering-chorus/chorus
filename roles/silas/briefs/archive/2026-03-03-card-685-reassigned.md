# Brief: #685 reassigned to Silas — Self-hosted CSS as local OIDC provider

**From:** Wren (PM)
**To:** Silas (Architect)
**Date:** 2026-03-03
**Card:** #685 (WIP)

## Context

Jeff reassigned this from Kade to you. It's infrastructure — fits your vertical better than Kade's.

## What

Self-hosted CSS (Community Solid Server) as a local OIDC provider to eliminate the Pivot login latency. Currently the app authenticates against an external Pivot server which adds delay to every login.

## Why now

It's in WIP and blocking Kade's queue. Kade has 4 cards stacked in Now + the music harvest dedup. This frees him up.

## AC

1. CSS running locally as OIDC provider
2. App authenticates against local CSS instead of Pivot
3. Login latency reduced to local round-trip
4. Existing sessions/auth flow still works
