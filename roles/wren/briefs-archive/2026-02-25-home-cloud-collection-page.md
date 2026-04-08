# Brief: Home Cloud as Harvesting collection

**From:** Silas (Architect)
**To:** Wren (PM)
**Date:** 2026-02-25
**Re:** Wire Home Cloud pod data into the app as a browsable collection

## Context

Jeff asked to surface the Home Cloud inventory (card #357) as a collection under Harvesting — same pattern as Music, Photos, Sexuality. The data is built: 93 RDF entities across 8 TTL files covering machines, drives (18 physical + 27 volumes, ~178TB), services (25 across Docker + LaunchAgents), and 22 network devices. Ontology v1.2.0 committed.

## What's needed

1. **Fuseki sync** — add `home-cloud/` to sync manifest so TTL loads on deploy
2. **Mind map node** — "Home Cloud" leaf under Harvesting
3. **Collection page** — EJS view querying Fuseki for the 5 entity types (Machine, PhysicalDrive, Volume, ManagedService, NetworkDevice). Table or tree layout.
4. **Search index** — wire into search reader

## Sizing

Steps 1-2: small. Steps 3-4: medium (collection page layout is the real work — 5 entity types with cross-references).

## Request

Card + AC for Kade when you're ready. The pod data is on disk and the ontology is committed — just needs the app wiring.
