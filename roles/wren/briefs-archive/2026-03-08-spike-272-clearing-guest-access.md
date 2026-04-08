# Spike Complete: #272 — Clearing Guest Access

**From:** Silas
**To:** Wren
**Card:** #272 Spike: Clearing guest access — external party joins as meeting visitor

## Finding: Build it.

~175 lines across 5 files. Small, well-scoped, infrastructure exists.

## What's needed

| Change | Size | Where |
|--------|------|-------|
| Session token auth (128-bit, Socket.IO middleware) | ~20 lines | server.ts |
| Multi-human identity (join handshake, replaces hardcoded "Jeff") | ~30 lines | server.ts |
| Cloudflare quick tunnel (`cloudflared tunnel --url`) | ~15 lines | bin/clearing |
| Guest-safe system prompts (roles don't leak internal context) | ~10 lines | participants.ts |
| Client join screen (name entry before chat) | ~60 lines | index.html |
| File proxy disabled when guest present | ~5 lines | server.ts |
| `clearing --guest` CLI flag | ~15 lines | bin/clearing |

## Security model

- Session token: 128-bit random, required for Socket.IO handshake, dies with session
- Quick tunnel URL: random `*.trycloudflare.com`, not discoverable
- File access: disabled when guest present
- Transcript: guest messages recorded with their name, indexed to Chorus
- No persistent accounts, no multi-guest, no guest AI roles

## What NOT to build (MVP)

- Multi-guest (one per session)
- Guest AI roles
- Persistent accounts
- Chat replay (guest sees from join time forward)

## Recommendation

Card this as a single build card for Silas. One session of work. Dependencies: `cloudflared` must be installed (already is for the external tunnel).

Full technical details in `architect/briefs/spike-clearing-guest-access.md`.
