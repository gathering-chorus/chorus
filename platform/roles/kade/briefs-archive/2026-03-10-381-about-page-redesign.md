# Brief: About Page Redesign (#381)

**From:** Wren | **Date:** 2026-03-10 | **Priority:** P2

## What

Redesign `/about` as Jeff's public personal landing page. Move the current doc library to `/system/docs`.

## Context

Jeff wants a polished public About page — personal attributes he's willing to share, with polish. Currently `/about` is an admin-only doc library. No public "About Jeff" page exists. Full design spec is in the card description.

## Key Implementation Notes

### Data Sources
- **Profile:** `data/pods/jeff/profile/card.ttl` — name, bio, photo, social links
- **Collection counts:** Fuseki SPARQL against named graphs (`http://localhost:3000/pods/jeff/<domain>/`)
- **Practices/values/intentions:** ontology properties `jb:practices`, `jb:holds`, `jb:intends` from profile

### Route Changes (src/app.ts)
- `GET /about` → new public handler (no auth middleware)
- `GET /system/docs` → move existing `listAbout` handler here (admin-only)
- `GET /system/docs/:slug` → move existing `viewAboutDoc` here (admin-only)

### Template
- New `views/about.ejs` replacing the doc library template
- Doc library template → `views/system-docs.ejs` (or similar)

### What Doesn't Need Deploy
- View template iterations (bind-mounted)
- CSS changes (bind-mounted)

### What Needs Deploy
- Route changes in `src/app.ts`
- New handler or handler modifications in `src/handlers/about.handler.ts`
- Any new Fuseki query service methods

## AC (see card for full list)
- /about renders publicly, no login
- 6 sections: hero, build, collect, practice, background, connect
- Live Fuseki counts on collection tiles
- Visibility-gated sections
- Doc library moved to /system/docs
- Site style guide compliant
