# #2515 audit v2 — post wave-2 heuristic

Generated: 2026-04-27

**Covered:** 31/48
**Missing:** 17

## Classification of 17 missing

| Subdomain | Bucket | Files mention base? | Notes |
|---|---|---|---|
| alerts-monitors-domain | genuinely-no-tests | 0 | `—` |
| code-domain | generic-bases | 9 | `gathering/tests/unit/handlers/codebase-graph.handler.test.ts` |
| commits-domain | genuinely-no-tests | 0 | `—` |
| domains-domain | generic-bases | 2 | `chorus/platform/api/tests/regression/athena-subdomains.regression.test.ts` |
| heralds-domain | genuinely-no-tests | 0 | `—` |
| loom-analytics | genuinely-no-tests | 0 | `—` |
| loom-metrics | genuinely-no-tests | 0 | `—` |
| loom-practices | genuinely-no-tests | 0 | `—` |
| loom-rcas | genuinely-no-tests | 0 | `—` |
| messages-domain | generic-bases | 0 | `—` |
| property-domain | plural-folding | 3 | `gathering/tests/unit/handlers/property.handler.test.ts` |
| services-domain | generic-bases | 89 | `gathering/tests/unit/services/audit.service.test.ts` |
| streams-domain | generic-bases | 1 | `gathering/tests/unit/clearing-mobile-streams.test.ts` |
| tests-domain | by-design | 533 | `gathering/tests/chorus-explorer-layout.test.js` |
| time-domain | generic-bases | 3 | `chorus/platform/api/tests/timestamp.test.ts` |
| toolchain-domain | genuinely-no-tests | 0 | `—` |
| video-domain | genuinely-no-tests | 0 | `—` |

## Buckets

- by-design: **1**
- plural-folding: **1**
- generic-bases: **6**
- genuinely-no-tests: **9**
- other: **0**

**Phase 0 exit:** 31/48 hasTest mapping declared. Remainder named: by-design 1, plural-folding 1, generic-bases tradeoff 6, genuinely-no-tests 9, other 0.
