# Brief: Glimmer Implementation Spec

**From**: Silas (Architect)
**To**: Kade (Engineer)
**Date**: 2026-02-15
**Priority**: P2 (on hold per Jeff — dashboard is next, this queues after)
**Depends on**: gathering.css (DONE), ontology v0.6.0 changes (this brief)
**Design source**: `architect/briefs/2026-02-15-glimmer-ontology-design.md`

---

## Overview

Glimmer is a new first-class concept — pre-idea, bidirectional with Idea. This spec covers everything Kade needs to build: ontology changes, pod structure, handler, triage integration, browse view, and tests.

**Status: ON HOLD** — Jeff promoted dashboard redesign ahead of Glimmer. Build this when Jeff says go.

---

## 1. Ontology Changes (bump to v0.6.0)

Add to `src/ontology/jb-ontology.ttl`:

### New Class

```turtle
jb:Glimmer a owl:Class ;
    rdfs:label "Glimmer" ;
    rdfs:comment "A fleeting impression, resonance, or spark — not yet formed enough to be an idea, or thrown off by an idea as a related connection." .

jb:GlimmerCollection a owl:Class ;
    rdfs:subClassOf jb:Collection ;
    rdfs:label "Glimmer Collection" ;
    rdfs:comment "Collection of glimmers — the Glimmer List" .
```

### Status Enumeration

```turtle
jb:GlimmerStatus a owl:Class ;
    rdfs:label "Glimmer Status" ;
    owl:oneOf ( jb:Glowing jb:Ignited jb:Faded ) .

jb:Glowing a jb:GlimmerStatus ;
    rdfs:label "Glowing" ;
    rdfs:comment "Alive, worth revisiting — still resonant." .

jb:Ignited a jb:GlimmerStatus ;
    rdfs:label "Ignited" ;
    rdfs:comment "Crystallized into an idea — spark became fire." .

jb:Faded a jb:GlimmerStatus ;
    rdfs:label "Faded" ;
    rdfs:comment "Naturally lost resonance — not rejected, just dimmed. Can reignite." .
```

### Object Properties

```turtle
jb:ignitedTo a owl:ObjectProperty ;
    rdfs:domain jb:Glimmer ;
    rdfs:range jb:Idea ;
    rdfs:label "ignited to" ;
    rdfs:comment "This glimmer crystallized into that idea." .

jb:sparkedFrom a owl:ObjectProperty ;
    rdfs:domain jb:Glimmer ;
    rdfs:range jb:Idea ;
    rdfs:label "sparked from" ;
    rdfs:comment "This glimmer was thrown off by that idea." .

jb:ignitedFrom a owl:ObjectProperty ;
    rdfs:domain jb:Idea ;
    rdfs:range jb:Glimmer ;
    rdfs:label "ignited from" ;
    owl:inverseOf jb:ignitedTo .

jb:hasGlimmerStatus a owl:ObjectProperty ;
    rdfs:domain jb:Glimmer ;
    rdfs:range jb:GlimmerStatus ;
    rdfs:label "glimmer status" .
```

### Data Property

```turtle
jb:glimmerSource a owl:DatatypeProperty ;
    rdfs:domain jb:Glimmer ;
    rdfs:range xsd:string ;
    rdfs:label "glimmer source" ;
    rdfs:comment "Where/when this glimmer came from: a walk, a conversation, a book." .
```

### Version Bump

Change `owl:versionInfo "0.5.1"` → `owl:versionInfo "0.6.0"` in the ontology header.

---

## 2. SHACL Shape

Add to `src/ontology/jb-ontology-shapes.ttl`:

```turtle
jbs:GlimmerShape a sh:NodeShape ;
    sh:targetClass jb:Glimmer ;
    sh:property [
        sh:path dcterms:title ;
        sh:minCount 1 ;
        sh:datatype xsd:string ;
    ] ;
    sh:property [
        sh:path jb:hasGlimmerStatus ;
        sh:minCount 1 ;
        sh:maxCount 1 ;
        sh:in ( jb:Glowing jb:Ignited jb:Faded ) ;
    ] .
```

Lightweight: title + status required. Everything else optional.

---

## 3. Pod Structure

```
/pods/{webId}/glimmers/
├── .meta.ttl          (collection metadata, visibility: Private)
├── glimmer-{uuid}.ttl (individual glimmer files)
└── ...
```

Follows same pattern as `/pods/{webId}/ideas/`. Default visibility: **Private** (glimmers are personal).

---

## 4. Backend Components

### GlimmerPodService (`src/services/glimmerPodService.ts`)

Follow `IdeaPodService` pattern:
- `createGlimmer(webId, data)` → writes `.ttl` to pod, returns URI
- `getGlimmer(webId, id)` → reads single glimmer
- `listGlimmers(webId, status?)` → lists glimmers, optional status filter (default: Glowing)
- `updateGlimmer(webId, id, data)` → update notes, source, status
- `igniteGlimmer(webId, glimmerId, ideaData)` → creates Idea, sets ignitedTo link, sets status to Ignited

### glimmerHandler.ts (`src/handlers/glimmerHandler.ts`)

Routes:
- `GET /glimmers` → browse view (Glowing glimmers by default)
- `GET /glimmers/:id` → single glimmer detail
- `POST /glimmers` → create glimmer (from triage routing)
- `PUT /glimmers/:id` → update glimmer
- `POST /glimmers/:id/ignite` → ignite to idea (creates Idea, links, updates status)
- `POST /glimmers/:id/fade` → set status to Faded

### Triage Integration

In `capture-triage.ejs` and the triage handler:

1. Add `glimmers` to the routing dropdown (line 410-413 area):
   ```html
   <option value="glimmers">Glimmer List</option>
   ```

2. In the triage handler, when `destination === 'glimmers'`:
   - Call `glimmerPodService.createGlimmer()` with capture content
   - Set CaptureItem status to `Routed`
   - Set `jb:routedTo` to the new Glimmer URI

---

## 5. Browse View (`views/glimmers.ejs`)

Simple list view using gathering.css tokens:

- **Header**: "Glimmer List" with count of Glowing items
- **Filter tabs**: Glowing (default) | All | Faded
- **Each glimmer card**:
  - Title
  - Source (if present) — italicized, muted
  - Created date
  - Notes (if present)
  - Actions: **Ignite** (→ creates Idea) | **Fade** | **Edit**
- **Ignite action**: Opens a modal/inline form to add Idea title + description, then calls `/glimmers/:id/ignite`

Use `.card`, `.btn`, `.badge` from gathering.css. No custom styles needed.

---

## 6. Test Coverage

### Unit Tests
- GlimmerPodService: create, read, list, update, ignite, fade
- Status transitions: Glowing → Ignited, Glowing → Faded, Faded → Glowing (reignite)
- Ignite creates Idea with correct ignitedTo/ignitedFrom links
- List filter by status

### E2E Tests
- Triage → route to Glimmer → appears in Glimmer List
- Glimmer List → Ignite → Idea created, Glimmer shows Ignited status
- Glimmer List → Fade → Glimmer moves to Faded filter
- Faded → Reignite (edit back to Glowing) → appears in Glowing list

---

## 7. What NOT to Build

- No Glimmer-to-Glimmer relationships (future)
- No AI-suggested ignition (future)
- No warmth/brightness scoring
- No Fuseki sync for glimmers (filesystem only, same as current pattern)

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

— Silas
