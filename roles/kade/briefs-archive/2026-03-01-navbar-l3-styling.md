# Navbar L3 styling + Values/Practices/People collection pages

**From:** Wren | **Card:** TBD (medium, vertical) | **Priority:** P2

## Part 1: L3 items — match L1/L2 styling

The L3 navbar items (Gathering README, Chorus README, Wardley Map, etc.) look visually demoted — small font, muted color, `↳` prefix. Jeff wants them styled the same as L1/L2: emoji + bold text, full size.

## Where

`views/partials/navbar.ejs` — lines with `padding-left:40px; font-size:0.8em; color:#777;`

## Current L3 style
```html
<a href="/about/GATHERING_README" style="padding-left:40px; font-size:0.8em; color:#777;">&#8627; Gathering README</a>
```

## Target L3 style
Match L1: remove inline styles (or match L1 sizing), replace `↳` with an appropriate emoji, use inherited color. Keep indentation via padding-left if needed for hierarchy, but font size and color should match L1.

## AC
- L3 items have emoji prefixes (not `↳`)
- L3 font size and color match L1/L2
- Visual hierarchy still readable (indentation is fine, just not the dimming)
- No deploy — views are bind-mounted

## Screenshot
Jeff shared a screenshot showing the current state — Growing dropdown, L3 items visually recessed under Projects.

---

## Part 2: Values, Practices, People collection pages

Mind map nodes for Values, Practices, People currently link to `/self` as a fallback. Jeff expects dedicated collection pages — same pattern as `/glimmers`, `/collection/ideas`, `/collection/projects`.

### What exists
- **Pod data**: 29 TTL files in `data/pods/jeff/` — 10 Values, 12 Practices, 7 People
- **Ontology**: `jb:Value`, `jb:Practice`, `jb:Person` types merged in #591
- **Mind map nodes**: Wired in #600, but href goes to `/self`

### What's needed
- Routes: `/collection/values`, `/collection/practices`, `/collection/people`
- SPARQL queries to pull each type from Fuseki (follow existing collection query pattern)
- Views: reuse collection view template (same as ideas/projects)
- Mind map + navbar links updated to point to the new routes
- **Deploy required** — new routes are TypeScript

### AC
- Clicking Values/Practices/People on mind map opens a collection page (not `/self`)
- Each page lists items from pod data via Fuseki
- Navbar Practicing dropdown includes links to all three
- Collection pages match existing styling (Glimmers, Ideas, Projects)
