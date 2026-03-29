# Conceptual Model

Last updated: 2026-02-14
Status: v1 — accepted (garden frame pending vision session with Jeff)

This is the shared conceptual model for Jeff's personal knowledge graph system. It defines the key concepts and how they relate — in plain language, not code. Everyone on the team (Jeff, Wren, Silas, Kade) should be able to read this and use the same words to mean the same things.

---

## The Big Picture

Jeff has a rich digital life spread across many services and systems — books on shelves, photos in Google and Apple, music on Spotify and CD shelves, blog posts in WordPress, ideas in his head, properties he manages. No single service sees the whole picture.

This system is a **semantic memory layer** that sits across all of it. It doesn't try to replace those services or store all that content. Instead, it harvests metadata — the memory of what Jeff has, when it mattered, and how things connect — and makes those connections visible, searchable, and meaningful.

The knowledge graph is Jeff's **extended mind on disk**.

---

## Core Concepts

### Pod
The personal data store. A pod is a structured collection of Turtle (RDF) files on the filesystem, organized into containers. Jeff owns his pod. It's the source of truth for his data — not any external service.

A pod contains: profile, collections, taxonomy, and admin data.

### Collection
A thematic grouping of items within a pod. Books, blog posts, property records, ideas, projects — each is a collection. Collections are the primary unit of organization and visibility control.

Current collections: Blog, Books, Property, Ideas, Projects. Planned: Music, Photos/Gallery.

### Resource
A single item within a collection. A book, a blog post, a garden bed, an idea. Each resource is stored as a Turtle file (or part of an aggregate Turtle file at scale). Every resource has a stable URI that persists across its lifetime.

### Ontology
The shared vocabulary that gives the data its shape. Defines what kinds of things exist (classes), what properties they have, and how they relate to each other. The ontology is architecture — changes ripple to queries, UI, and AI.

Current version: v0.4.0. Domains: Property, Books, Blog, Gallery, Profile, Ideas/Projects, Visibility.

### Relationship
A typed connection between two resources. "This book is on this shelf." "This idea became this project." "This photo was taken at this property." Relationships are what make the graph a *graph* and not just a catalog. Cross-domain relationships are where the unique value lives.

### Visibility
Every collection has a visibility level that controls who can see it.

- **Private**: Only Jeff (the owner) can see it. The default for everything.
- **Selective**: Visible to specific people Jeff chooses (not yet enforced — planned).
- **Public**: Visible to anyone on the web, no login required.

Content **graduates** from private toward public when Jeff decides it's ready. The system never pushes toward public — it protects on the way up.

### Graduation
The act of moving content from a more restricted visibility to a less restricted one. Private → Selective → Public. Graduation is always an intentional act by Jeff. The metaphor: "the workshop is not the storefront." Work stays private until it's ready to be seen.

### Harvest
The process of pulling metadata from an external source into the knowledge graph. A harvester connects to a source (Google Photos, Spotify, WordPress), maps its data to the ontology, and writes metadata into the pod. Content stays in the source — only the memory comes into the graph.

The WordPress webhook harvester is the reference implementation. Every new source follows the same pattern.

### Ingestion Depth
How much of an external source enters the knowledge graph. Four tiers:

- **L0 — Reference**: Pod knows the source exists but doesn't model individual items.
- **L1 — Catalog**: Basic metadata per item (title, date, source URI). Searchable.
- **L2 — Rich**: Full ontology modeling with personal annotations, ratings, cross-domain connections.
- **L3 — Content**: Actual content stored in the pod (blog post text, book cover images).

Most external sources target L1. L2 is added where personal meaning matters. L3 is for content Jeff authors or owns directly. Tiers are a spectrum — L1 promotes to L2 by adding triples, not by rewriting.

### Fuseki Index
The SPARQL query engine. Fuseki holds a copy of pod data in named graphs, enabling fast, structured queries across the entire knowledge graph. The filesystem Turtle files are the source of truth; Fuseki is the read-optimized index.

### Named Graph
In Fuseki, each resource's data lives in its own named graph, identified by URI. This maps directly to the pod filesystem structure. Named graphs enable scoped queries — "only search within books" — which is how visibility enforcement works at the data layer.

### Curation
The core user activity. Curation is the human act of deciding what matters, what connects, and what graduates. It sits between harvest (automated intake) and graduation (intentional publishing). Jeff reviewing a new book, annotating it with personal meaning, connecting it to an idea, deciding it's ready to share — that's curation. The AI thinking partner exists to support curation at scale, not replace it.

