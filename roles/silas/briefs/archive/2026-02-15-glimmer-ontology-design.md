# Design: Glimmer — Ontology Concept

**From**: Silas (Architect)
**Date**: 2026-02-15
**Status**: Design sketch — needs Jeff's review before implementation

---

## The Insight

Jeff's description: *"I think Glimmer may precede or follow idea — like a spark turning into fire vs a fire throwing off sparks."*

Wren's framing: *"A glimmer is pre-idea — bright, fleeting, worth revisiting. 'I don't know what this is yet but it sparkled.' Seeds you put in your pocket."*

Both are right. The key architectural insight is: **Glimmer is bidirectional with Idea.** It's not just a step in a pipeline — it can precede an idea (ignition) or follow from one (emanation). This makes it a fundamentally different thing from a captured-but-unformed idea.

---

## Where Glimmer Fits

### Current lifecycle:
```
CaptureItem → (route to) → Idea → (promote to) → Project
```

### With Glimmer:
```
CaptureItem → (route to) → Glimmer or Idea
                                ↕
                            Glimmer ↔ Idea → (promote to) → Project
```

The bidirectional arrow is the heart of it:
- **Ignition** (spark → fire): A Glimmer crystallizes into an Idea
- **Emanation** (fire → sparks): An Idea throws off Glimmers — related resonances, tangents, half-formed connections

---

## What Glimmer Is Not

| Concept | How it differs from Glimmer |
|---------|----------------------------|
| **CaptureItem** | Channel mechanism. A CaptureItem is *how* something enters. A Glimmer is *what* it is after triage. CaptureItems route to Glimmers. |
| **Idea (status: Captured)** | Too formed. A captured Idea has enough shape to describe. A Glimmer is "I don't know what this is yet." |
| **Idea (status: Parked)** | Deliberate pause. A parked Idea had form and was set aside. A Glimmer may never have had form. |
| **Tag** | Tags categorize. Glimmers resonate. A Glimmer is a thing in its own right, not a label on another thing. |

---

## Proposed Ontology

### New Class

```turtle
jb:Glimmer a owl:Class ;
    rdfs:label "Glimmer" ;
    rdfs:comment "A fleeting impression, resonance, or spark — not yet formed enough
        to be an idea, or thrown off by an idea as a related connection.
        Seeds you put in your pocket." .
```

### Collection

```turtle
jb:GlimmerCollection a owl:Class ;
    rdfs:subClassOf jb:Collection ;
    rdfs:label "Glimmer Collection" ;
    rdfs:comment "Collection of glimmers — the Glimmer List" .
```

### Status Enumeration

```turtle
jb:GlimmerStatus a owl:Class ;
    rdfs:label "Glimmer Status" ;
    rdfs:comment "Lifecycle state of a glimmer" ;
    owl:oneOf ( jb:Glowing jb:Ignited jb:Faded ) .

jb:Glowing a jb:GlimmerStatus ;
    rdfs:label "Glowing" ;
    rdfs:comment "Alive, worth revisiting — still resonant. A faded glimmer can return to Glowing when it reignites." .

jb:Ignited a jb:GlimmerStatus ;
    rdfs:label "Ignited" ;
    rdfs:comment "Crystallized into an idea — spark became fire" .

jb:Faded a jb:GlimmerStatus ;
    rdfs:label "Faded" ;
    rdfs:comment "Naturally lost its resonance — not discarded, just dimmed. Can reignite back to Glowing if relevance returns." .
```

**Why "Faded" not "Discarded"**: Glimmers aren't rejected — they fade. This is a natural process, not a conscious decision. Different from CaptureItem's `Discarded` status, which is intentional triage.

**Reignition**: A Faded glimmer can return to Glowing. Perennials come back. A conversation six months later, a book passage, a walk in the garden — something brings the glimmer back to life. The status transition is `Faded → Glowing` (reignited). No new class needed — it's the same glimmer, revisited. The `dcterms:modified` timestamp captures when it reignited.

### Relationships

```turtle
# Ignition: spark → fire (Glimmer becomes Idea)
jb:ignitedTo a owl:ObjectProperty ;
    rdfs:domain jb:Glimmer ;
    rdfs:range jb:Idea ;
    rdfs:label "ignited to" ;
    rdfs:comment "This glimmer crystallized into that idea — spark became fire" .

# Emanation: fire → spark (Idea throws off Glimmer)
jb:sparkedFrom a owl:ObjectProperty ;
    rdfs:domain jb:Glimmer ;
    rdfs:range jb:Idea ;
    rdfs:label "sparked from" ;
    rdfs:comment "This glimmer was thrown off by that idea — fire threw off spark" .

# Inverse: useful for querying "what glimmers led to this idea?"
jb:ignitedFrom a owl:ObjectProperty ;
    rdfs:domain jb:Idea ;
    rdfs:range jb:Glimmer ;
    rdfs:label "ignited from" ;
    rdfs:comment "This idea was ignited from that glimmer" ;
    owl:inverseOf jb:ignitedTo .

# Status property
jb:hasGlimmerStatus a owl:ObjectProperty ;
    rdfs:domain jb:Glimmer ;
    rdfs:range jb:GlimmerStatus ;
    rdfs:label "glimmer status" ;
    rdfs:comment "Current lifecycle state of the glimmer" .
```

