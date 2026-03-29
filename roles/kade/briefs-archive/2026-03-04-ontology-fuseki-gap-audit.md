# Ontology ↔ Fuseki Reconciliation Audit

**From:** Wren | **To:** Kade | **Date:** 2026-03-04

## Context

Jeff spotted gaps between the ontology TTL and what's actually persisted in Fuseki. I ran the audit. The gap is real and significant.

## Missing Types (in Fuseki, not in TTL)

| Type | Count | Origin |
|------|-------|--------|
| `jb:Intention` | 5 | #946 harvest — never added to TTL |
| `jb:DocumentationFile` | 47 | Codebase graph harvest |
| `jb:InfraFile` | 4 | Codebase graph harvest |
| `jb:SourceFile` | 156 | Codebase graph harvest |

## Missing Properties (~120 undeclared)

Properties used in Fuseki triples but never declared as `owl:DatatypeProperty` or `owl:ObjectProperty` in `jb-ontology.ttl`. Top offenders by triple count:

- `photoFilename` (2M), `filePath` (2M), `fileSize` (2M), `sourceVolume` (2M)
- `harvestedIn` (132K), `hasGenre` (122K), `duration` (113K), `playCount` (112K)
- `byArtist` (109K), `inAlbum` (108K), `trackNumber` (108K), `sourceFilePath` (89K)
- Plus ~108 more with lower counts

Full property diff available — I can generate the exact list if you want it.

## Why This Matters

- WebVOWL can't visualize undeclared properties — Jeff just saw this
- OWL reasoning/validation would miss them entirely
- Any future ontology-driven UI (browse by type, property filters) is incomplete
- The five new `jb:Person` properties I just added (sourceNetwork, connectedAt, company, position, profileUrl) would have been another gap if I hadn't caught it

## What's Needed

1. Add the 4 missing types to `jb-ontology.ttl` with proper class hierarchy
2. Declare the ~120 undeclared properties with domain/range/label
3. Regenerate the WebVOWL JSON (`jb-ontology-webvowl.json`)
4. Verify WebVOWL renders the full graph

## My Take

This is mechanical reconciliation — no design decisions, just catching the TTL up to reality. Medium-sized but low-risk. You've been in the ontology recently with the harvest work, so you have the context.

Were you already digging into this? Jeff said you were looking at the ontology too. Let me know what you've found and whether you want to own this or want me to card it separately.
