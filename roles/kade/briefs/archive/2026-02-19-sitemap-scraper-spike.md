# Spike: Automated Sitemap Page Scraper

**From:** Wren (PM)
**To:** Kade (Engineer)
**Date:** 2026-02-19
**Priority:** P2 — Next
**Card:** #76
**Re:** Automate visual health checks across all Gathering pages

---

## What We Want to Learn

Can we automate rendering and screenshotting every page in the Gathering sitemap using Playwright (which you already have for E2E)?

---

## Sitemap (from UX walkthrough)

```
/                          Login page
/profile                   Mind Map (landing after login)
/capture/triage            Capture Triage
/collection/glimmers       Glimmer List
/ideas                     Ideas & Projects
/collection/books          Books grid
/property                  Property detail
/blog                      Blog post list
/collection/sexuality      Gallery (images-api)
/collection/music          Music album grid
/collection/music/:id      Music album detail (pick one)
/photos/browse             Photos grid (paginated)
/reflecting                Reflecting quadrant
```

---

## Spike Questions

### 1. Auth
Can Playwright authenticate with SOLID (solidcommunity.net) in headless mode? The app requires SOLID login. Options:
- a) Use an existing session cookie / token
- b) Automate the SOLID login flow in Playwright
- c) Add a dev-mode bypass for local testing
- d) Something else?

### 2. Screenshots
For each route:
- Navigate to the page
- Wait for content to load (network idle or specific selector)
- Take a full-page screenshot
- Save to `e2e/screenshots/YYYY-MM-DD/<route-name>.png`

### 3. Visual Health Checks
Beyond screenshots, can Playwright assert:
- [ ] No `<img>` elements with failed loads (naturalWidth === 0)
- [ ] No elements with text color matching background color (invisible text)
- [ ] Page has at least N content elements (not empty/error page)
- [ ] No raw UUIDs visible in text content
- [ ] No HTML entities visible (&amp;, &#8217;, etc.)

### 4. Execution Model
- Manual script: `npm run scrape-sitemap` — Jeff runs when he wants a gemba snapshot
- Could later become: pre-deploy check, cron job, or CI step

### 5. Baseline Comparison (stretch)
- Can we diff today's screenshots against yesterday's?
- Tools: pixelmatch, playwright built-in visual comparison, or just side-by-side folder comparison

---

## Time Box

This is a spike — 2 hours max. Goal is a working script that can screenshot 3-4 pages and report broken images. If auth is a wall, document what's needed and stop.

---

## Why This Matters

Jeff's gemba walk yesterday (UX walkthrough) was the most productive session we've had. But it was manual — he had to open every page and screenshot it himself. If we can automate that, Jeff gets a visual health report whenever he wants without being on the production floor. That's the lean model: quality built into the flow, visible without inspection.

---

— Wren
