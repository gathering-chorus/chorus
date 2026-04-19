# Chorus API — Endpoint Inventory & Classification

**Silas, 2026-04-19. Under #2234 (context-service-design). First pass — 136 endpoints classified into Memory / Context / Knowledge / Mixed / Review.**

## Method

Source: `grep "app.(get|post|put|delete|patch)" platform/api/src/server.ts`. Classification rule per endpoint: which sub-domain's question does this endpoint answer? Applied the Memory/Context/Knowledge definitions from `context-service-design.md`.

- **Memory** — persistent state across sessions (history, sessions, briefs, decisions, voice archive, seed-upload assets)
- **Context** — current-moment system state (health, pulse, role-state, alerts firing, quality now, metrics)
- **Knowledge** — canonical model truth (domain/subdomain detail, ICDs, ontology, relationships, code topology)
- **Mixed** — straddles two sub-domains; candidate for splitting
- **Action** — mutates state (write, crawl, reindex); not strictly a read-sub-domain concern but included for completeness
- **Review** — unclear or likely dead; flag for retire or reshape

## Totals

| Class | Count | % |
|-------|------:|-:|
| Knowledge | 62 | 46% |
| Context | 36 | 26% |
| Memory | 17 | 13% |
| Action | 9 | 7% |
| Mixed | 9 | 7% |
| Review | 3 | 2% |
| **Total** | **136** | **100%** |

## Key Finding

**Agent-question shape is ~26% (Context).** The largest bucket is Knowledge — mostly Athena's domain/subdomain endpoints (48 of 62 Knowledge endpoints are `/api/athena/*`). Context — the sub-domain with the hallucination problem — is the SMALLEST primary category and is scattered across `/api/chorus/*` without a coherent namespace.

**Taxonomy recommendation confirmed:** the three sub-domains need dedicated URL prefixes. Today's structure mixes them under `/api/chorus/*` without discipline. An agent looking for "current role state" and an agent looking for "the chorus domain's services" both land under `/api/chorus/*` with no signal about which sub-domain they're hitting.

## Athena Endpoints (48 total)

Mostly Knowledge. Some Context/Mixed inside sub-domain detail paths.

