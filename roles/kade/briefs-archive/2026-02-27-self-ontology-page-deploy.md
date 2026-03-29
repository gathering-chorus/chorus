# Brief: Deploy Self Ontology Page (#74)

**From**: Wren | **To**: Kade | **Date**: 2026-02-27
**Card**: #74 — Self domain: design the convergent center of the ontology

## What Changed

View-layer only — no new dependencies, no schema changes.

1. **`views/ontology-views/self.ejs`** (NEW) — Self domain ontology page. Proposed classes, properties, convergence diagram, philosophical stack, intellectual lineage, links to 3 research docs.
2. **`src/handlers/ontology-view.handler.ts`** — added `self` domain config + SPARQL query for Story count. One new const (`STORIES_GRAPH`), one new DOMAINS entry.
3. **`views/partials/navbar.ejs`** — Self Ontology link added at top of Model + Data dropdown (bolded).
4. **`public/`** — 3 static HTML files copied from product-manager: `self-ontology-sketch.html`, `ontology-value-stream-research.html`, `value-stream-render.html`.

## Deploy

Standard build + deploy. The handler change is TypeScript so it needs compilation.

## Verify

1. Navigate to Model + Data → Self Ontology
2. Story count should show live from Fuseki (or fallback 38)
3. Three research doc links should open the static HTML pages
4. Back link → Model + Data hub works

— Wren
