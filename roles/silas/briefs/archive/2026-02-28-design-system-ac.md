# AC for #542 — Extend design system to all views

**From:** Wren | **To:** Silas | **Date:** 2026-02-28

## Design tokens (the standard)

`gathering.css` already defines these. The AC is: every page uses them instead of hardcoded values.

| Token | Value | Use for |
|-------|-------|---------|
| `--primary-color` | #1A202C | Headings, body text |
| `--accent-color` | #6366F1 | Links, active states, buttons |
| `--bg-color` | #F7FAFC | Page background |
| `--card-bg` | #FFFFFF | Card/container surfaces |
| `--border-color` | #E2E8F0 | Borders, dividers |
| `--text-muted` | #718096 | Secondary text, hints |
| `--font-family` | Inter, system stack | All text |
| `--radius-lg` | 0.5rem | Cards, containers |
| `--shadow-sm` | 0 1px 3px | Cards |
| `--space-*` | xs through xl | All spacing |

## New shared classes to add to gathering.css

| Class | Purpose |
|-------|---------|
| `.page-container` | `max-width: 900px; margin: 0 auto; padding: 20px;` — every page layout |
| `.page-card` | `.card` + padding 2rem — the white content box most pages use |
| `.page-title` | `h1` styling — color, margin, font-size |
| `.prose` | Typography for rendered markdown — headings, paragraphs, code, pre, tables, blockquotes, lists. Currently duplicated in about.ejs, docs views, README pages. |
| `.data-table` | Table styling — borders, padding, header background. Currently hardcoded per page. |
| `.section-fold` | `<details>`/`<summary>` accordion — currently inline in about.ejs |

## Pages in priority order

**Tier 1 — Jeff sees these daily:**
1. `home.ejs` (mind map) — already has photos + consistent circles after #546. Check spacing, font consistency.
2. `about.ejs` (docs list) — has inline styles for prose, section headings, collapsible sections. Migrate to shared classes.
3. `search.ejs` — check form controls, results layout use tokens.
4. `werk.ejs` / `chorus.ejs` / `loom.ejs` / `flow.ejs` — Chorus surfaces Jeff checks frequently.

**Tier 2 — Jeff sees these often:**
5. Collection views (`collection.ejs`, `music.ejs`, `photos.ejs`, etc.) — table/card layouts should use shared classes.
6. `harvest-scope.ejs` — dashboard layout.
7. `practice-spine.html` — recently shipped, check token usage.

**Tier 3 — Less frequent:**
8. `model-data.ejs` / `model-data-hub.html` / `chorus-model-data.html` — data pages.
9. `dashboard.ejs` — admin pod browser.
10. Static HTML in `public/` (gathering-chorus.html, business-plan.html, etc.) — lower priority, these are one-off pages.

## Reference points

- **Navbar** — the gold standard. Clean, consistent, well-spaced.
- **Mind map** (#546) — white circles, consistent sizing, clear hierarchy.
- **gathering.css tokens** — the source of truth for all values.

## What's in scope

- Add the 6 shared classes above to `gathering.css`
- Sweep Tier 1 + Tier 2 pages: replace inline `<style>` blocks with shared classes
- Move navbar styles from `navbar.ejs` partial into `gathering.css`
- Remove any hardcoded color/spacing values that duplicate tokens

## What's out of scope

- No redesign. Pages should look the same or better, not different.
- No new layouts or components beyond the 6 classes above.
- No changes to page logic, routes, or handlers.
- Static HTML pages in `public/` (Tier 3) — defer unless trivial.
- No dark mode, no responsive overhaul, no animation.

## Done when

- Zero inline `<style>` blocks in Tier 1+2 views that duplicate token values
- Every Tier 1+2 page uses `.page-container` for layout
- Prose content uses shared `.prose` class
- Tables use shared `.data-table` class
- Visual regression: pages look the same or cleaner, nothing broken
- Jeff can browse the app without snagging on inconsistencies