| Endpoint | Class | Note |
|----------|-------|------|
| /api/athena/card/:id | Context | Current card state |
| /api/athena/discover-code | Knowledge | Inventory |
| /api/athena/discover-endpoints | Knowledge | Inventory |
| /api/athena/discover-pages | Knowledge | Inventory |
| /api/athena/discover-tests | Knowledge | Inventory |
| /api/athena/health | Context | System health |
| /api/athena/machines | Knowledge | Topology |
| /api/athena/owners | Knowledge | Ownership model |
| /api/athena/products | Knowledge | Canonical model |
| /api/athena/reload | Action | Graph reload |
| /api/athena/steps | Knowledge | Canonical model |
| /api/athena/subdomains | Knowledge | List |
| /api/athena/subdomains/:id | Knowledge | Detail |
| /api/athena/subdomains/:id/:section/:entityId | Knowledge | Generic |
| /api/athena/subdomains/:id/actors | Knowledge | Relationships |
| /api/athena/subdomains/:id/alerts | Context | Firing now |
| /api/athena/subdomains/:id/blast-radius | Knowledge | Code static analysis |
| /api/athena/subdomains/:id/cards | Context | Current cards |
| /api/athena/subdomains/:id/code | Knowledge | Files |
| /api/athena/subdomains/:id/completeness | Mixed | Measurement of Knowledge vs spec |
| /api/athena/subdomains/:id/consumes | Knowledge | Graph relationship |
| /api/athena/subdomains/:id/consumes/:targetId | Knowledge | Graph relationship |
| /api/athena/subdomains/:id/contract | Knowledge | ICD |
| /api/athena/subdomains/:id/contract/:entityId | Knowledge | ICD |
| /api/athena/subdomains/:id/coverage | Context | Current test coverage |
| /api/athena/subdomains/:id/gaps | Mixed | Computed delta Knowledge vs spec |
| /api/athena/subdomains/:id/gaps/:entityId | Mixed | Same |
| /api/athena/subdomains/:id/integrations | Knowledge | Relationships |
| /api/athena/subdomains/:id/integrations/:entityId | Knowledge | Detail |
| /api/athena/subdomains/:id/logs | Context | Current logs |
| /api/athena/subdomains/:id/logs/:entityId | Context | Current logs |
| /api/athena/subdomains/:id/pages | Knowledge | UI inventory |
| /api/athena/subdomains/:id/pages/:entityId | Knowledge | Detail |
| /api/athena/subdomains/:id/persistence | Knowledge | Storage spec |
| /api/athena/subdomains/:id/persistence/:eid/description | Knowledge | Detail |
| /api/athena/subdomains/:id/persistence/:entityId | Knowledge | Detail |
| /api/athena/subdomains/:id/pipeline | Knowledge | Data flow |
| /api/athena/subdomains/:id/pipeline/:entityId | Knowledge | Detail |
| /api/athena/subdomains/:id/prior-art | Knowledge | Historical references |
| /api/athena/subdomains/:id/prior-art/:entityId | Knowledge | Detail |
| /api/athena/subdomains/:id/scenarios | Knowledge | BDD |
| /api/athena/subdomains/:id/scenarios/:entityId | Knowledge | Detail |
| /api/athena/subdomains/:id/services | Knowledge | Services in subdomain |
| /api/athena/subdomains/:id/services/:eid/description | Knowledge | Detail |
| /api/athena/subdomains/:id/services/:entityId | Knowledge | Detail |
| /api/athena/subdomains/:id/test-coverage | Context | Current test coverage |
| /api/athena/subproducts | Knowledge | Canonical model |
| /api/athena/validate | Action | SHACL/schema validate |

**Athena subtotal:** Knowledge 32, Context 9, Mixed 3, Action 2, Review 0 + 2 unclassified rows.

**Recommendation:** Most of Athena maps cleanly to `/api/chorus/knowledge/*`. Sub-domain detail paths that are Context (alerts, cards, coverage, logs, test-coverage) should live under `/api/chorus/context/*` and cross-reference the subdomain by ID. Today they're conflated with Knowledge under `/api/athena/subdomains/:id/*`.

## Chorus Endpoints (85 total)

Most chaotic. Clear split into sub-domains required.

### Context (shipping state) — 23
- /api/chorus/alert — firing alerts
- /api/chorus/attention-analytics — session attention
- /api/chorus/cost — current cost tracking
- /api/chorus/cost/summary — same
- /api/chorus/disk — disk state
- /api/chorus/domain/:name/alerts — firing alerts per domain
- /api/chorus/domain/:name/logs — current logs
- /api/chorus/fitness/summary — current fitness metrics
- /api/chorus/freshness — index freshness
- /api/chorus/health — system health
- /api/chorus/health/detail — detailed
- /api/chorus/hooks/metrics — current hook metrics
- /api/chorus/hooks/summary — same
- /api/chorus/patterns/summary — current patterns
- /api/chorus/perf — current perf
- /api/chorus/pulse — pulse snapshot
- /api/chorus/pulse/latest — latest pulse
- /api/chorus/quality/domain/:domain — current quality per domain
- /api/chorus/quality/summary — current quality overall
- /api/chorus/reprompt-analytics — current reprompt data
- /api/chorus/role-state — current role states
- /api/chorus/stats — current stats
- /api/chorus/voice/:role — current voice identity

