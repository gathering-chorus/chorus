# ADR-025: Ontology graph vs Instances graph separation

**Status:** Accepted — 2026-06-17 (Jeff, final). Proposed 2026-04-24 (Silas); ratified per the 2026-06-17 coherence audit — it was load-bearing under ADR-027/028/040 (Accepted-resting-on-Proposed, now cleared).
**Card:** #2468 (ADR) — blocks #2314 (Loom Principles API) and #2437 follow-ups.
**Supersedes:** — (establishes the rule for the first time; prior graph writes happened case-by-case without a stated policy).

## Context

The chorus graph today mixes schema and populated content in `urn:chorus:ontology`:

| Class                | urn:chorus:ontology | urn:chorus:instances |
|----------------------|--------------------:|---------------------:|
| chorus:Principle     | **27**              | 0                    |
| chorus:Practice      | **40**              | 0                    |
| chorus:Policy        | **14**              | 0                    |
| chorus:Skill         | **32**              | 0                    |
| chorus:Gate          | **17**              | 0                    |
| chorus:SubDomain     | **48**              | 0                    |
| chorus:Product       | **3**               | 0                    |
| chorus:SubProduct    | **6**               | 0                    |
| chorus:Vertebra      | **11**              | 0                    |
| chorus:Machine       | **2**               | 0                    |
| chorus:Actor         | 0                   | **49**               |
| chorus:Scenario      | 0                   | **19**               |
| chorus:Page          | 0                   | **63**               |
| chorus:Service       | 0                   | **42**               |
| chorus:Decision      | 0                   | 0                    |
| chorus:Role          | 0                   | 0                    |
| chorus:Metric        | 0                   | 0                    |

**200 content instances live in the ontology graph.** 173 live in the instances graph. The Athena write handler (`ENTITY_SECTIONS` in `handlers/subdomain-entities.ts`) writes to `urn:chorus:instances` for all per-subdomain content added via the POST pattern (actors, scenarios, pages, services). Hand-landed content (principles, practices, policies, subdomains, products, subproducts, skills, gates) sits in the ontology graph because it was added alongside class declarations before the write API existed.

Card #2314 (Loom Principles API) surfaced this: any POST handler for principles must decide which graph to write to, and either choice entrenches a bad pattern — writing to ontology mixes schema and content further, writing to instances creates a read/write split (principles readable from ontology, new ones written to instances).

## Decision

**Separation rule.**

- **`urn:chorus:ontology`** contains **only schema**: OWL class declarations, RDFS property declarations, OWL/RDFS axioms, SHACL shapes, RDFS `comment` on classes/properties. No instance data.
- **`urn:chorus:instances`** contains **all populated content**: every instance of every class that isn't a class/property definition itself. Principles, practices, policies, decisions, roles, skills, gates, metrics, subdomains, products, subproducts, services, actors, scenarios, pages, vertebrae, machines.
- **SHACL shapes live in ontology** and validate writes to instances across the graph boundary (Fuseki supports cross-graph SHACL).
- **URIs are preserved across migration.** Moving triples between graphs does not change subject URIs. `chorus:principle-observe` stays `chorus:principle-observe` — citations, links, and external references remain valid.

## Consequences

### Handlers that change

| File                                             | Current behavior                               | After migration                                  |
|--------------------------------------------------|------------------------------------------------|--------------------------------------------------|
| `platform/api/src/handlers/athena-subdomains.ts` | Reads Subdomains from ontology graph           | Reads from instances graph                       |
| `platform/api/src/handlers/athena-products.ts`   | Reads Products from ontology                    | Reads from instances                             |
| `platform/api/src/handlers/athena-subproducts.ts`| Reads SubProducts from ontology                 | Reads from instances                             |
| `platform/api/src/handlers/athena-steps.ts`      | Reads Vertebrae from ontology                    | Reads from instances                             |
| `platform/api/src/handlers/athena-machines.ts`   | Reads Machines from ontology                     | Reads from instances                             |
| `platform/api/src/handlers/loom-principles.ts`   | Reads Principles from ontology                   | Reads from instances                             |
| `platform/api/src/handlers/loom-practices.ts`    | Reads Practices from ontology                    | Reads from instances                             |
| `platform/api/src/handlers/loom-policies.ts`     | Reads Policies from ontology                     | Reads from instances                             |
| `platform/api/src/handlers/loom-skills.ts`       | Reads Skills from ontology                       | Reads from instances                             |
| `platform/api/src/handlers/loom-gates.ts`        | Reads Gates from ontology                        | Reads from instances                             |

Every SPARQL query currently scoped `GRAPH <urn:chorus:ontology>` for class instance data is rewritten to `GRAPH <urn:chorus:instances>`. Schema queries (class listing, property enumeration, SHACL shape lookup) stay on the ontology graph.

