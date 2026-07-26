# Buzz relay — ops runbook (#3695 AC4)

The self-hosted Buzz relay is team-messaging substrate on **Bedroom** (192.168.86.242).
Stack: `~/CascadeProjects/buzz/deploy/compose/` — relay (`:3000`) + postgres:17 + redis:7 + minio,
prebuilt image `ghcr.io/block/buzz:main`. Secrets in `deploy/compose/.env` (0600, generated on-box).

## Lifecycle
- **Start/stop:** `cd ~/CascadeProjects/buzz/deploy/compose && ./run.sh start|stop|status|logs`
- **Supervision:** Bedroom login → Docker Desktop auto-starts (`AutoStart=true`, set 2026-07-25)
  → all 4 services `restart: unless-stopped`. No LaunchAgent needed; Docker is the supervisor.
- **Health:** deep-health probes `http://192.168.86.242:3000/_liveness` (`buzz-relay-http`).
  The relay is **host-routed**: the `BUZZ_DOMAIN` host (the LAN IP) selects the community;
  `localhost`/`127.0.0.1` 404 "no community configured" **by design** — always probe/connect
  via the LAN IP. NIP-11 doc: `curl -H 'Accept: application/nostr+json' http://192.168.86.242:3000/`.
- **Teardown (spike exit):** `./run.sh stop` + quit Docker Desktop. Volumes survive stop; `down -v` erases.

## Identity (the part that makes this ours — ADR-052/054 lane)
- Every actor = a `chorus:Principal` owning a `KeyRegistryEntry {keyType: nostr-secp256k1,
  keyId: <ENV NAME>, nostrPubkey}` in `urn:chorus:domains:security` (shape: `NostrCredentialShape`, #3691).
  Private keys are **never** in the graph — `nostrPubkey` only.
- **Relay membership = a projection of the allow-set**: `platform/scripts/buzz-allowlist-project`
  (diff graph→relay, add/remove, witnessed as `buzz.allowlist.projected`, DRY=1 plans, fail-closed
  on an empty read). Admit = add the credential edge + run it. Revoke = drop the edge + run it.
  Nobody hand-runs `add-member`.

## Key placement — THE DECISION (2026-07-25)
**A bound private key lives where its Principal-process runs. One key, one place. Never copied.**
- `bridge.key` → **Library** `~/.chorus/buzz/bridge.key` (0600) — the Clearing's bridge runs there.
  Moved 2026-07-25: Bedroom source `rm -P` shredded after verified transfer. Move ≠ copy: the
  source is destroyed in the same motion, so dual residence never exists.
- `wren.key` / `kade.key` / `marknakib.key` → Bedroom `~/.chorus/buzz/` (0600) until each key's
  consuming process exists; they move (same protocol) when placement is known.
- `jeff` — private key was handed to Jeff once (reveal-and-shred) and lives only in his client.
- Re-mint over recover: if a key's placement is lost or doubted, mint a fresh key and rebind
  (~30s via `buzz-admin generate-key` + DBA-path rebind + re-project) rather than hunt for copies.

## Backup (per `run.sh backup-hint`)
Before upgrades + on cadence, snapshot together (same maintenance window):
1. `deploy/compose/.env` — relay key, DB/Redis/S3 secrets, HMAC (0600; it IS the relay's identity)
2. Postgres: `docker compose exec -T postgres pg_dump -U buzz buzz > buzz-pg-$(date +%F).sql`
3. MinIO bucket (`buzz-media`) + `buzz-git-data` volume
4. NOT the role private keys — keys are re-mintable by design (see above); backups of secrets
   multiply the places they can leak from.

## Channels
`#team` = `160e1f5d-6816-4c68-94b3-b5f1ade75565` (open/stream — the Clearing mirror target,
created 2026-07-25 by bridge via kind:9007). Channel creation = admin-signed kind:9007 with
`name`/`visibility`/`channel_type` tags (+ optional `h` = client-chosen UUID); there is no
buzz-admin CLI verb for it.