### Memory (historical/persistent) — 14
- /api/chorus/card-story/:id — card lifecycle history
- /api/chorus/conversation — conversation history
- /api/chorus/domain-story/:domain — domain narrative
- /api/chorus/domain/:name/decisions — decision records
- /api/chorus/domain/:name/releases — release history
- /api/chorus/rca — RCA detail
- /api/chorus/rcas — RCA list
- /api/chorus/seed-media/:filename — uploaded assets
- /api/chorus/seeds — seed history
- /api/chorus/sessions — session index
- /api/chorus/sessions/:id — session detail
- /api/chorus/sessions/:id/log — session log
- /api/chorus/trace — trace history
- /api/chorus/trace/:correlationId — detail
- /api/chorus/voice-analytics — voice history
- /api/chorus/werk/activity — activity history

### Knowledge (canonical truth) — 16
- /api/chorus/codebase/topology — code topology
- /api/chorus/domain/:domain/code-files — files per domain
- /api/chorus/domain/:name — domain detail (canonical)
- /api/chorus/domain/:name/blast-radius — static analysis
- /api/chorus/domain/:name/code — files
- /api/chorus/domain/:name/dependencies — graph relationships
- /api/chorus/domain/:name/infra — infrastructure topology
- /api/chorus/domain/:name/pipeline — data flow spec
- /api/chorus/domain/:name/radius — blast radius
- /api/chorus/domain/:name/services — services
- /api/chorus/domain/:name/tests — test files
- /api/chorus/domains — list
- /api/chorus/products — canonical
- /api/chorus/services — list
- /api/chorus/tests — list
- /api/chorus/tests/:domain — per domain

### Mixed — 5
- /api/chorus/index — ambiguous; index of what?
- /api/chorus/refs — ambiguous; refs to what?
- /api/chorus/search — spans Memory/Knowledge/Context
- /api/chorus/self — ambiguous; self of what?
- /api/chorus/trace/integrations/:domain — history + static

### Action (mutate) — 7
- /api/chorus/crawl/:domain — crawl trigger
- /api/chorus/embed — embed trigger
- /api/chorus/harvest — harvest trigger
- /api/chorus/open — open in browser
- /api/chorus/reconcile — reconcile trigger
- /api/chorus/reindex — reindex trigger
- /api/chorus/spine-event — write spine event

### Review (likely dead or unclear) — 3
- /api/chorus/jeff/posture/strip — namespace is weird; likely legacy UI-specific
- /api/chorus/conversation — overlap with /sessions? verify
- /api/chorus/self — purpose unclear

## ICD Endpoints (3 total)

All Knowledge — interface contract detail.

- /api/icd/domains/:id/fields
- /api/icd/domains/:id/mappings
- /api/icd/domains/:id/providers/:pid/sections

## Target Shape After Rework

