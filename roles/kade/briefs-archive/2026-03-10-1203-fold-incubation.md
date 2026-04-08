# Brief: Fold Incubation into Ideas (#1203)

**From:** Wren | **Date:** 2026-03-10 | **Decision:** DEC-082

## What

Remove `/incubation` from the navbar and redirect the route to `/ideas`. Migrate the admin actions (promote, merge, tag, delete) from the Incubation chat UI to the Ideas page as inline controls visible to admin users.

## Why

Seed pipeline (#1202) is the intake funnel now. Incubation was the old intake — a command-line chat for idea capture. Keeping both confuses where ideas enter the system. The admin actions are useful but belong on the Ideas page.

## Implementation

1. **Navbar** (`views/partials/navbar.ejs` line 26): Remove `/incubation` link from Growing dropdown.
2. **Route** (`src/app.ts` ~line 1048): Change `GET /incubation` from `renderIncubation()` to `res.redirect('/ideas')`.
3. **Ideas page** (`views/collection-ideas-list.ejs`): Add admin-only inline controls for each idea row — promote to project, merge, tag, delete. Gate behind `isAdmin` like the incubation page was.
4. **Keep all `/api/ideas/*` and `/api/incubation` endpoints** — they serve the Ideas page too.
5. **Archive** `views/incubation.ejs` (rename to `views/_archived/incubation.ejs` or delete — your call).

## AC (from card)
- [ ] `/incubation` redirects to `/ideas`
- [ ] Incubation removed from nav
- [ ] Admin actions (promote, merge, tag) available on Ideas page
- [ ] Ideas page still renders for non-admin users (without admin controls)

## Notes
- Views are bind-mounted — navbar change is live immediately, no deploy needed.
- The redirect and admin controls touch `src/` — will need build + deploy.
- Low priority (P3) — pick up when WIP has a slot.
