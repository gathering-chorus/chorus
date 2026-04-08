# Brief: #636 — UX normalization across all pages

**From:** Wren
**Date:** 2026-03-01
**Card:** #636
**Priority:** P1

## Context

Deep UX scan of 14+ pages. Site is cleaner after recent work (#615, #617) but two distinct page families exist — "new standard" pages look great, "original" pages are missing action buttons, card containers, and consistent indentation.

## Broken (fix first)

- **`/collection/music`** — blank. Tan background, no navbar, no content.
- **`/search`** — blank. Same issue.

## Missing PDF / Share / Reflect buttons

These pages have NO `content-actions` partial:
- `/collection/stories`
- `/collection/values`
- `/collection/practices`
- `/collection/people`
- `/collection/blog`
- `/collection/photos`

Reference: Glimmers and About/Architecture have it and look clean. The partial is at `views/partials/content-actions.ejs`.

## Container inconsistencies

| Page | Container | Breadcrumb | Notes |
|------|-----------|------------|-------|
| Glimmers | Card on white | ← Home | **Good — the standard** |
| About | Card on white | No (but OK) | Has action buttons |
| Stories | No card, flush on white | Missing | Older feel |
| Values | No card, flush on white | ← Home | No action buttons |
| Practices | Same as Values | ← Home | Consistent with Values |
| People | Same as Values | ← Home | Same pattern |
| Blog | Same as Values | ← Home | Has Sync Now but no actions |
| Photos | Has card | ← Home | Empty state, no actions |
| Wrapped (chorus.html) | Dark hero | Action buttons in navbar zone | Buttons visually disconnected |

## Target state

Every content page should have:
1. `← Home` breadcrumb
2. PDF + Share + Reflect action buttons (content-actions partial)
3. Card container treatment (`.detail-container` or `.collection-container`)
4. 1in page margin matching nav

## Notes

- All collection pages are EJS views — bind-mounted, live immediately. **No deploy needed.**
- CSS changes in `public/css/gathering.css` also bind-mounted.
- Werk page intentionally excluded from action buttons — it's an instrument, not content.
- Reflect/Self excluded — different interaction model (chat).
