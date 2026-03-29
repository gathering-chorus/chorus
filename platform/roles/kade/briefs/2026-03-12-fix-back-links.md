# Brief: Fix ~24 broken/missing back-links — #1335

**From:** Wren (PM)
**To:** Kade (Engineer)
**Date:** 2026-03-12
**Card:** #1335

## Context

Site walkthrough (#1265) found back-links broken across the site after nav-tree restructure (#1294). Three problems: wrong targets, missing back-links, inconsistent UX.

## Rules

- **L1 pages** (collections) → `← Home` (mind map)
- **L2 pages** (details, sub-pages) → `← [L1 Parent Name]`
- All use `.back-link` class, `&larr;` prefix, positioned top of page
- No inline styles, no `.back-btn`, no arrow-only links

## Fixes

### Chorus pages — add missing back-links (`← Chorus`, href `/chorus`)
- `cost.ejs`
- `decisions.ejs`
- `flow.ejs`
- `werk.ejs`
- `gathering-chorus-system-graph.ejs`
- `session-replay.ejs`
- `dashboard.ejs`

### Chorus pages — fix wrong target (→ `/chorus`)
| File | Current href | Fix to |
|------|-------------|--------|
| `hooks.ejs` | `/` | `/chorus` |
| `fitness-functions.ejs` | `/` | `/chorus` |
| `admin-access.ejs` | `/dashboard` | `/chorus` |
| `admin-users.ejs` | `/dashboard` | `/chorus` |

### Broken route
| File | Current href | Fix to |
|------|-------------|--------|
| `admin-harvest-photos.ejs` | `/admin` (doesn't exist) | `/photos` |

### Ontology views — fix parent (`← Self`, href `/self`)
All files in `views/ontology-views/`:
- `books.ejs`, `chorus.ejs`, `model-data.ejs`, `music.ejs`, `notes.ejs`, `photos.ejs`, `property.ejs`, `self.ejs` — change `/model-data` → `/self`, text `← Self`
- `sexuality.ejs` — add back-link (currently missing): `<a href="/self" class="back-link">&larr; Self</a>`

### UX consistency
| File | Problem | Fix |
|------|---------|-----|
| `collection-network.ejs` | inline styles | Replace with `.back-link` class |
| `collection-people.ejs` | inline styles | Replace with `.back-link` class |
| `book-upload.ejs` | `.back-btn` + arrow-only | `.back-link` + `← Books` |
| `self.ejs` | text says `← Mind Map` | Change to `← Home` |

## AC

- Every page with a back-link uses `.back-link` class with `&larr;` prefix
- L1 → Home, L2 → L1 parent, no exceptions
- All hrefs resolve to live routes
- No inline-styled navigation elements
- No deploy needed (views are bind-mounted)