### Capture Channel
The intake point for raw, unstructured input. A thought, a photo, a voice note — pre-idea, pre-resource. The seed before it's planted. Today, everything enters the system through structured forms (book upload) or automated harvest (WordPress webhook). A capture channel would provide a lightweight path for Jeff's raw thinking to enter the graph before it has structure. Future work.

### Storefront
The experience an unauthenticated visitor has. What they see at the public URL. The graduation destination — where content ends up after it goes public. The system's outward face. Currently: individual public collection pages. Future: a curated front door showing public collections as a portfolio.

### Ideas and Projects Lifecycle
Ideas and projects have their own maturity lifecycle within the system:
- **Ideas**: Captured → Developing → Parked → Merged. An idea can be promoted to a project when it's ready for structured work.
- **Projects**: Active → Paused → Completed → Abandoned. Projects have outcomes; ideas have potential.
- **Promotion**: The `promotedTo` / `promotedFrom` relationship links an idea to the project it became. This is the emergence pattern — raw thinking becoming structured work.

### Source
An external system the knowledge graph harvests from. Each source has its own ingestion tier (L0-L3), adapter, and sync status. The graph doesn't replace the source or protect against losing the source — it remembers what was there and how it connects. WordPress, Google Photos, Spotify, Apple Music, Facebook, LinkedIn, and local media are all sources.

---

## How Concepts Relate

```
Jeff (Profile)
  │
  ├── owns → Pod
  │            │
  │            ├── contains → Collection (Blog, Books, Property, Ideas, Projects, ...)
  │            │                │
  │            │                ├── has visibility → Private / Selective / Public
  │            │                │                      │
  │            │                │                      └── graduates to → Storefront (public face)
  │            │                │
  │            │                └── contains → Resources
  │            │                                │
  │            │                                ├── has properties (title, date, tags, ...)
  │            │                                ├── has relationships → other Resources
  │            │                                │     (within or across collections)
  │            │                                └── has annotations → personal meaning (L2)
  │            │
  │            └── governed by → Ontology (shared vocabulary)
  │
  ├── curates ──→ (the core activity: review, annotate, connect, graduate)
  │
  ├── captures from → Capture Channel (raw input: thoughts, photos, notes)
  │                      └── triages to → Ideas / Resources / Compost
  │
  ├── harvests from → Sources (external systems)
  │                     │
  │                     ├── WordPress (L3 — full content)
  │                     ├── Google Photos (L1 — catalog metadata)
  │                     ├── Apple Photos (L1)
  │                     ├── Spotify / Apple Music (L1)
  │                     ├── Facebook / LinkedIn (L1)
  │                     └── images-api / local storage (L1 reference)
  │
  ├── incubates → Ideas (captured → developing → parked → merged)
  │                 └── promotes to → Projects (active → paused → completed → abandoned)
  │
  └── queries via → Fuseki Index (SPARQL over named graphs)
```

---

## Key Patterns

### The Harvest Pattern
External source → Adapter (handles auth + pagination) → Ontology mapper → Writer (Turtle or SPARQL) → Pod metadata + Fuseki index. Every harvester also runs link discovery: "does this new item connect to anything already in the graph?"

### The Graduation Pattern
New content starts private. Jeff explicitly promotes it: private → selective → public. The system defaults to protection. Visibility is enforced at the route level (middleware) and respected at the data level (query scoping).

### The Connection Pattern
The graph's value is in cross-domain relationships. A book connected to a shelf in a room. A photo from a trip connected to a blog post. A song connected to a memory. These connections are what make this more than a catalog — they're what make it an extended mind.

### The Concentric Circles Pattern
Jeff builds from the inside out. Each pass through data, security, features, and automation teaches something that informs the next ring. The system grows in maturity, not just scope.

---

## What This System Is Not

- **Not a content management system.** Content lives where it lives. This is a memory layer.
- **Not a social network.** Sharing is a permission, not a feature. The system optimizes for Jeff's thinking. Sharing is a side effect of the graduation model, not a goal.
- **Not a search engine.** SPARQL enables search, but the value is in structured relationships, not full-text search.
- **Not a public-first platform.** Private is the default. Public is the exception.
- **Not a backup system.** Content stays in its source. The knowledge graph doesn't protect against losing Google Photos or Spotify. It remembers what was there and how it connected — but if the source disappears, the content is gone. The graph preserves the memory, not the media.
