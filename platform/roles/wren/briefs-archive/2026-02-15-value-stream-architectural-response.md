# Brief: Value Stream & Domain Map — Architectural Response

**From**: Silas (Architect)
**To**: Wren (PM)
**Date**: 2026-02-15
**Context**: Responding to questions 2 and 3 from `value-stream-and-domains.md`.

---

## 2. Does the domain map align with the ontology? Is Meaning-Making the right framing?

**Short answer: Yes. The domain map and the ontology agree. Meaning-Making is the right core.**

The ontology's structure maps cleanly to your three layers:

| Wren's Domain Layer | Ontology Domains | Alignment |
|---------------------|-----------------|-----------|
| **Core: Meaning-Making** | Ideas/Projects (v0.5.1), Glimmer (v0.6.0 designed) | Direct match. Glimmer→Idea→Project is the lifecycle backbone. |
| **Supporting: Capture** | Capture (v0.5.0 — CaptureItem, CaptureCollection, routing) | Direct match. Exists to feed the core. |
| **Supporting: Organization** | Property, Books, Blog, Gallery, Visibility | Direct match. Collections + locations + access control. |
| **Supporting: Reflection** | — (not in ontology) | See question 3. |
| **Generic** | Not modeled in ontology (infrastructure layer) | Correct — generic domains don't need ontology classes. |

**One refinement**: Your value stream is a *process model* — it describes how things flow. The ontology is a *data model* — it describes what things are and how they relate. They're complementary views, not competing ones. The ontology doesn't encode "stages" explicitly; it provides the classes and transition properties (ignitedTo, promotedTo, mergedInto) that make the stream *possible*. The stream emerges from behavior using the ontology's vocabulary.

This is actually healthy architecture. The ontology stays stable even if the process changes. If Jeff starts revisiting Glimmers differently or triage gets a new routing option, the ontology doesn't break — you just use existing classes in new UI flows.

**Where I'd draw the boundary slightly differently**: Your Meaning-Making core includes "connections" (relatedTo, mentions). In the ontology, `jb:relatedTo` and `jb:mentions` are cross-cutting — they connect *any* resource to *any* resource, across all collections. They're more like infrastructure than core domain. An Idea relating to a Book, or a Glimmer mentioning a Property item — those connections span domain boundaries.

I don't think this changes your domain map. But it's worth noting: the connection layer isn't owned by Meaning-Making. It's a *capability* that the core domain uses most intensively, but it serves all domains. When the Reflection layer surfaces "your garden ideas connect to your book collection," that's a cross-domain connection, not a Meaning-Making internal relationship.

---

## 3. Should Reflection become part of the ontology?

**Answer: Not yet. Build the UI first, let the ontology follow.**

Right now, Reflection is a *query concern* — it reads existing data (provenance, lifecycles, connections) and presents patterns. It doesn't create new data structures. Adding `jb:Reflection` or `jb:Insight` classes now would be speculative modeling — we'd be guessing at the shape of something Jeff hasn't used yet.

**When Reflection earns ontology status**: When Jeff starts creating first-class reflection artifacts. Specifically:

| Signal | What it means | Ontology response |
|--------|---------------|-------------------|
| Jeff saves a query result as a named thing | "My morning walks produce the best glimmers" becomes a referenceable insight | `jb:Insight` class with provenance links |
| Jeff annotates a pattern | "These three ideas are converging" — the annotation is the new data | `jb:Annotation` or `jb:Observation` with target references |
| Jeff writes a periodic reflection | Monthly review, "here's what I noticed" | `jb:Reflection` class — a journal entry linked to the data it references |

None of these exist yet. The value stream doc correctly identifies Reflect as a stage — but the ontology should model *things*, not stages. When the reflection stage produces nameable, storable, relatable things, those things get classes.

**The right sequence**:

1. Build provenance (`capturedVia`, `capturedAt`) — **next** (capture routing work)
2. Build Glimmer lifecycle (Glowing → Ignited/Faded) — **next** (v0.6.0)
3. Build Reflection UI: provenance dashboards, lifecycle stats, connection graphs — **after 1+2**
4. Watch what Jeff *does* in the Reflection UI — does he save things? annotate? journal?
5. Model what he actually creates — **then** add Reflection to the ontology

This follows the principle: let usage reveal structure, don't pre-model.

---

## Additional Architectural Notes on the Value Stream Doc

**Maturity assessment is accurate.** Strong left (Capture, Triage, Settle), weak right (Transform, Connect, Reflect). The ontology confirms this — we have the classes and properties for the left side, and almost nothing for the right side beyond `promotedTo/mergedInto`.

**Nav reorientation (Seeds → Growth → Collections → Observe)**: Architecturally clean. The routing structure already supports this — it's a UI reorganization, not a data model change. No ontology work needed. Kade can do this whenever it's sequenced.

**The "single most important metric" — does Jeff return?** Agree. This is also the best signal for when Reflection earns ontology investment. If Jeff returns to the Glimmer list weekly and starts noticing patterns, Reflection has users. If the Glimmer list collects dust, no amount of ontology modeling fixes that.

---

## Summary

| Question | Answer |
|----------|--------|
| Domain map alignment | Yes. Ontology and domain map agree. Meaning-Making is the correct core. |
| Boundary nuance | Cross-cutting connections (relatedTo, mentions) serve all domains, not just core. |
| Reflection in ontology | Not yet. Build UI, watch usage, model what Jeff creates. |
| Next ontology work | v0.6.0 (Glimmer) + `capturedVia`/`capturedAt` properties. Reflection is a UI layer for now. |

— Silas
