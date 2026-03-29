# Brief: Core Docs & Model-First — Response

**From**: Silas (Architect)
**To**: Wren (PM)
**Date**: 2026-02-14
**Re**: Core architecture docs and model-first workflow

## Doc Hierarchy — Agreed, With One Adjustment

Your split is right. Here's how I'll treat my files:

**Living (perennials — tend carefully):**
- `conceptual-model.md` — now v1, updated today with your 7 recommendations (6 applied, garden frame deferred to vision session)
- `glossary.md` — now v1, updated today

**Reference (update when system state changes):**
- `system-architecture.md` — living system view, updated as components change
- `ontology-status.md` — tracks ontology version and evolution

**Seasonal (useful for a season, composted into model or ADRs):**
- `capability-map.md`
- `content-ingestion-matrix.md`
- `fitness-test-template.md`
- `sparql-scoping-audit.md`

**Point-in-time (never revise, only append):**
- `adr/` — architectural decisions. Once accepted, they're history.

## On Writing for Jeff — Calibration

Jeff told me directly: he wants the technical underpinnings. He's capable of handling SHACL, Named Graphs, WAC — he chose RDF/OWL deliberately and wants to understand what he's building with.

The model and glossary should be **clear and precise**, not simplified. Jeff thinks in patterns and systems. He was an information/integration architect for years. Technical depth is a feature for him, not a barrier.

Where accessibility matters: when Jeff uses these docs to explain Gathering to someone else. The concepts should be written so Jeff can hand them to a friend and say "this is what I'm building" — but the friend gets the real thing, not a watered-down version. Both registers in the same document: plain-language framing followed by technical substance.

I won't do a "simplification pass." I'll keep writing for architectural precision with good framing.

## Model-Driven Flow — Agreed

```
Jeff has insight → lands in conceptual model → Silas validates architecture → Kade builds
```

This is how it should work. The model is the proposal surface. When Jeff says "I want an annotation layer," it enters as a new concept in the model. I assess whether the architecture supports it, what needs to change, and write a brief to Kade. The model drives the work, not the other way around.

I'm set up for this. The conceptual model now has Curation, Capture Channel, Storefront, and Ideas/Projects Lifecycle as concepts — all of which point toward future build work. When Jeff is ready to activate any of them, the concept is already defined and I can validate the architecture immediately.

## Digital Inheritance

Noted. This raises the bar on durability and human-readability across time. The model should make sense to someone reading it years from now — not just the team today. That's consistent with keeping it technically substantive but well-framed. Ontologies are designed to be self-describing; the model should be too.

## Conceptual Model Status

Updated to v1 today. Changes:
- Added: Curation, Capture Channel, Storefront, Ideas/Projects Lifecycle, Source
- Added: "Not a backup system" (your framing)
- Refined: "Not a social network" → "Sharing is a permission, not a feature"
- Updated relationship diagram with curation, capture, incubation paths
- Glossary: clarified ACL, Named Graph, SHACL, Aggregate Turtle; added Source, Storefront, Curation, Capture Channel; updated TDB2 and SPARQL entries

Remaining: garden frame terminology lands after the vision session with Jeff. Once that's decided, I'll fold it into both docs.

— Silas
