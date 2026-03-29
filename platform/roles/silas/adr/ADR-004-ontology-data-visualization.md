l# ADR-004: Ontology and Data Visualization Tooling

**Date**: 2026-02-13
**Status**: Accepted
**Deciders**: Jeff Bridwell, Silas (Architect)

## Context

The ontology (v0.4.0) spans 7 domains with cross-collection relationships, and the pod data contains 40+ blog posts, books, property structures, ideas, and projects — all stored as Turtle files. The system is inherently graph-shaped: classes link to classes, instances link to instances, and relationships cross collection boundaries (see ADR-003 section 7).

Jeff needs to **see** this — not read triples, but understand the shape of both the model and the data. The ontology is "3-dimensional" in his words: classes, relationships, and instances form a structure that flat text doesn't reveal. This is also critical for validating the visibility enforcement work — you need to see which relationships cross boundaries.

Two distinct needs:

1. **Ontology visualization** — the schema/model level. "What classes exist? How do they relate? What's the shape of my world model?" This changes infrequently but matters when evolving the ontology.

2. **Data exploration** — the instance level. "What's actually in my pods? Show me a book and everything connected to it. Let me browse the graph." This is day-to-day — understanding what you have, finding patterns, verifying data quality.

## Options

### Ontology Visualization (the model)

**Option A: WebVOWL (Recommended)**
- Web-based OWL visualizer — classes as circles, properties as arrows, interactive zoom/filter
- Can be self-hosted as a static site or Docker container
- Load the `.ttl` ontology file directly
- Lightweight, no backend needed
- Limitation: model-level only, doesn't show instance data
- Fits the Prometheus guardrail: simple, self-contained, no magic

**Option B: Protege (Desktop)**
- The standard OWL editor — full ontology development environment
- Plugins: OntoGraf (visual graph), OWLViz (class hierarchy)
- Heavy: Java desktop app, steep learning curve for visualization
- Best for ontology *editing*, not just browsing
- Jeff may already know this from his Staples architecture days

**Option C: Graphviz/Mermaid export**
- Generate static diagrams from the ontology (automated)
- Could be a build step: ontology changes → regenerate diagram
- Lightweight, version-controllable, embeddable in docs
- Limitation: static, no interactive exploration

### Data Exploration (the instances)

**Option D: YASGUI embedded in dashboard (Recommended for querying)**
- SPARQL query editor with autocomplete, syntax highlighting, result tables
- Can render results as tables, pivot tables, or charts
- Embeddable JavaScript widget — integrates directly into the existing Express dashboard
- Already works with Fuseki's SPARQL endpoint
- Replaces the current raw SPARQL textarea with something much more usable
- Limitation: query-based, not visual graph browsing

**Option E: vis.js / d3-force graph renderer (Recommended for visual browsing)**
- JavaScript libraries for interactive force-directed graph rendering
- Feed SPARQL results → render as nodes and edges
- Could add a "visualize" button to YASGUI results or collection pages
- Integrates into the existing Express app — no new service
- Shows the actual shape of the data: "this book connects to this shelf in this room"
- Limitation: needs custom integration work, not turnkey

**Option F: Ontodia / Graph Explorer**
- Full-featured RDF visual explorer, connects to SPARQL endpoints
- Interactive: click a node, see its properties, follow relationships
- Can be embedded as a React component
- More capable than vis.js but heavier dependency
- May be more than needed for a personal system

**Option G: Neo4j + neosemantics**
- Import RDF into Neo4j via neosemantics (n10s) plugin
- Neo4j's built-in browser has excellent graph visualization
- Adds a whole new service to the stack (Java, another database)
- Powerful but violates the Prometheus guardrail — significant operational complexity for visualization
- Would NOT replace Fuseki (different query semantics), so it's additive

## Recommendation

A layered approach, lightest to heaviest, added incrementally:

### Layer 1: YASGUI in dashboard (low effort, high value)
Replace the current SPARQL textarea in the admin dashboard with YASGUI. Immediate usability improvement for querying. Zero infrastructure change — it's a JavaScript embed that talks to the existing Fuseki endpoint.

### Layer 2: WebVOWL for ontology browsing (low effort, medium value)
Self-host WebVOWL as a Docker container or static page. Load the ontology file. Use it when evolving the ontology to see the full class/property graph. Especially valuable for seeing cross-collection relationships visually (ADR-003 concern).

### Layer 3: vis.js graph renderer on collection pages (medium effort, high value)
Add an interactive graph view to collection pages or the dashboard. "Show me this book and everything connected to it" as a force-directed graph. Uses SPARQL results from Fuseki, rendered client-side. This is where the "3D" feeling comes alive — you can see the shape of your data, follow relationships, spot patterns.

### Not recommended (now)
- Neo4j: Too much operational overhead for a personal system. Revisit only if Fuseki proves insufficient for graph exploration.
- Protege: Better as an editing tool. WebVOWL is lighter for visualization.
- Ontodia: Evaluate if vis.js proves too limited, but start simpler.

## Visibility integration

Visualization tools must respect the visibility model:
- **Admin views**: Full graph, all collections, all relationships
- **Public views** (if collection pages get graph rendering): Only show nodes/edges within accessible collections. Cross-collection edges to private collections are omitted or shown as opaque references (consistent with ADR-003 section 7, opaque URI pattern).
- **YASGUI/SPARQL**: Admin-only (per ADR-003 section 6), so no visibility filtering needed.

## Consequences

- YASGUI replaces the raw SPARQL tool — better UX, same security model (admin-only)
- WebVOWL adds one lightweight container or static page — minimal operational cost
- vis.js integration requires frontend development in the Express app — Kade builds this
- All three tools work with the existing Fuseki endpoint — no new data infrastructure
- The combination covers both needs: model visualization (WebVOWL) + data exploration (YASGUI + vis.js)

## Key Files

- `src/handlers/access-dashboard.handler.ts` — current dashboard with SPARQL tool (YASGUI replaces the textarea)
- `src/ontology/jb-ontology.ttl` — loaded into WebVOWL
- `src/services/sparql.service.ts` — backing SPARQL queries for vis.js rendering
- `docker-compose.yml` or Terraform — if WebVOWL is containerized
