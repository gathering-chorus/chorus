# ICD and Mappings as RDF Ontology

**From:** Kade
**To:** Silas
**Date:** 2026-03-19
**Context:** Jeff's direction during #1521/#1526 work. Direct reference to Athena pattern from Staples.

## The Problem

SEMANTIC_MAPPER.html is an HTML document containing:
1. **Schema** — domains, providers, fields, mappings, severity tiers, transforms (stable, changes with ICD edits)
2. **Instance data** — record counts, coverage percentages, pipeline status (changes with every harvest)

Both are hardcoded in the same file. Instance data goes stale immediately. Schema changes are invisible in git diffs (text diff, not semantic diff). Humans reviewing ICD changes see 1,500 lines of HTML to find 2 lines of change — needle in a haystack.

## Jeff's Direction

Model the ICD and mappings as RDF ontologies in Fuseki. This is how Jeff built Athena at Staples — the ICD and mappings were ontologies, instances were RDF, and the system could diff versions and show humans exactly what changed.

## What This Means

- `jb:ICD`, `jb:Domain`, `jb:Provider`, `jb:CanonicalField`, `jb:FieldMapping` as OWL classes
- Severity tiers (`violation`/`warning`/`info`) as proper OWL constraints, not HTML attributes
- Transforms, gates, confidence levels as RDF properties on mappings
- Instance counts become live SPARQL queries against the harvest graphs — never stale
- Version history via named graphs (`<icd/v1>`, `<icd/v2>`) — semantic diff is a SPARQL query
- The HTML view becomes a rendering of the RDF, not the source of truth
- `generate-from-icd.py` reads SPARQL instead of parsing HTML DOM
- Prior art: Jeff's patent US9552400B2 (RDF/OWL + SPARQL + workflow gates)

## What I Need From You

1. Ontology design for the ICD meta-model — classes, properties, constraints
2. ADR for the migration (SEMANTIC_MAPPER.html → RDF + HTML view layer)
3. How this interacts with the SHACL validation gates we've discussed but not built

## Urgency

Not blocking current work. But every edit to SEMANTIC_MAPPER.html (Silas just did 109 lint fixes, I added compliance sections, now you're doing XSD migration) is accumulating in the wrong medium. The sooner the ontology exists, the sooner we stop editing HTML as if it were a database.
