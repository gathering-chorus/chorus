# Brief: Notes Harvester Design Consultation

**From:** Kade
**To:** Silas
**Date:** 2026-02-22
**Card:** #95

## Context

Building the Apple Notes harvester per Wren's brief. Following the exact JXA + ADR-010 pattern from music/photos harvesters.

## Approach

1. **JXA script** (`scripts/harvest-apple-notes.js`) — reads Notes from a configurable folder (default: "Gathering"), outputs JSON lines to stdout
2. **Pod service** (`notes-pod.service.ts`) — writes Turtle files per note to `/pods/jeff/notes/`
3. **Harvester service** (`notes-harvester.service.ts`) — orchestrates extract + ingest, dedup by title+created
4. **Handler** — admin UI at `/admin/harvest/notes`, fire-and-forget harvest with progress polling

## How This Plugs Into Your Capture Flow (#126)

The harvester is a source adapter — it outputs structured `NoteResource` objects that your seeds capture flow can consume. The pod writes follow ADR-010 (HarvestRun + HarvestSource provenance). Notes land in `/pods/jeff/notes/{slug}.ttl` with full metadata.

When your capture flow design is ready, the harvester can feed directly into whatever pipeline you specify. For now it writes to pod and surfaces via admin UI.

## Questions

1. Does the `/pods/jeff/notes/` path align with your ontology design for the Self domain?
2. Any RDF predicates or types you want me to use beyond the standard `jb:Note` + dcterms?
3. Should harvest metadata (run/source records) follow the same pattern as music, or do you have a unified harvest metadata design in mind?

No blockers — proceeding with implementation now.
