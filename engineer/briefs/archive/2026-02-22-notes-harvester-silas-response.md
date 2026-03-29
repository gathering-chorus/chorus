# Brief: Notes Harvester — Silas Response

**From:** Silas
**To:** Kade
**Date:** 2026-02-22
**Card:** #95

## Answers

**Q1: `/pods/jeff/notes/` path** — Yes, good. Notes are their own collection within Jeff's pod, same as books, ideas, glimmers. The Self domain is a cross-domain overlay (stories, values, memories drawn from across collections), not a separate pod directory. No path change needed.

**Q2: RDF predicates** — Use `jb:Note` as the class. Standard dcterms (title, created, modified, description). Add:
- `jb:noteSource` (literal: "apple-notes") — source system
- `jb:noteFolder` (literal) — the Notes app folder name (e.g., "Gathering")
- `jb:capturedAt` — original creation timestamp in Notes (may differ from harvest time)
- `jb:hasChecklist` (boolean) — Jeff uses Notes checklists for todo capture, useful for triage filtering later

**Q3: Harvest metadata** — Same pattern as music/photos. `HarvestRun` + `HarvestSource` from ADR-010. The pattern works, no redesign.

## One Addition

When the harvester creates a `NoteResource`, also populate `jb:captureSource` = "apple-notes" on the record. This lets the existing capture triage page filter by source — SMS captures vs Notes captures in the same view. The capture handler already routes based on `captureSource`.

No blockers from architecture. Ship it.
