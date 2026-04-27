# #2515 — Test inventory backfill audit

Generated: 2026-04-27

**Subdomains in graph:** 48
**Currently covered (hasTest triple via discover-tests):** 7 → ['cards-service', 'chorus-domain', 'loom-decisions', 'loom-principles', 'skills-service', 'spine-service', 'tests-domain']
**Missing:** 41

## Classification of 41 missing subdomains

| Subdomain | Class | Crawled-dir hits | Uncrawled-dir hits | Sample / Reason |
|---|---|---|---|---|
| code-domain | (a) alias-name miss — files mention the base but alias map skips/folds it | 0 | 0 | `gathering/tests/unit/handlers/codebase-graph.handler.test.ts` |
| commits-domain | (b) genuinely no tests yet | 0 | 0 | `—` |
| gates-service | (a') crawl gap — alias matches files in non-crawled dirs | 0 | 21 | `chorus/directing/products/cards/tests/jdi-gate-flow.test.ts` |
| athena-domain | (a) alias-name miss — files mention the base but alias map skips/folds it | 0 | 0 | `chorus/platform/api/tests/athena-sparql.test.ts` |
| convergence-domain | (a) alias-name miss — files mention the base but alias map skips/folds it | 0 | 0 | `gathering/tests/integration/convergence-page.test.ts` |
| domains-domain | (a) alias-name miss — files mention the base but alias map skips/folds it | 0 | 0 | `chorus/platform/api/tests/regression/athena-subdomains.regression.test.ts` |
| integrations-domain | (a') crawl gap — alias matches files in non-crawled dirs | 0 | 1 | `chorus/platform/tests/nudge-integration-hermetic-default.bats` |
| knowledge-domain | (a') crawl gap — alias matches files in non-crawled dirs | 0 | 1 | `chorus/platform/tests/knowledge-domain.bats` |
| services-domain | (a) alias-name miss — files mention the base but alias map skips/folds it | 0 | 0 | `gathering/tests/unit/services/audit.service.test.ts` |
| property-domain | (a) alias-name miss — files mention the base but alias map skips/folds it | 0 | 0 | `gathering/tests/unit/handlers/property.handler.test.ts` |
| books-domain | (b) genuinely no tests yet | 0 | 0 | `—` |
| documents-domain | (b) genuinely no tests yet | 0 | 0 | `—` |
| music-domain | (a) alias-name miss — files mention the base but alias map skips/folds it | 0 | 0 | `gathering/tests/unit/handlers/music-perf.test.ts` |
| notes-domain | (a) alias-name miss — files mention the base but alias map skips/folds it | 0 | 0 | `gathering/tests/unit/handlers/notes.handler.test.ts` |
| photos-domain | (a) alias-name miss — files mention the base but alias map skips/folds it | 0 | 0 | `gathering/tests/unit/services/google-photos.service.test.ts` |
| video-domain | (b) genuinely no tests yet | 0 | 0 | `—` |
| deploys-domain | (a) alias-name miss — files mention the base but alias map skips/folds it | 0 | 0 | `chorus/platform/api/tests/deploys.test.ts` |
| infrastructure-domain | (a') crawl gap — alias matches files in non-crawled dirs | 0 | 1 | `chorus/platform/tests/infrastructure-service-design.bats` |
| pipelines-domain | (a') crawl gap — alias matches files in non-crawled dirs | 0 | 1 | `chorus/platform/tests/features/seeds/seed-pipeline.feature` |
| toolchain-domain | (b) genuinely no tests yet | 0 | 0 | `—` |
| blog-domain | (b) genuinely no tests yet | 0 | 0 | `—` |
| social-domain | (a) alias-name miss — files mention the base but alias map skips/folds it | 0 | 0 | `gathering/tests/unit/handlers/socialpost.handler.test.ts` |
| alerts-monitors-domain | (b) genuinely no tests yet | 0 | 0 | `—` |
| logs-domain | (a) alias-name miss — files mention the base but alias map skips/folds it | 0 | 0 | `chorus/platform/api/tests/logs-facet.test.ts` |
| messages-domain | (b) genuinely no tests yet | 0 | 0 | `—` |
| observability-domain | (a) alias-name miss — files mention the base but alias map skips/folds it | 0 | 0 | `chorus/platform/api/tests/observability.test.ts` |
| properties-domain | (b) genuinely no tests yet | 0 | 0 | `—` |
| loom-rcas | (b) genuinely no tests yet | 0 | 0 | `—` |
| security-domain | (a) alias-name miss — files mention the base but alias map skips/folds it | 0 | 0 | `gathering/tests/unit/security/authorization-enforcement.test.ts` |
| streams-domain | (a) alias-name miss — files mention the base but alias map skips/folds it | 0 | 0 | `gathering/tests/unit/clearing-mobile-streams.test.ts` |
| time-domain | (a) alias-name miss — files mention the base but alias map skips/folds it | 0 | 0 | `chorus/platform/api/tests/timestamp.test.ts` |
| heralds-domain | (b) genuinely no tests yet | 0 | 0 | `—` |
| people-domain | (a) alias-name miss — files mention the base but alias map skips/folds it | 0 | 0 | `gathering/tests/unit/services/people-pod.service.test.ts` |
| sexuality-domain | (a) alias-name miss — files mention the base but alias map skips/folds it | 0 | 0 | `gathering/tests/unit/handlers/sexuality.handler.test.ts` |
| stories-domain | (a') crawl gap — alias matches files in non-crawled dirs | 0 | 1 | `chorus/platform/tests/write-story.bats` |
| loom-analytics | (b) genuinely no tests yet | 0 | 0 | `—` |
| loom-metrics | (b) genuinely no tests yet | 0 | 0 | `—` |
| loom-policies | (a) alias-name miss — files mention the base but alias map skips/folds it | 0 | 0 | `chorus/platform/api/tests/handlers/loom-policies.test.ts` |
| loom-practices | (b) genuinely no tests yet | 0 | 0 | `—` |
| roles-domain | (a') crawl gap — alias matches files in non-crawled dirs | 0 | 2 | `chorus/platform/tests/auto-role-state.bats` |
| seeds-domain | (a') crawl gap — alias matches files in non-crawled dirs | 0 | 3 | `chorus/platform/tests/seed-probe-hop5.bats` |

## Summary

- (a) heuristic / alias / crawl miss (fixable in code): **27**
- (b) genuinely no tests yet (real coverage gap): **14**
- (c) cross-repo (gathering-team / chorus-consulting / etc., not crawled): **0** [not yet probed]

## Wave-2 implications

- Code change: add `directing/products/cards/tests` and `platform/tests` to the scanTests() list in discover-tests.ts (plus revisit GENERIC_BASES — `loom` is filtered which kills loom-* aliases).
- Alias additions: each (a)-classified subdomain gets a SPECIAL_ALIASES entry mapping its natural name → graph id.
- (b) bucket — file as known-coverage-gap report, not mapping bug.