```
/api/chorus/context/...
  /board            from: /api/chorus/pulse, /api/athena/subdomains/:id/cards
  /roles            from: /api/chorus/role-state
  /health           from: /api/chorus/health, /api/athena/health
  /alerts           from: /api/chorus/alert, /api/chorus/domain/:name/alerts
  /cost             from: /api/chorus/cost*
  /quality          from: /api/chorus/quality*
  /perf             from: /api/chorus/perf, /api/chorus/hooks/*
  /freshness        from: /api/chorus/freshness
  /stats            from: /api/chorus/stats
  /analytics        from: /api/chorus/attention-analytics, /reprompt-analytics
  /voice/:role      from: /api/chorus/voice/:role
  /coverage         from: /api/athena/subdomains/:id/coverage, /test-coverage
  /logs             from: /api/chorus/domain/:name/logs, /api/athena/subdomains/:id/logs

/api/chorus/memory/...
  /sessions         from: /api/chorus/sessions*
  /briefs           (missing; need to add)
  /decisions        from: /api/chorus/domain/:name/decisions
  /activity         from: /api/chorus/werk/activity
  /rcas             from: /api/chorus/rcas, /rca
  /traces           from: /api/chorus/trace*
  /stories          from: /api/chorus/card-story/:id, /domain-story/:domain
  /seeds            from: /api/chorus/seeds, /seed-media/:filename
  /voice            from: /api/chorus/voice-analytics
  /releases         from: /api/chorus/domain/:name/releases

/api/chorus/knowledge/...
  /domains          from: /api/chorus/domains, /api/athena/products, /subproducts, /steps
  /domains/:name    from: /api/chorus/domain/:name, /api/athena/subdomains/:id
  /domains/:name/services       from: /api/chorus/domain/:name/services, /api/athena/.../services
  /domains/:name/contract       from: /api/athena/subdomains/:id/contract
  /domains/:name/infra          from: /api/chorus/domain/:name/infra
  /domains/:name/blast-radius   from: /api/chorus/domain/:name/blast-radius, /radius
  /domains/:name/dependencies   from: /api/chorus/domain/:name/dependencies
  /domains/:name/code           from: /api/chorus/domain/:name/code, /api/athena/subdomains/:id/code
  /domains/:name/pipeline       from: /api/chorus/domain/:name/pipeline, /api/athena/subdomains/:id/pipeline
  /domains/:name/tests          from: /api/chorus/domain/:name/tests, /api/chorus/tests/:domain
  /domains/:name/actors         from: /api/athena/subdomains/:id/actors
  /domains/:name/consumes       from: /api/athena/subdomains/:id/consumes
  /domains/:name/gaps           from: /api/athena/subdomains/:id/gaps
  /domains/:name/scenarios      from: /api/athena/subdomains/:id/scenarios
  /domains/:name/prior-art      from: /api/athena/subdomains/:id/prior-art
  /domains/:name/integrations   from: /api/athena/subdomains/:id/integrations
  /domains/:name/persistence    from: /api/athena/subdomains/:id/persistence
  /domains/:name/pages          from: /api/athena/subdomains/:id/pages
  /domains/:name/completeness   from: /api/athena/subdomains/:id/completeness
  /topology                     from: /api/chorus/codebase/topology, /api/athena/machines
  /owners                       from: /api/athena/owners
  /icd/:id                      from: /api/icd/domains/:id/*
  /search                       from: /api/chorus/search

/api/chorus/actions/...  (Services vertical, mutations)
  /crawl/:domain          from: /api/chorus/crawl/:domain
  /harvest                from: /api/chorus/harvest
  /reindex                from: /api/chorus/reindex
  /reconcile              from: /api/chorus/reconcile
  /embed                  from: /api/chorus/embed
  /reload                 from: /api/athena/reload
  /validate               from: /api/athena/validate
  /spine-event            from: /api/chorus/spine-event
  /open                   from: /api/chorus/open
```

## Retirement Candidates

After reshape, these 3 endpoints should be investigated for retire vs. migrate:
- `/api/chorus/jeff/posture/strip` — namespace suggests legacy UI-only integration
- `/api/chorus/self` — purpose unclear
- `/api/chorus/conversation` — likely duplicates `/sessions`

Plus the existing `/api/chorus/domain/:name/*` endpoints should largely be deprecated in favor of the `/api/chorus/knowledge/domains/:name/*` shape — they're duplicates of Athena subdomains by another name.

## Next Steps (per #2234 Implementation Outline)

- [x] **Step 1: Endpoint inventory audit** — this document
- [ ] **Step 2: Design minimum Context endpoint set** with declared response schemas — follows below in separate artifact
- [ ] **Step 3: Common envelope implementation** — Kade-owned, Services vertical
- [ ] **Step 4: Data correctness** — pick three staleness sources, fix one
- [ ] **Step 5: Presentation** — reshape three worst-shaped responses
- [ ] **Step 6: Push envelope reshape** — to manifest + orientation
- [ ] **Step 7: Demonstration** — live citation from `/context/*` in a role response

## References

- `context-service-design.md` — parent design
- `chorus-overview.md` — refreshed service design
- `chorus-context-diagram-v2.html` — canonical visual
- `platform/api/src/server.ts` — source of truth for current routes
- #2234 — the card this feeds
