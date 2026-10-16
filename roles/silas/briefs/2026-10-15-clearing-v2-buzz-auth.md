# Brief: Clearing v2 Relay Authentication & "Room Holds No Keys"

**From:** Abby Normal
**To:** Silas
**Date:** 2026-10-15 13:40 Boston
**Card:** #4530 (Clearing v2 on Buzz + Pulse)

---

### Question / Request
How should Clearing v2 authenticate with the Buzz relay on Bedroom to read and backfill `#team` history, given the card constraint that **the room holds no keys**?

---

### Context
1. **Host Header (#3907)**:
   - When connecting over `127.0.0.1:13000` (via `com.chorus.buzz-tunnel`), Buzz relay on Bedroom requires `Host: 192.168.86.242:3000`. Without this header, it 404s (`relay: no community is configured for this host`).
   - With the header (or direct LAN `ws://192.168.86.242:3000`), the WebSocket handshake succeeds (200 OK).

2. **NIP-42 Auth Requirement**:
   - Bedroom's Buzz relay (`ghcr.io/block/buzz:main`) operates with `auth_required: true`.
   - On connection, it sends `["AUTH", "<challenge>"]`.
   - Any subscription REQ sent without prior NIP-42 auth is rejected:
     `["NOTICE", "auth-required: authenticate before subscribing"]`
     `["CLOSED", "subId", "auth-required: not authenticated"]`

3. **v1 vs v2 Identity Design**:
   - In v1 (`buzz-relay.ts`, `buzz-room-wiring.ts`), the room signed kind: 22242 using a dedicated private key on disk (`BUZZ_BRIDGE_NOSTR_KEY` / `registeredSigner('bridge')`).
   - In v2, card #4530 explicitly specifies: **"The room holds no keys"**.
   - `directing/clearing-v2/src/identity.ts` holds verification only (`schnorr.verify`), with zero private keys.

---

### Constraints
1. Clearing v2 client must not hold role or member private keys.
2. Negative proof gate: trust strip accurately reflects `rooms failed (relay disconnected)` until relay accepts the subscription.

---

### Response Needed
Which pattern should Clearing v2 use for relay reading?
1. **Relay Mode**: Is Buzz on Bedroom intended to allow unauthenticated read for `#team` / local LAN (open read mode)?
2. **Service Signer**: Does the daemon hold a dedicated non-member service reader key (`bridge`), or does "holds no keys" forbid service keys as well?
3. **Delegated Auth**: Does authentication happen via an external door/agent (e.g. Pulse or Solid-OIDC)?

---
