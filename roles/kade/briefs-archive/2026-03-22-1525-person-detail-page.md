# Brief: #1525 Person Detail Page — Build from Design

**From:** Wren
**Card:** #1525
**Priority:** P1 — Jeff approved design

## Context
Design doc with wireframes at `product-manager/designs/person-detail-page.html`. Jeff approved. Three wireframes showing Dani Perea, Aubrey Haltom, and Crissy Bridwell — different relationship depths, different data richness, same template.

## What to build

1. **Route**: `GET /people/:slug` in people.handler.ts
2. **View**: `views/person-detail.ejs` (new) — follow the wireframe layout
3. **Query**: extend `people-query.service.ts` with `getPersonDetail(slug)` — federated SPARQL across people, photos, stories, social graphs
4. **Link change**: People collection page — person name click goes to `/people/:slug`, not LinkedIn

## Sections (only render if data exists)
- Identity: name, source badges, avatar (face crop or initials), relationship state badge
- Relationship depth: howWeMet, sharedContext (from #1270)
- Contact: masked phone/email from Apple Contacts
- Professional: company, title, LinkedIn link (outbound)
- Family: links to related Person pages
- Photos: thumbnail grid from face cluster linkage
- Stories: linked stories with title, excerpt, date
- Social: cross-domain linked posts
- Influence: influenceDescription text

## Key design decisions
- Empty sections hidden, not shown as "none"
- Contact info masked (last 4 digits phone, masked email)
- Relationship state: Active=green, Dormant=orange, PassedAway=gray
- Photo grid: small thumbnails, "+N more" for large sets
- Responsive: 2-column desktop, 1-column mobile

## Files
- views/person-detail.ejs (new)
- src/handlers/people.handler.ts
- src/services/people-query.service.ts
- src/routes/index.ts
- views/collection-people.ejs (link change)
