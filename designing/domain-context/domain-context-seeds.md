# Domain Context: Seeds

Last updated: 2026-03-29 by Kade (#1843)

## ICD

No ICD defined yet for seeds domain.

## Tests

| File | Coverage |
|------|----------|
| `tests/unit/handlers/seed.handler.test.ts` | webhook, routing, error handling |
| `tests/unit/adapters/sms-seed.adapter.test.ts` | SMS payload parsing |
| `tests/unit/services/seed-sparql.service.test.ts` | Fuseki persistence |
| `tests/unit/services/seed-pod.service.test.ts` | legacy pod persistence |
| `tests/integration/seed-two-message-flow.test.ts` | two-message SMS flow |
| `tests/integration/seed-routing-flow.test.ts` | routing to all destinations |
| `tests/unit/seed-pipeline-logging.test.ts` | logging coverage |

## Persistence

| Type | Location | Details |
|------|----------|---------|
| Fuseki graph | `urn:jb:seeds/` | SMS seeds via SeedSparqlService |
| Twilio webhook | `POST /api/seed/sms` | signature-verified, phone-whitelisted |
| List endpoints | `GET /api/seed` and `GET /api/seeds` | admin-gated |

## Key Decisions

| Decision | Summary |
|----------|---------|
| #1794 | Kill dual write — Fuseki-only persistence, no more pod files |

## Constraints

- **Webhook must respond immediately.** Twilio retries on timeout, Cloudflare returns 502. As of #1843, webhook responds before Fuseki processing to prevent 11200 errors.
- **Fuseki dependency.** Seeds don't persist when Fuseki is down. No fallback. Sync endpoint exists to recover missed messages from Twilio API.
- **Dedup via MessageSid.** getKnownSids() queries Fuseki before processing. If Fuseki is down, dedup is skipped (seed may duplicate on retry).
