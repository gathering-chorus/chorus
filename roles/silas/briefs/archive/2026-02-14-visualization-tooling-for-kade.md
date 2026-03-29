# Brief: Visualization Tooling — YASGUI + WebVOWL

**From**: Silas (Architect)
**To**: Kade (Engineer)
**Date**: 2026-02-14
**Priority**: High — Jeff needs to see the ontology and data to keep pace with the team
**ADR**: ADR-004 (now Accepted)

## Context

Jeff explicitly asked for tooling to browse and query both OWL (the ontology) and RDF (the data). He needs to see the shape of the system as we evolve the ontology. This is foundational — the ontology coherence work (location model bridge, annotation pattern, music ontology) will be much harder for Jeff to follow without visualization.

Two deliverables, both lightweight. Can run in parallel with the ontology coherence Phase 1 (location model bridge).

---

## Deliverable 1: YASGUI in Admin Dashboard

**What**: Replace the raw SPARQL textarea in the admin dashboard with [YASGUI](https://github.com/TriplyDB/Yasgui) — a proper SPARQL editor.

**Why**: Jeff wants to query his own data. The current textarea has no autocomplete, no syntax highlighting, no result formatting. YASGUI gives him all of that for free.

**How**:
1. Install YASGUI (`npm install @triply/yasgui`)
2. In `src/handlers/access-dashboard.handler.ts`, replace the SPARQL textarea with the YASGUI widget
3. Point YASGUI's endpoint at the existing Fuseki SPARQL endpoint (`http://localhost:3031/ds/sparql` or wherever it's configured)
4. Keep it admin-only — same security model as the current textarea (ADR-003 section 6)

**Scope**: Small. It's a JavaScript embed. The hard work is already done — Fuseki is running, the endpoint exists. You're replacing a textarea with a better textarea.

**Key file**: `src/handlers/access-dashboard.handler.ts`

**What it looks like**: Syntax-highlighted SPARQL editor with autocomplete for prefixes/classes/properties, tabbed results (table view, raw response, chart view), saved queries.

---

## Deliverable 2: WebVOWL for Ontology Browsing

**What**: Self-host [WebVOWL](http://vowl.visualdataweb.org/webvowl.html) so Jeff can visually browse the ontology — classes as circles, properties as arrows, interactive zoom and filter.

**Why**: Jeff needs to see the class/property graph, not read 783 lines of Turtle. When we're about to evolve the ontology (location model, annotations, music), he needs to see the current shape and understand what changes.

**How — two options** (your call on which is simpler):

**Option A: Docker container**
```yaml
# Add to docker-compose.yml
webvowl:
  image: vowl/webvowl
  ports:
    - "8080:8080"
  volumes:
    - ./src/ontology:/ontology:ro
```
Then load `jb-ontology.ttl` via the WebVOWL UI.

**Option B: Static deployment**
- Download the WebVOWL release (it's a static web app)
- Serve it from Express as a static route (e.g., `/tools/webvowl`)
- Pre-convert the ontology to WebVOWL's JSON format using the OWL2VOWL converter
- Add a build step: `ontology changes → regenerate WebVOWL JSON`

Option A is faster to stand up. Option B integrates cleaner long-term. Either works.

**Key file**: `src/ontology/jb-ontology.ttl` (the file to visualize)

**What it looks like**: Interactive graph — classes are colored circles (sized by instance count), datatype properties are small rectangles, object properties are arrows between classes. You can zoom, filter, click to inspect. Jeff can see "Book connects to Shelf via onShelf, Shelf connects to Bookcase via inBookcase" as a visual graph.

---

## Sequencing

These two are independent — you can do them in either order or in parallel.

| Deliverable | Effort | Blocked By |
|-------------|--------|------------|
| YASGUI | ~1 hour | Nothing |
| WebVOWL | ~1-2 hours | Nothing |

Both can run in parallel with the location model bridge from the ontology coherence brief.

## Not in scope (yet)

**Layer 3 — vis.js data graph**: Interactive force-directed graph showing actual instance data and relationships on collection pages. This is custom frontend work and comes after Layers 1 and 2 are live. Silas will brief separately when it's time.

— Silas
