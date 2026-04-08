# Brief: Ontology Coherence — Next Architecture Priority

**From**: Silas (Architect)
**To**: Wren (PM), Kade (Engineer)
**Date**: 2026-02-14
**Priority**: High — foundational before new collections or harvesters
**Context**: Foundation work is complete (ADR-003, backup, CI, SPARQL audit, TDB2). The conceptual model is at v1. The next architectural priority is making sure the ontology is coherent and ready to grow.

## What the Audit Found

I audited the full ontology (v0.4.0, 783 lines) against the pod data and the five-domain vision. Three gaps surfaced, in priority order.

---

## Gap 1: Location Model Disconnect (Books ↔ Property)

**The problem**: Two incompatible location models coexist in the ontology.

Books store location as flat strings:
```turtle
jb:locationRoom "Library" ;
jb:locationBookcase "South" ;
jb:locationShelf 1 .
```

The property domain has a rich object graph:
```turtle
Room → inHouse → House → onProperty → Property
Bookcase → inRoom → Room
Shelf → inBookcase → Bookcase
```

These don't connect. You can't query "all books in this room" because books reference "Library" as a string, not as the `jb:Room` instance that exists in the property collection. The object properties (`jb:onShelf`, `jb:inBookcase`, `jb:inRoom`) are defined in the ontology but never used on books.

**Why it matters**: This is the first real cross-collection relationship — books on shelves in rooms in a house. It's concrete, queryable, and it proves the pattern for connecting domains. If we can't link a book to a shelf, linking a photo to a location or a song to a memory is the same problem at larger scale.

**Recommended fix**:
1. Create Bookcase and Shelf instances for the existing locations (Library/South/Shelf 1, etc.)
2. Add `jb:onShelf` links to existing books, pointing to Shelf instances
3. Keep the flat strings for now (backwards compatible) — mark as deprecated, remove in a future pass
4. Result: books connect to the property graph via the object model

**Scope**: Small. 19 books, all on the same shelf currently. A handful of Shelf/Bookcase instances to create. Kade could do this in a session.

**What it unlocks**:
- "Show me all books in the Library" — a real SPARQL query across collections
- The pattern for every future cross-collection link
- Proof that the ontology's object model works end-to-end

---

## Gap 2: Annotation Model (Feeling Domain)

**The problem**: Personal metadata is rich on books and sparse everywhere else.

Books have:
- Reading status lifecycle (ToRead → Reading → Read → Abandoned)
- Personal rating (1-5)
- Notes (free text)
- Acquisition date and source
- Start/finish dates

Ideas have: status + summary. That's it.
Projects have: status. That's it.
Property/Garden: notes on garden beds, nothing on rooms or the house.
Blog posts: nothing personal (no rating, no "why I wrote this").
Plants: no observation notes, growth tracking, pest notes.

**Why it matters**: This is the "Feeling" domain from Wren's vision synthesis — it's what makes the system an extended mind instead of a catalog. A catalog knows *what* Jeff has. A semantic memory layer knows *why it matters*. The book model shows the right pattern; other collections don't follow it.

**Recommended approach**: Not a massive expansion — a lightweight, consistent annotation pattern applied across collections:

1. **Define `jb:Annotation` as a reusable pattern**, not a new class — a set of properties any resource can have:
   - `jb:personalRating` (1-5, already exists on Book — extend domain to all resources)
   - `jb:notes` (already exists — extend domain explicitly)
   - `jb:significance` (why this matters — new, optional, free text)
   - `jb:addedAt` (already exists — ensure consistent across all types)

2. **Per-collection enrichment** stays per-collection. Reading status belongs on books. Idea status belongs on ideas. Garden observations belong on plants. These aren't generic — they're domain-specific lifecycle properties.

3. **Don't over-model**. Jeff should be able to rate any resource and add a note to anything. That's the minimum. Structured temporal annotations (mood over time, confidence tracking) are future work — the conceptual model has the concept, but the ontology doesn't need it yet.

**Scope**: Ontology change (extend property domains) + minor UI work (add rating/notes to collections that don't have them). Not large.

**What it unlocks**:
- "Show me everything Jeff rated 5 stars" — cross-collection query
- The AI companion can reason over personal meaning, not just facts
- L2 enrichment has a consistent pattern to follow for any collection

---

## Gap 3: Music Collection Ontology (First Harvester Prep)

**The problem**: Music collection classes exist as stubs with no content model.

```turtle
jb:MusicCollection a owl:Class ;
    rdfs:subClassOf jb:Collection .
# That's it. No Album, Track, Artist, or any properties.
```

The same is true for ImageCollection and MovieCollection — stubs only.

**Why it matters**: The first external harvester (Wren recommended music — 5k albums, Pattern A, manageable scale) needs an ontology to map into. Before Kade can build a Spotify/Apple Music adapter, the ontology needs Album, Track, and their properties defined.

**Recommended approach**: Follow the Book pattern — dual typing with schema.org:

```
jb:Album   → rdfs:subClassOf schema:MusicAlbum
jb:Track   → rdfs:subClassOf schema:MusicRecording
jb:Artist  → rdfs:subClassOf schema:MusicGroup (or schema:Person)
```

Properties to define:
- **Album**: title, artist, year, genre, trackCount, albumArt, spotifyId/appleMusicId, format (vinyl/CD/digital)
- **Track**: title, artist, album (object → jb:Album), duration, trackNumber
- **Personal**: personalRating, notes, acquisitionDate, acquisitionSource, physicalLocation (if physical media)

This follows the same pattern as books: schema.org for interop, jb: for personal/domain-specific, flat strings for physical location (with object model as a future upgrade).

**Scope**: Ontology design (~30-40 new triples) + SHACL instance shapes for Album/Track. No code until the harvester build.

**What it unlocks**: Kade can build the music harvester with a clear target schema. Jeff's 5k+ albums have a home in the graph.

---

## Recommended Sequence

| Phase | Work | Who | Blocked By |
|-------|------|-----|------------|
| 1 | **Location model bridge** — connect books to property graph | Kade | Nothing — ready now |
| 2 | **Annotation pattern** — extend rating/notes to all collections | Kade (ontology + UI) | Silas designs pattern |
| 3 | **Music ontology** — Album/Track/Artist classes and properties | Silas (design) → Kade (implement) | Jeff's ingestion depth decision for music |

Phase 1 is the quick win — proves cross-collection linking works end-to-end. Phase 2 enables the "Feeling" domain. Phase 3 prepares for the first harvester.

All three are ontology-first work: change the model, then build on it. This follows the model-driven workflow Jeff and Wren agreed on.

---

## For Wren

This sequence aligns with the vision synthesis:
- Phase 1 proves **Connecting** (cross-domain relationships actually work, not just defined)
- Phase 2 enables **Feeling** (personal annotations across all collections)
- Phase 3 enables **Collecting** (first new harvest bed)

The garden frame: we're strengthening the root system (Phase 1), preparing the soil for richer growth (Phase 2), then planting a new bed (Phase 3).

## For Kade

Phase 1 is ready for you now — no design dependency. You've seen the book data (you built the middleware that reads `.meta.ttl`). The task is:
1. Create Shelf/Bookcase instances in the property collection (or a shared locations container)
2. Add `jb:onShelf` triples to existing book resources
3. Verify the cross-collection query works in Fuseki

I'll have the annotation pattern design (Phase 2) and the music ontology design (Phase 3) ready by the time you finish Phase 1.

— Silas
