# ADR-033: Crawler Decomposition — the harvest layer as declared instances

**Status:** Accepted — 2026-06-13 (Silas, SA + ops owner DEC-022). The decision was made and acted on (#3069 / #3071, 2026-06-02) but never written as an ADR file until now; the 2026-06-13 coherence audit (#3393) found the number referenced by 8 design-doc citations with no backing file. This ADR lands the decision it always named.
**Deciders:** Jeff Bridwell, Silas
**Context cards:** #3069 (the decomposition), #3071 (verified as-is dependency map), #3185 (harvest-layer synthesis), #3066 (the 35–55s graph-rdf join cost)
**Design of record:** `designing/docs/crawler-service-design.html` (current-state), `designing/docs/crawler-instance-model.html`, `designing/docs/crawler-dependency-map.html`. Prior art: `roles/silas/architecture/harvest-layer.md`.

## Context

The crawler had grown into a monolith that coupled three things with nothing in common:

- **Cadence** — code/tests change constantly (file-watch frequent); the ontology rarely; the RDF/Fuseki join is expensive *every time* (35–55s, #3066); embeds are async.
- **Write-home** — different facets belong in different stores (knowledge → the graph; operational state → the working graph; logs → Loki).
- **Cost** — cheap interval scans vs the heavy rare join.

Coupling them produced two recurring failures: the **all-day spine storm** (the heavy join dragged into the frequent path) and the **overnight-sleep false-alarm** (a fixed `StartInterval` fire missed while the host slept). The crawler and Borg were treated as separate systems when they are the same pattern: a per-facet herald that scans its own source and writes to the store that owns that data.

## Decision

**Decompose the monolith into a `crawlers` domain of declared instances** — a registry parallel to skills, run by a harvest engine that executes each registered instance per its declaration. The crawler is the system's **context-harvest layer (Borg v2)**: it turns filesystem and operational reality into queryable structure in the graph. "Everything" is both halves of the system — **structural context** (code, config, data, ontology) and **operational context** (disk, network, compute, alerts/monitors, logs).

Each declared instance carries three properties:

1. **Own cadence** — matched to how fast its source changes × its cost. Code/tests on file-watch (frequent, cheap); the graph-rdf join hourly/rare (heavy); embed async.
2. **Right home** — persist once, to the store that owns that data (knowledge → the graph), never flattened into a search blob.
3. **Pull, not push** — read on demand for a consumer that needs it, not constantly on spec for output nobody reads.

### Facet / herald registry

| Facet (herald) | Source | Half | Write-home | Cadence | Cost | Lane |
|---|---|---|---|---|---|---|
| code / tests | git / fs | structural | graph (`chorus:File` / `chorus:Test`) | file-watch / frequent | cheap | Silas harvest / Wren graph |
| config | .mcp.json, plists, settings.json, ICDs, declarations | structural | graph | file-watch | cheap | shared |
| data | graph, cards, messages, memory | structural | graph / knowledge | on-mutation | cheap | Wren (knowledge) |
| graph-rdf join | Fuseki | structural | graph | rare / hourly | heavy (35–55s, #3066) | Wren (knowledge) |
| disk / storage | df / volumes | operational | graph (working) | interval | cheap | Silas / Borg |
| network | two machines, ports, reachability | operational | graph (working) | interval | cheap | Silas / Borg |
| compute | CPU / load / processes | operational | graph (working) | interval | cheap | Silas / Borg |
| alerts / monitors | borg alert rules + firings | operational | graph (working) | on-change | cheap | Silas / Borg |
| logs | Loki | operational | — (Loki IS the store; pull-on-demand) | — | cheap | Silas / Borg |

## Consequences

- **Cheap-frequent / heavy-rare separation.** The cheap facets run often; the 35–55s join runs rarely. The two are never in the same path.
- **No storm by construction.** The heavy chunk is structurally never in the frequent path — the all-day spine storm becomes impossible by construction, not by tuning.
- **No fixed wall-clock to miss.** Own-cadence / on-change heralds have no single `StartInterval` fire to miss during host sleep, so the overnight-sleep false-alarm class is removed.
- **Idempotent, bounded, pull-not-push** facets satisfy `principle-services-reliable-bounded-idempotent`; per-domain harvest is the unit (`practice-domain-first`); per-domain query surfaces (`practice-api-first`).
- **DEC-095 (ICD gate) already governs harvest facets** — they are harvesters, so no facet ships without a matching ICD provider section.

## Ownership + build order

- **Silas** — the operational facets (disk / network / compute / alerts / monitors / logs) + this decomposition decision + the persistence write-path.
- **Wren** — the structural/graph data facet + the graph/ontology contract (`chorus-core.ttl`) + #3184 the semantic domain tagger.
- **Kade** — werk-tests, the downstream consumer/verb (and likely the generator/codegen build). Not buildable until the substrate exists.

Build order is forced by the dependency, not preference: **OWL + persistence first** (the harvest has nowhere structured to land until the schema + write-path exist) → **instance registry** → **prove one facet (tests)** end-to-end (harvest → tag → consume) → **#3184** (the semantic half) → **more facets** (code, then an operational one).

## Not in scope

- **Cross-machine harvest** — the harvest engine runs on Library; operational facets describe both machines but the engine is single-machine.
- **Embed-worker rework** — post-index Lance enrichment is its own concern; the harvest hands off via the messages table without coupling.
- **Athena's own design** — the harvest writes *into* the graph; how Athena renders is Wren's design.

## Note on the number (#3393)

ADR-033 was briefly referenced by an earlier `werk-subproduct-design` draft as "the werk pipeline model." That was a labeling error — the pipeline model was never written under this number and lives in **ADR-037** (atomic verb execution) + ADR-036 + ADR-032. ADR-033 is, and always semantically was, the crawler-decomposition decision (8 of its 10 design-doc citations already meant this). The 2026-06-13 hygiene pass re-pointed the 2 pipeline references to ADR-037 and landed this file so the remaining 8 references resolve correctly.
