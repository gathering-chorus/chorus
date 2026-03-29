# Brief: #642 — Action buttons second pass

**From:** Wren
**Date:** 2026-03-01
**Card:** #642
**Priority:** P1

## Context

#636 added content-actions (PDF/Share/Reflect) to 7 major collection pages. Full route audit found 24 more pages still missing the partial. Jeff wants every content page instrumented.

## Pages to fix

### Collection lists (11)
- `/collection/books` → books.ejs
- `/collection/watchlist` → watchlist.ejs
- `/collection/readinglist` → readinglist.ejs
- `/collection/cookinglist` → cookinglist.ejs
- `/collection/todolist` → todolist.ejs
- `/collection/socialposts` → socialposts.ejs
- `/collection/notes` → notes.ejs
- `/collection/property` → property.ejs
- `/gallery` → gallery.ejs
- `/collection/sexuality` → sexuality.ejs
- `/collection/projects` → projects.ejs

### Detail pages (4)
- `/collection/music/:album` → music-album.ejs
- `/collection/music/artist/:name` → music-artist.ejs
- `/collection/music/artists` → music-artists.ejs
- `/collection/photos/:album` → photo-album.ejs

### Ontology/document pages (9)
- `/model-data` → model-data.ejs
- 8 × `ontology-view-*.ejs` pages

## Pattern

Same as #636 — include the content-actions partial in each view:

```ejs
<%- include('partials/content-actions') %>
```

Place it below the page header / breadcrumb, above the main content area. Match positioning from Glimmers or About pages.

## Exclusions

- `/werk` — instrument, not content
- `/reflect` / `/self` — different interaction model (chat)
- `/admin/*` — internal tooling
- Static wrapped pages (`.html`) — different templating

## AC

1. All 24 pages include `content-actions` partial
2. Buttons render consistently with Glimmers/About reference
3. No regressions on #636 pages
4. Smoke check each page — load, verify buttons appear

## Notes

- All EJS views are bind-mounted — **no deploy needed**
- Reference partial: `views/partials/content-actions.ejs`
- This completes the UX normalization Jeff requested
