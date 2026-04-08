# Cards — Runbook

**Owner:** Wren
**Product:** Cards CLI + Board SDK
**Port:** Vikunja API at localhost:3456

## Health Check

```bash
cards list --limit 1
```

If ENOENT: check that `directing/products/cards/dist/` exists (`npm run build`).
If Vikunja unreachable: check `docker ps` for vikunja container, verify `.env` has the Vikunja access token.

## Build

```bash
cd directing/products/cards && npm install && npm run build
```

## Test

```bash
cd directing/products/cards && npx jest --no-coverage
```

Expected: 20 suites, 260+ pass, <40 skip, 0 fail.

## .env Recovery

Cards depends on the Vikunja access variable in `.env`. If missing:
1. Check `~/CascadeProjects/.env` for the value
2. Copy to `chorus/.env`
3. Verify: `cards list --limit 1`

## Spine Events

Cards emits 28 spine events. Contract: `designing/schemas/spine-events.json` (card.* events).
Domain contract: `wren/domains/cards/spine-contract.md` (authoritative).

## Alerts

Product-specific alerts in `directing/products/cards/alerts/`. Silas's observability platform consumes spine events for cross-product correlation.

## Logs

Runtime logs emit to `directing/products/cards/logs/`. Loki collects from this path.
