# Seeds — Product Template

## Name
Seeds

## Owner
Kade (Gathering code: handler, adapter, SPARQL service, pod service, triage UI)
Silas (monitoring: alerting, Cloudflare tunnel, Loki queries)
Wren (product: service design, operating model, AC)

## Domain
Seeds — Jeff sends a text from his phone, it arrives in the system, gets classified, and Jeff sees proof. No silent drops.

## Services

| Service | Location | Runtime | Owner |
|---------|----------|---------|-------|
| SMS webhook handler | `jeff-bridwell-personal-site/src/handlers/seed.handler.ts` | Express :3000 | Kade |
| SMS adapter | `jeff-bridwell-personal-site/src/adapters/sms-seed.adapter.ts` | Express :3000 | Kade |
| Seed SPARQL service | `jeff-bridwell-personal-site/src/services/seed-sparql.service.ts` | Express :3000 | Kade |
| Seed pod service | `jeff-bridwell-personal-site/src/services/seed-pod.service.ts` | Express :3000 | Kade |
| Seed capture service | `jeff-bridwell-personal-site/src/services/seed-capture.service.ts` | **ORPHANED** — built, not wired | Kade |
| Seed triage UI | `jeff-bridwell-personal-site/views/seeds/triage.ejs` | Express :3000 | Kade |
| Seed media endpoint | Chorus API :3340 `/api/chorus/seed-media/` | Chorus | Silas |
| Seed probe | `chorus/platform/scripts/check-seeds.sh` | CLI | Silas |
| Cloudflare tunnel | External → :3000 | Cloudflare | Silas |

## Tests

### Gathering (unit + integration) — 235 green
| Suite | File | Tests |
|-------|------|-------|
| Handler | `tests/unit/handlers/seed.handler.test.ts` | unit |
| SPARQL service | `tests/unit/services/seed-sparql.service.test.ts` | unit |
| Capture service | `tests/unit/services/seed-capture.service.test.ts` | unit |
| Pod service | `tests/unit/services/seed-pod.service.test.ts` | unit |
| SMS adapter | `tests/unit/adapters/sms-seed.adapter.test.ts` | unit |
| Pipeline logging | `tests/unit/seed-pipeline-logging.test.ts` | unit |
| Pipeline flow | `tests/integration/seed-pipeline-flow.test.ts` | integration |
| Webhook e2e | `tests/integration/seed-webhook-e2e.test.ts` | integration |
| Routing flow | `tests/integration/seed-routing-flow.test.ts` | integration |
| Two-message flow | `tests/integration/seed-two-message-flow.test.ts` | integration |
| Triage e2e | `e2e/tests/seed-triage.spec.ts` | e2e |

### Chorus (BDD) — 11 scenarios, 41 steps green
| Feature | File | Scenarios |
|---------|------|-----------|
| Pipeline | `platform/tests/features/seeds/seed-pipeline.feature` | 4 (arrive + anti-patterns) |
| Media | `platform/tests/features/seeds/seed-media.feature` | 3 (serve, 404, path traversal) |
| Alerting | `platform/tests/features/seeds/seed-alert.feature` | 4 (write fail, app down, healthy, probe) |

## Spine Events

| Event | Emitted by | When |
|-------|-----------|------|
| `seed.arrived` | SMS handler | Twilio webhook received, seed persisted |
| `seed.classified` | Capture service | Auto-classification complete (when wired) |
| `seed.routed` | Handler | Seed routed to role via hashtag or default |
| `seed.write.failed` | SPARQL service | Fuseki write error |

## Gates

Per card completion pipeline (#1812):

| Gate | Owner | Automated checks |
|------|-------|-----------------|
| Product | Wren | AC items, demo ran |
| Code | Kade | Jest unit+integration green, build clean, no new warnings |
| Quality | Kade | Hooks pass, no console.log in production, no deleted tests |
| Architecture | Silas | Namespace/URI conventions, ICD consistency, domain boundary |
| Ops | Silas | app-state.sh deploy, health check, Loki logs flowing, rollback |

## Known Gaps

| Gap | Severity | Status |
|-----|----------|--------|
| **seed-capture.service.ts orphaned** — auto-classifier built but no handler/route imports it | High | Not wired |
| **Bridge confirmation** — Jeff gets no visual confirmation when a seed lands | Medium | Gap carded |
| **Close-the-loop** — Jeff can't see what a role did with his seed | Medium | Gap carded |
| **Integration test teardown leak** — worker force-exits, likely timer/connection not cleaned | Low | Debt |
| **BDD alert steps are stubs** — simulate alert logic in-memory, don't hit live Fuseki/Bridge/Loki | Low | Acceptable for pilot |
| **Cloudflare tunnel = silent SPOF** — if tunnel drops, seeds silently fail | High | Silas monitoring |