### Queries that change

- `SELECT ?p WHERE { GRAPH <urn:chorus:ontology> { ?p a chorus:Principle } }` → `GRAPH <urn:chorus:instances>`.
- Same pattern for Practice, Policy, Skill, Gate, SubDomain, Product, SubProduct, Vertebra, Machine.
- SHACL validation remains ontology-graph-scoped and cross-validates instances graph.

### Writes

- All new POST/PUT/DELETE handlers write to `urn:chorus:instances`.
- `ENTITY_SECTIONS` pattern in `subdomain-entities.ts` extended to handle class-level content (principles, practices, etc.) following the same `{id, section, data}` shape.
- #2314 becomes the first implementation of the new rule: Loom Principles API extends the existing write pattern, writing to instances.

### Side effects

- No UI breakage if migration is atomic (all classes migrate in one reload). Queries flip from ontology to instances in the same commit as the data moves.
- During rolling migration: queries may return empty until both data and handler flip. Migrations should be per-class atomic: move-data + flip-handler-query in one commit.
- SPARQL federation across graphs stays correct — the data is the same, only its graph annotation changes.

### Gaps this exposes

- **Decision, Role, Metric classes have zero instances** in either graph. They're declared but unpopulated. Separate backlog cards, not blockers for this migration.
- **Service class has 42 instances in instances graph but `chorus:Service` declaration doesn't appear to be in active use for runtime services** (runtime Services are modeled via `borg:Environment`). Clarify in a follow-on whether `chorus:Service` and `borg:Environment` should reconcile or remain distinct concepts.

## Migration plan

**Parent card:** #2469 (to be filed alongside this ADR) — "Migrate content instances from urn:chorus:ontology to urn:chorus:instances (per ADR-025)."

**Per-class children** (one migration card each, serializable to run in order). Order is **lowest-blast-radius first, highest-blast-radius last**, so the per-class atomic-commit pattern is proven on small classes before touching the high-traffic ones (Wren's PM-review reorder, 2026-04-25).

1. **Principle** (27 instances) — first migrant. Lowest blast (`/loom/principles.html` only consumer), cleanest data after #2447. Pairs with #2314 which ships writes on the new location.
2. **Practice** (40) — second.
3. **Policy** (14)
4. **Skill** (32)
5. **Gate** (17)
6. **Vertebra** (11)
7. **Machine** (2)
8. **Product** (3)
9. **SubProduct** (6)
10. **SubDomain** (48) — **last migrant.** Highest blast: every Athena read handler the team uses for `/pull`, `/gate-product`, domain pages, and `/pull`'s design gate (`/api/athena/subdomains/{X}-domain/completeness`). Migrating SubDomain mid-flight breaks every `/pull` invocation. Run last after the pattern is proven on the 9 lower-risk classes.

Each child card:
- Migrates that class's instances from ontology to instances graph (SPARQL `DELETE ... INSERT` or equivalent reload script).
- Flips the relevant handler SPARQL queries.
- Verifies read API returns same data post-migration.
- One commit per class; atomic within a class, not atomic across classes.

### Cross-class join risk

Mid-migration (between class N and class N+1), some Athena queries may break if they `JOIN` across classes that sit in different graphs. Example: a query joining `chorus:SubDomain` and `chorus:Service` where one is in ontology and the other in instances. Before locking the migration order, scan `platform/api/src/sparql/*.sparql` for `GRAPH <urn:chorus:ontology>` clauses that cross-reference multiple class types. Any cross-class join is a constraint on co-migration: the joined classes must move together (one commit, two classes), or the query must be rewritten to read across both graphs (`UNION` over both graph clauses) for the duration of the migration.

### SHACL across graphs

Fuseki documents support cross-graph SHACL validation, but verify rather than trust. Before the first migration ships, hermetic test: stage a known-bad Principle write to `urn:chorus:instances`, verify SHACL shape from `urn:chorus:ontology` rejects it. If it doesn't, the separation rule still holds but enforcement is weaker until cross-graph SHACL is wired explicitly.

## Dependencies

- **Blocks:** #2314 (Loom Principles API) — depends on Principle class being migrated first.
- **Informed by:** #2437 (competing-implementations audit) — naming the specific "parallel write path" that would have been created.
- **Related:** #2447 (principles graph completeness — already shipped in ontology graph; first migrant will pull it forward into instances).
- **Future:** #2466 (L1 as a query — rendering improves once Product→SubProduct and value-stream-order encodings land in instances graph, which is easier once schema/content is separated).

## Review

- Silas: drafted, 2026-04-24.
- Wren: PM review pending.
- Kade: code-impact review pending (handler list above).
- Jeff: final sign-off pending.