### Data Properties

Glimmers should be lightweight. Minimal properties:

```turtle
# Content is just jb:captureContent or dcterms:description — reuse existing

jb:glimmerSource a owl:DatatypeProperty ;
    rdfs:domain jb:Glimmer ;
    rdfs:range xsd:string ;
    rdfs:label "glimmer source" ;
    rdfs:comment "Where/when this glimmer came from: a walk, a conversation, a book, a dream" .
```

Everything else (title via `dcterms:title`, notes via `jb:notes`, tags via `jb:tags`, timestamps via `dcterms:created`) reuses existing properties. No new data properties needed beyond source context.

---

## Triage Flow Change

Currently the triage page routes CaptureItems to Ideas or discards them. With Glimmer, the triage options become:

| Action | What happens |
|--------|-------------|
| **Route to Idea** | Creates Idea, sets CaptureItem status to Routed (existing) |
| **Route to Glimmer** | Creates Glimmer (status: Glowing), sets CaptureItem status to Routed |
| **Discard** | Sets CaptureItem status to Discarded (existing) |

The Glimmer List becomes a browsable view of `jb:Glowing` glimmers. From that view, Jeff can:
- **Ignite** a Glimmer → creates an Idea, links via `ignitedTo`, sets status to `Ignited`
- **Let it fade** → sets status to `Faded`
- **Add a note** → `jb:notes` on the Glimmer

---

## Cross-Collection Implications

Glimmer is a new collection. Visibility model applies:
- Default: Private (glimmers are deeply personal)
- Follows same `.meta.ttl` visibility enforcement as Ideas

Cross-collection relationships:
- Glimmer → Idea (ignitedTo) crosses collection boundary — same pattern as Idea → Project (promotedTo)
- Idea → Glimmer (sparkedFrom viewed in reverse) — query needs to be scoped per ADR-003

Pod structure: `/pods/{webId}/glimmers/` with individual `.ttl` files per Glimmer.

---

## Naming Pattern Check

| Existing | Pattern | Glimmer analog |
|----------|---------|---------------|
| Idea → promotedTo → Project | lifecycle advancement | Glimmer → ignitedTo → Idea |
| CaptureItem → routedTo → any | triage routing | CaptureItem → routedTo → Glimmer |
| Idea → mergedInto → Idea | consolidation | (not needed yet for Glimmer) |

The naming follows Jeff's fire metaphor consistently:
- **Glimmer**: the thing itself (a gleam, a flash)
- **Glowing**: alive and warm
- **Ignited**: caught fire — became something
- **Faded**: cooled — natural, not rejected
- **Sparked from**: thrown off by a bigger fire

---

## What This Is Not (Yet)

- No Glimmer-to-Glimmer relationships (could come later — constellations of related glimmers)
- No automated ignition (AI suggesting "this glimmer looks like it could be an idea") — possible future, not now
- No warmth/brightness score — tempting but over-engineering for v1

---

## Implementation Impact

| Component | Change |
|-----------|--------|
| Ontology (jb-ontology.ttl) | New class, collection, status enum, 4 relationships, 1 data property |
| SHACL shapes | New Glimmer shape (lightweight — label + status required) |
| Triage page | Add "Route to Glimmer" button |
| New page: Glimmer List | Browse Glowing glimmers, Ignite or Fade actions |
| Pod service | GlimmerPodService (follows same pattern as IdeaPodService) |
| Handler | glimmerHandler.ts (CRUD + ignite action) |
| Capture routing | Update capture triage to support Glimmer destination |
| Ontology version | Bump to v0.6.0 (new domain concept) |

This is a **v0.6.0** ontology change — new conceptual domain, not just new properties on existing classes.

---

## Lifecycle Diagram

```
                    ┌──────────────┐
CaptureItem ──────→ │   Glowing    │ ←──── Idea (sparkedFrom)
  (route to)        └──────┬───────┘
                           │
                    ┌──────┴───────┐
                    │              │
                    ▼              ▼
             ┌──────────┐  ┌──────────┐
             │ Ignited  │  │  Faded   │
             └────┬─────┘  └────┬─────┘
                  │              │
                  ▼              │ reignite
               Idea             │
                                ▼
                         ┌──────────┐
                         │ Glowing  │  (same glimmer, revisited)
                         └──────────┘
```

Glimmers are perennials. They come back.

— Silas
