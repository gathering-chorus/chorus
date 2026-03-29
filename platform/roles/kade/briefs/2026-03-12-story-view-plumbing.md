# Wire Related Documents + Mentioned People into Story Detail View

**From:** Wren | **To:** Kade | **Card:** #1356 | **Priority:** P2 — pull while photos pipeline runs

## Context
Photos batch harvest (#1351) is processing 157 zip files through tomorrow. You don't need to babysit it. This is bounded view work you can do in parallel.

Stories now have two properties that the detail view doesn't render:
- `jb:relatedDocument` — links to transcripts and other documents
- `jb:mentionsPerson` — links to People entries

Example: "Walking the childhood land — return at 57" has both — a 27-minute voice transcript and 6 mentioned people (Julian, Dani Perea, father, mother, sister, uncle Larry). The data is in Fuseki but invisible on the page.

## AC
- [ ] Story detail view (`views/self-domain-detail.ejs` or stories detail) renders `jb:relatedDocument` as clickable links
- [ ] Story detail view renders `jb:mentionsPerson` as clickable links to `/people/<slug>`
- [ ] SPARQL query in handler pulls these properties for stories
- [ ] Works for stories that have 0 related docs / 0 mentioned people (no empty sections)

## Blast Radius
- `src/handlers/stories.handler.ts` or `self-domain.handler.ts` — SPARQL query extension
- `views/self-domain-detail.ejs` or story detail view — render new sections
- No new routes needed — links point to existing `/documents/` and `/people/` pages

## What's Already Done
- TTL files with `jb:relatedDocument` and `jb:mentionsPerson` exist and are synced to Fuseki
- 9 people already have `jb:mentionedInStory` back-links
- Wren owns enrichment data (#1270), you own the plumbing

## Why Now
This unblocks #1270 (relationship enrichment) — Wren has 38 stories synced with people mentions, but the connections are invisible until the view renders them. Small job, high leverage.
