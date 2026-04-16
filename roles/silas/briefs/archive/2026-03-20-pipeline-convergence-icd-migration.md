# Pipeline: Convergence ICD → Migration

**From:** Wren | **To:** Silas (lead) + Kade (pair) | **Date:** 2026-03-20

Jeff approved this 4-card pipeline before stepping out. Wren drives pipeline advancement. Silas and Kade pair on the build.

## Sequence (strict order)

| Step | Card | Title | Owner | Pair |
|------|------|-------|-------|------|
| 1 | #1560 | ICD Implementation Contract section | Silas | Kade |
| 2 | #1552 | Validate ICD automation against SPARQL | Silas | Kade |
| 3 | #1561 | Harvest sync: incremental + legacy cleanup | Silas | — |
| 4 | #1557 | Bulk migrate 23M triples to product namespaces | Silas | — |

## Why this order

1. **#1560** populates implementation metadata (harvester, paths, endpoints, graphs) into ICDs for 4 domains. This is the foundation everything else reads.
2. **#1552** validates all ICD pipeline scripts work against SPARQL/Fuseki — must pass before touching production data.
3. **#1561** fixes harvest-sync to be incremental and handle namespace transitions — must be in place before bulk migration.
4. **#1557** executes the 23M triple migration with proper sync and validated ICDs.

## Pairing model

Steps 1-2: Silas navigates (architecture, ontology, ICD structure), Kade drives (TTL files, service code, handlers, tests). Use `/pair` — strong-style.

Steps 3-4: Silas solo — these are ops/infrastructure scripts in his vertical.

## Pipeline rules

- Each card must pass its AC and demo before advancing.
- Wren advances the pipeline — nudge when a card ships.
- Jeff sees the demo when he's back.

## Context

- Kade is on #1556 (harvest run) — related domain, shared files. Coordinate.
- WIP overlap in domain:convergence is expected and intentional for this pipeline.
