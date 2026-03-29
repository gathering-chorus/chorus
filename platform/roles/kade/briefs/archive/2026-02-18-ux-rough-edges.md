# Brief: UX Rough Edges — Quick Wins from Walkthrough

**From:** Wren (PM)
**To:** Kade (Engineer)
**Date:** 2026-02-18
**Priority:** Now — these are the items Jeff can see tomorrow
**Re:** Consolidated UX fixes from full app walkthrough with Jeff

---

## Context

Jeff and I walked every page in Gathering today with screenshots. Full review at `product-manager/ux-walkthrough-2026-02-18.md`. These are the **quick-fix items** — things that make the app feel rough but shouldn't require architectural changes.

**Separate brief already sent:** Photos browse visual quality (`engineer/briefs/2026-02-18-photos-browse-visual-quality.md`) — that's a bigger investigation. This brief is everything else.

---

## Now Items (fix in next session)

### 1. Profile: Hub node shows "SolidCommunity User" instead of "Jeff Bridwell"
- **Where:** Profile / Mind Map landing page (the center hub node)
- **What's wrong:** Intermittently shows "SolidCommunity User" instead of Jeff's name
- **Fix:** Check where the display name is sourced — likely falling back to a default when SOLID profile fetch fails or is slow
- **Effort:** Small — probably a data fetch / fallback issue

### 2. Profile: "lifes-practice" label
- **Where:** Profile / Mind Map — the Reflecting quadrant child node
- **What's wrong:** Shows `lifes-practice` (looks like a URL slug). Every other node is title-cased.
- **Fix:** Display as "Life's Practice"
- **Effort:** Tiny — string change in the mind map data

### 3. Blog: Harvest quality (author, categories, HTML encoding)
- **Where:** Blog Post List (Harvesting > Blog)
- **What's wrong:**
  - Author shows "User 1" instead of "Jeff Bridwell"
  - Categories show numeric IDs ("Category 33", "Category 34") instead of names
  - HTML entities not decoded — `&#8217;` (apostrophes), `&nbsp;` (spaces) showing as raw text in titles and excerpts
- **Fix:** Update the WordPress harvester to pull author display name, category names (not IDs), and decode HTML entities
- **Effort:** Medium — harvester code changes + re-harvest

### 4. Music: Artist name slugs
- **Where:** Music Album Grid (Harvesting > Music)
- **What's wrong:** Artist names display as URL slugs — "13th-floor-elevators" instead of "13th Floor Elevators"
- **Fix:** Either store proper display names during harvest, or transform slugs to title case on display (replace hyphens with spaces, capitalize words)
- **Effort:** Small — display transform or harvest fix

### 5. Photos: Pagination buttons unreadable
- **Where:** Photos Browse page — bottom pagination
- **What's wrong:** Page number buttons have dark text on dark background — numbers are invisible
- **Fix:** Light text color, or outlined/bordered buttons, or use the gathering.css style tokens
- **Effort:** Tiny — CSS fix

### 6. Photos: UUID tooltip on hover
- **Where:** Photos Browse page — hover over any photo
- **What's wrong:** Shows raw UUID (78FE7D67-5D0B-4424...) as tooltip — debug info leaking to UI
- **Fix:** Remove the `title` attribute or replace with photo date/filename
- **Effort:** Tiny — template change

---

## Nice-to-Have If Time (Next priority)

### 7. Music: Missing album titles in grid
- **Where:** Music Album Grid
- **What's wrong:** Grid tiles show album name + artist slug but no album title text below the artwork
- **Effort:** Small — template change

### 8. Music: "Unknown year" on albums with known years
- **Where:** Music Album Grid / Detail
- **What's wrong:** Some albums show "Unknown year" when the year is known (e.g., Easter Everywhere = 1968)
- **Effort:** Small — check harvest mapping for year field

### 9. Blog: "User 1" author name
- Already covered in #3 above but calling it out — this one is especially visible

### 10. Profile: Connection lines barely visible
- **Where:** Mind Map
- **What's wrong:** Lines connecting nodes are thin and low-contrast. Relationships are the point of a graph view.
- **Fix:** Increase stroke width and/or contrast
- **Effort:** Tiny — CSS/SVG change

---

## DEC-022 Reminder: Ship with Visual Proof

New rule from today: **Before declaring work done, open the page in a browser and screenshot it.** Post the screenshot in #kade. Jeff should never be the first person to discover a visual bug.

"Done" is defined by what the user sees, not what the data says.

---

## Acceptance Test

Jeff loads the app tomorrow. The rough edges should be smoother:
- Hub node says "Jeff Bridwell" (not SolidCommunity User)
- "Life's Practice" (not lifes-practice)
- Blog shows real author name, category names, decoded HTML
- Music shows "13th Floor Elevators" (not 13th-floor-elevators)
- Photos pagination is readable
- No UUIDs visible anywhere

**The full walkthrough doc has more items** (login page redesign, nav bar removal, time grouping for photos, cross-domain links) — those are Next/Later. This brief is the Now stuff.

---

— Wren
