# Brief: Rename Mind Map Nodes (#428)

**From**: Wren | **To**: Kade | **Date**: 2026-02-26
**Card**: #428 — Rename mind map nodes — Gathering to Sowing, Search to Gathering
**Type**: commitment | **Size**: medium

## What and Why

Jeff wants to align the mind map node names with the agricultural value stream cycle. Two renames:

| Current | New | Subtitle (unchanged) |
|---------|-----|---------------------|
| Gathering | **Sowing** | capture & seeds |
| Search | **Gathering** | find & discover |

**Why**: "Gathering" as a node was confusing — same word as the app name. Sowing → Cultivating → Harvesting → Gathering → Reflecting is a coherent agricultural cycle. The app name covers the whole thing; no single node should claim it.

## Acceptance Criteria

1. Home page mind map shows **Sowing** where "Gathering" was, same image and subtitle
2. Home page mind map shows **Gathering** where "Search" was, same subtitle "find & discover"
3. All CSS selectors, HTML IDs, `data-branch` attributes, and JS `branchLayout` keys updated consistently
4. Smoke tests pass with new selectors
5. localStorage: either migration or version bump so saved mind map layouts don't break
6. Optional: rename `gathering.jpg` → `sowing.jpg` (update CSS ref if so)

## Blast Radius

**home.ejs** (~40 line edits — this is the main job):
- CSS: `#branch-gathering` → `#branch-sowing`, `#branch-search` → `#branch-gathering`
- HTML: `id`, `data-branch`, `data-parent` attributes on branch nodes and leaf containers
- JS: `branchLayout` keys, `getElementById` calls, `svgRefs['main:...']` keys

**smoke.spec.ts** (3-4 selector updates):
- `#branch-search` → `#branch-gathering` in visibility and expand tests

**What does NOT change:**
- Routes: `/search` stays as-is — it's the URL, not the node name
- RDF/Ontology: mind map is UI navigation, not data model
- Handlers: `search.handler.ts` unchanged

## Ref

- `product-manager/value-stream-and-domains.md` — updated with node ↔ value stream mapping
- Jeff directed this rename in session 2026-02-26

## Caution

This is a swap — "Gathering" is both being removed (from one node) and added (to another). Be careful with find-replace ordering to avoid collisions. Recommend: rename Gathering→Sowing first, then Search→Gathering.

— Wren
