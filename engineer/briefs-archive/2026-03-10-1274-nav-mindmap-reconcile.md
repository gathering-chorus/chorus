# Brief: Reconcile mind map and navbar (#1274)

**From:** Wren | **Date:** 2026-03-10

## What

Mind map (`views/home.ejs` ~line 860-1100) and navbar (`views/partials/navbar.ejs`) maintain separate lists of the same navigation structure. They've drifted. Reconcile into a single data source rendered two ways.

## Gaps (from audit)

**Navbar has, mind map missing:**
- Voice Analytics (`/voice-analytics`) — Reflecting dropdown
- Gathering Graph (`/gathering-graph`) — System dropdown
- System Graph (`/gathering-chorus-system-graph`) — System dropdown
- Hooks (`/hooks`) — System > Analytics
- Fitness Functions (`/fitness-functions`) — System > Analytics
- Hooks Lifecycle (`/gathering-docs/claude-hooks`) — System > Analytics
- Session Tempo (Grafana link) — System > Analytics

**Mind map stale:**
- `/incubation` still in Growing branch — DEC-082 folds into Ideas, remove it
- `.html` suffixes: `/lightlife.html`, `/practice-spine.html`, `/gathering-chorus.html`, `/chorus-consulting.html`, `/business-plan.html`, `/chorus-model-data.html`, `/model-data-hub.html` — use clean routes

**Disabled placeholders (no pages behind them):**
- Meditation, Yoga, Gardening, Exercising (Practicing branch)
- Journal (Reflecting branch)

**Path divergence:**
- Mind map: `/admin/replay` vs navbar: `/system/replay`

## Approach

Extract a shared nav tree — either a JSON file or an EJS data partial — that both `home.ejs` and `navbar.ejs` render from. Structure:

```json
{
  "branches": [
    {
      "name": "Sowing",
      "leaves": [
        { "label": "Seeds", "icon": "🌱", "href": "/seeds" }
      ]
    }
  ]
}
```

Mind map reads branches + leaves for its radial layout. Navbar reads the same structure for dropdowns. Add/remove in one place.

## AC
- [ ] Single data source for navigation structure
- [ ] Mind map and navbar render identical node sets
- [ ] /incubation removed (DEC-082)
- [ ] .html suffixes cleaned up
- [ ] Disabled nodes clearly marked with rationale

## Notes
- Views are bind-mounted — changes are live immediately, no deploy needed unless you touch src/
- #1203 (fold incubation) overlaps — the /incubation removal can happen in either card. Just don't duplicate.
- Keep disabled nodes visible on mind map (greyed out) but hidden from navbar — they're aspirational on the home page, clutter in the nav.
