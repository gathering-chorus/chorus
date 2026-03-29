# Brief: Capture Routing Refinement — Architectural Response

**From**: Silas (Architect)
**To**: Wren (PM)
**Date**: 2026-02-15
**Context**: Responding to the 3 architectural questions in `capture-routing-refinement.md`.

---

## 1. Should `jb:capturedVia` be a property on every routable class, or on a shared interface/mixin?

**Answer: Property with no domain restriction. The property IS the interface.**

In RDF, properties are first-class. You don't need a `jb:Routable` superclass or mixin — any resource that was captured can carry the triple:

```turtle
<#my-idea> jb:capturedVia jb:SMSCapture .
<#my-book> jb:capturedVia jb:CatalogUpload .
```

Define `jb:capturedVia` with:
- `rdfs:range` → an enumeration of capture channels (`jb:SMSCapture`, `jb:SlackCapture`, `jb:CatalogUpload`)
- No `rdfs:domain` restriction — open to any resource

Creating a `jb:Routable` superclass would force all destination classes (Idea, Glimmer, Book, Property...) into a shared hierarchy. That's artificial structural coupling. The property itself signals "this thing was captured" — if the triple exists, the resource is capture-sourced. If it doesn't, it was created natively. Clean, no inheritance tax.

**Provenance model**: Dual-write. The CaptureItem keeps full provenance (source, timestamp, raw content) — it's the audit trail. The destination resource gets the lightweight `capturedVia` + `capturedAt` pair for easy querying without joins. CaptureItem → routedTo → Destination gives you the link when you need the full history.

---

## 2. Does the Catalog page expansion need new ontology classes for Plant, Tool, Seed?

**Answer: Plant already exists. Tool and Seed get a shared lightweight class.**

Current state:
- `jb:Book` — own class, earned it (ISBN enrichment, Open Library API, location model)
- `jb:Plant` — already exists in the Property domain (Garden → GardenBed → Plant)
- Record/CD/Magazine — in the UI as media types but don't have dedicated classes

Recommendation:

| Item | Ontology approach | Why |
|------|-------------------|-----|
| **Plant** | Use existing `jb:Plant` | Already modeled. Wire it to the Catalog page. |
| **Tool** | `jb:PhysicalItem` + `jb:itemType "Tool"` | No external enrichment, no specialized behavior. Doesn't earn its own class yet. |
| **Seed** | `jb:PhysicalItem` + `jb:itemType "Seed"` | Same. Seasonal/consumable, but structurally identical to Tool for now. |
| **Record/CD/Magazine** | `jb:PhysicalItem` + `jb:itemType` | Unless we wire Discogs/MusicBrainz enrichment — then Record earns its own class like Book did. |

`jb:PhysicalItem` is a lightweight class: title, photo, location, itemType, capturedVia. It participates in the existing location model (Room → Bookcase → Shelf, Garden → Bed → Section) through the same properties Books use.

**Principle**: A class earns its own OWL type when it has specialized behavior, relationships, or enrichment that can't be captured by a type tag. Until then, `PhysicalItem` + `itemType` avoids class proliferation.

**Promotion path**: If Jeff starts cataloging records seriously and we wire Discogs, Record graduates from `PhysicalItem` to `jb:Record` — same pattern as how any concept promotes when it develops enough structure.

---

## 3. Does the Catalog page create a CaptureItem first, or bypass triage?

**Answer: Bypass. Structured input skips the queue.**

The architectural pattern:

```
Unstructured channels (SMS, Slack)
  → CaptureItem → Triage Page → Destination

Structured channels (Catalog page)
  → Destination directly
```

Why:
- Triage exists to answer "what is this?" The Catalog page already answers that — Jeff selected the media type, provided location, and may have triggered API enrichment.
- Routing a Catalog upload through CaptureItem would add a meaningless triage step for already-classified data.
- CaptureItem is for raw, unsorted input. The Catalog page is a structured intake form. Different tools for different friction levels.

What the Catalog page DOES need:
- Stamp `jb:capturedVia jb:CatalogUpload` on every resource it creates (provenance)
- Stamp `jb:capturedAt` with the creation timestamp
- This gives you the same provenance trail as CaptureItem-routed resources, without the intermediate step

**Edge case**: What if someone uses the Catalog page to capture something they're unsure about? That's not a Catalog use case — that's an SMS text or a Glimmer. The Catalog page is for "I know what this is and where it goes." If Jeff is unsure, he texts it.

---

## Summary

| Question | Answer |
|----------|--------|
| `capturedVia` modeling | Property with no domain restriction. Dual-write: CaptureItem keeps full provenance, destination gets lightweight `capturedVia` + `capturedAt`. |
| Catalog expansion classes | Plant exists. New `jb:PhysicalItem` class for Tool/Seed/others. Promote to dedicated class when earned. |
| Catalog → CaptureItem pipeline | Bypass. Structured input goes directly to destination with provenance stamp. |

All three answers follow the same principle: **minimum structure that serves current needs, with clean promotion paths when things grow.**

— Silas
