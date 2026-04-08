# Brief: Photos Browse — Data Quality & Completeness

**From:** Wren (PM)
**To:** Kade (Engineer)
**Date:** 2026-02-18
**Priority:** P1 — Now
**Re:** The Photos browse page doesn't serve Jeff yet. Here's what needs to change.

---

## Context

Jeff opened `/collection/photos` this morning after your iCloud backfill shipped. Your Slack post said coverage went from 75% to ~100%. **What Jeff actually sees: ~30-40% of the grid is still blank dark squares, and screenshots dominate the real photos.**

The backfill was good engineering. But the user experience doesn't match the numbers. We need to reconcile and fix.

Screenshot attached to this brief's context: 9,733 items, roughly a third are blank thumbnails, another third are screenshots/screen recordings mixed with actual photographs.

---

## Three Problems (Prioritized)

### 1. Screenshot Filter (Highest Impact)

**Problem:** Screenshots, screen recordings, iOS home screen captures, app mockups, SOLID login pages, and Google Drive error pages are mixed in with real photographs. They're digital debris, not photos Jeff wants to browse.

**What to do:**
- Use `mediaSubtype` from Apple Photos SQLite to classify items
- Filter out screenshots (`PHAssetMediaSubtype.screenshot`) and screen recordings by default
- Add a toggle or filter control: "Show screenshots" (off by default)
- Display the filtered count: "6,200 photos" not "9,733 photos" (numbers are illustrative — get the real split)

**Acceptance criteria:**
- [ ] Default browse view shows only real photos (no screenshots, no screen recordings)
- [ ] Page count reflects filtered total
- [ ] User can toggle screenshots on if they want them
- [ ] No data deleted — just filtered from the default view

### 2. Thumbnail Coverage Reconciliation

**Problem:** Your backfill report says 2,410 thumbnails generated, coverage ~100% of PhotoKit-visible assets. But the browse page still shows many blank (dark navy) squares. The numbers don't match what Jeff sees.

**What to do:**
- Audit: How many of the 9,733 items in the browse page actually have a thumbnail file on disk?
- For items WITHOUT thumbnails: why? Categories:
  - iCloud-only (PhotoKit couldn't download)
  - Not in Apple Photos at all (harvested from somewhere else?)
  - File corruption / generation failure
  - Video without poster frame
- Report the real breakdown so we can make product decisions on each category

**Acceptance criteria:**
- [ ] Audit report: exact count of items with/without thumbnails, broken down by reason
- [ ] Blank squares show a reason indicator (not just dark navy) — at minimum, a text label like "No thumbnail" or a distinct placeholder per category
- [ ] If there's a fixable category (e.g., videos need poster frames), flag it as follow-up work

### 3. Time Grouping (After 1 & 2)

**Problem:** The browse page is a flat grid of 9,733 items with no organization. No dates, no months, no visual breaks. It's a wall of tiles, not a browsable collection.

**What to do:**
- Group photos by month/year using the date metadata from SQLite
- Add section headers: "February 2026", "January 2026", etc.
- Most recent first (reverse chronological)

**Acceptance criteria:**
- [ ] Photos grouped by month with visible section headers
- [ ] Reverse chronological order (newest first)
- [ ] Section headers show month + year + count ("February 2026 — 47 photos")

---

## Sequencing

1. **Screenshot filter** — do this first. Biggest visual quality improvement, lowest effort.
2. **Thumbnail audit** — do this second. We need the real numbers before we can call coverage "done."
3. **Time grouping** — do this third. Transforms the page from "dump" to "timeline."

Each ships independently. Don't wait for all three to land one PR.

---

## Questions for You

**Q1:** Your backfill said 2,410 thumbnails generated. The page shows 9,733 items. How many of those 9,733 actually have a thumbnail file on disk right now? I need the real number.

**Q2:** Do you already have `mediaSubtype` in the harvested data? If not, is it in the SQLite DB and easy to add to the harvest?

**Q3:** For videos (I see at least one play button icon) — are we generating poster frames, or are those all blank?

---

— Wren
