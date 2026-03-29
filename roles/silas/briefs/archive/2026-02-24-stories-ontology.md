# Brief: Stories Ontology — gathering:Story type

**From**: Wren | **To**: Silas | **Card**: #330 | **Workflow**: WF-057 (step 3 is your review)
**Priority**: P1 — Kade is building the collection, needs the type to exist

## What

New collection under Reflecting: Stories. Jeff's personal experiences, values, prior art — structured as browsable items, not a flat file.

## Ontology Needs

**Type**: `gathering:Story`
**Graph**: `gathering:stories`
**Parent branch**: Reflecting (same level as Notes, Journal, lifes-practice)

**Properties**:
- `gathering:title` (xsd:string) — story name
- `gathering:date` (xsd:date) — when Jeff shared it
- `gathering:period` (xsd:string) — when it happened (e.g., "1998-2002")
- `gathering:body` (xsd:string) — full text, markdown
- `gathering:theme` (xsd:string, multi) — career, prior-art, management, values, identity
- `gathering:linkedDecision` (xsd:string, multi) — DEC-NNN references
- `gathering:source` (xsd:string) — session, clearing, seed, manual

## Context

Stories replace the flat `stories.md` that's blocked by the privacy hook. They become first-class Gathering data — searchable via #318 (text ripple), browsable on the mind map. The harvest use case: Wren scans session transcripts and extracts stories into this collection.

## What I Need From You

1. Confirm the type/properties fit the existing ontology patterns
2. Add `gathering:Story` to the ontology TTL if needed
3. Ensure the Reflecting branch in the mind map data includes Stories as a leaf

Kade's brief is already delivered — he'll follow whatever schema you confirm.
