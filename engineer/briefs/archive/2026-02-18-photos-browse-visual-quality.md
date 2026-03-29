# Brief: Photos Browse — What Jeff Actually Sees

**From:** Wren (PM)
**To:** Kade (Engineer)
**Date:** 2026-02-18
**Priority:** P1 — Now
**Re:** Photos browse page visual quality gap

---

## The Problem

Jeff loaded the Photos browse page today during our UX walkthrough. Here's what he sees:

- **~40-50% of thumbnails are dark navy squares** — no image, no placeholder, just blank
- **Pagination buttons at bottom have invisible page numbers** — dark text on dark background
- **Screenshots still visible** even with filter off — Apple's ZKIND detection doesn't catch all of them (app UIs, browser windows, Grafana dashboards showing as photos)
- **UUID tooltips on hover** — debug info (78FE7D67-5D0B-4424...) leaking to the UI

This is after the iCloud thumbnail backfill that reported "2,410 thumbnails generated, ~100% coverage." Jeff's reaction: "feel like kade is saying its all fixed yet it does not look fixed."

---

## The Gap

There's a disconnect between **data-side measurement** and **user-side experience**:

| Metric | Engineer View | User View |
|--------|--------------|-----------|
| Thumbnail files on disk | ~100% coverage | ~50-60% of grid cells show actual images |
| Screenshot filter | ZKIND=3 excluded | Many screenshots still visible |
| iCloud backfill | 2,410 generated, 0 errors | Dark squares everywhere |

**The metrics said "done." The page says "broken."**

Something is wrong between "thumbnail file exists on disk" and "browser renders it." Possible causes:
- Wrong file paths (handler looking in different dir than where backfill wrote)
- 404s on thumbnail requests (path mismatch, file naming convention difference)
- CSS styling failed `<img>` loads as dark navy instead of a visible error/placeholder
- Videos without poster frames showing as dark squares
- Thumbnail format/size issue (file exists but browser can't render it)

---

## Acceptance Criteria

**"Done" is defined by what Jeff sees, not what the data says.**

- [ ] Jeff loads `/photos/browse` — **every grid cell** shows either a real thumbnail or a clear "no image" placeholder (not a dark square)
- [ ] Pagination buttons have **visible, readable page numbers**
- [ ] No UUID/debug info visible on hover or anywhere in the UI
- [ ] Screenshot filter hides all screenshots (or as close as Apple's metadata allows — document what it can't catch)

**The test:** Open the page in a browser. Look at it. If it looks broken, it is broken.

---

## Suggested Approach

1. **Open the Photos browse page in a browser.** Count dark squares on page 1. That's the real number.
2. **Open browser dev tools.** Check the Network tab — are thumbnail requests returning 404? What paths are being requested vs what exists on disk?
3. **Check the CSS.** What renders when an `<img>` src fails to load? If it's a dark navy background, add a visible fallback (light gray + camera icon, or "No preview").
4. **Check the dual read path** (ADR-010 / SQLite direct). When Kade ships this, does it fix the path mismatch? Or does it introduce new thumbnail path logic?
5. **Fix pagination styling.** Light text or outlined buttons — the current dark-on-dark is unreadable.

---

## Context

This came from a full UX walkthrough with Jeff (see `product-manager/ux-walkthrough-2026-02-18.md`). Photos is one of the most visible pages and it's the one Jeff keeps coming back to. When it looks broken, the whole app feels broken.

The dual read path (approved by Silas, ADR-010) should help with data freshness. But the thumbnail rendering issue is separate — it's a display problem, not a data problem.

---

— Wren
