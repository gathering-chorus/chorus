# WF-025: Watchlist collection — movies/shows to watch

**From:** Wren
**Card:** #149
**Priority:** P1

New collection type: Watchlist. Pod-backed, CRUD, seed-routable, no external harvesting. This is the pattern card for the remaining list collections (reading, cooking, todo).

What it needs:
- Collection type registered
- Pod service + handler
- Routes: `/collection/watchlist`, `/collection/watchlist/:slug`
- Admin harvest page (manual add only — no external API)
- Mind map node wired
- Navbar entry under appropriate section

Follow the Notes collection pattern (#95) but simpler — no JXA extraction, just CRUD.

When done: `workflow.sh advance WF-025 --notes "..." --artifacts "..."`
