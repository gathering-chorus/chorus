# Brief: #883 Search Page Visual Refresh

**From:** Wren (PM)
**To:** Kade (Engineer)
**Date:** 2026-03-05
**Card:** #883
**Priority:** P3

## What

The search page is functional but visually flat — plain white, no identity. Give it presence and reduce tag clutter.

## Design Direction

### 1. Hero / Background Treatment

Use `backgrounds/reflect-dawn.jpg` as a subtle background behind the search form area. The home page uses `moon-bg.jpg` full-bleed on dark — search should be lighter but with the same "this page has atmosphere" feel.

Pattern:
- Top section (search form) gets the background image, darkened overlay, white text on the input
- Results section below stays clean/light for readability
- Transition between hero and results should be a soft gradient fade, not a hard line

Reference: `home.ejs` lines 69-77 for how the mindmap container handles full-viewport background. Search doesn't need full viewport — just the top ~200px as a banner.

### 2. Reduce Tag Noise

Current: `Tags: tag1, tag2, tag3...` dumps everything inline (search.ejs line 239-241). For results with 10+ tags this is visual noise.

Fix:
- Show max 3 tags as small pills/badges (match the collection/type badge style already on line 233-234)
- If more than 3, show "+N more" that expands on click
- Tags should be muted — secondary to title and description, not competing with them

### 3. Small Polish

- The `h1.page-title` "Search" is generic. Drop it — the search input IS the page title. The presence of the hero section makes a heading redundant.
- Stats bar at bottom is fine, keep it

## AC

- Search page has a background image treatment on the form area
- Tags show max 3 as pills, overflow collapsed
- Page title removed, search input is the focal point
- No deploy needed (view is bind-mounted) unless handler changes
- Existing tests still pass

## What NOT to Do

- Don't change search functionality, API, or result ranking
- Don't add new images — use what's in `public/images/backgrounds/`
- Don't dark-mode the results section — keep it readable
