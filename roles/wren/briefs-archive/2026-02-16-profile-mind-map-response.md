# Architectural Response: Profile Mind Map

**From**: Silas (Architect)
**To**: Wren (PM)
**Date**: 2026-02-16
**Re**: `architect/briefs/2026-02-16-profile-mind-map.md`

---

## Answers to Your 4 Questions

### 1. Interaction Model

**Recommendation: Vanilla JS drag — no library.**

Rationale:
- 5 nodes (center + 4 quadrants), simple reposition behavior — this is ~100-120 lines of JS
- vis.js is in the stack for WebVOWL but it's overkill here — it's a network graph engine for hundreds of nodes
- HTML5 drag API is awkward for spatial repositioning (designed for drop targets, not freeform placement)
- Plain `mousedown`/`mousemove`/`mouseup` (+ touch equivalents) gives exact control with zero dependencies
- interact.js is fine but adds a dependency for something we can write inline

Pattern: SVG nodes with JS event handlers. SVG gives us clean rendering, scalable graphics, and easy hit detection. Each node is an SVG `<g>` element with a circle + text, positioned via `transform`.

### 2. Data Shape

**Keep it flat for v1. Don't anticipate v2 in the schema.**

```json
{
  "version": 1,
  "positions": {
    "jeff": { "x": 400, "y": 300 },
    "gathering": { "x": 200, "y": 150 },
    "cultivating": { "x": 600, "y": 150 },
    "harvesting": { "x": 600, "y": 450 },
    "reflecting": { "x": 200, "y": 450 }
  }
}
```

localStorage key: `gathering.profile.mindmap`

The `version` field is the only forward-looking piece — lets us migrate the schema cleanly if v2 changes the structure. No arrays of children, no connection metadata, no nesting. That's v2's problem.

### 3. v2 Persistence

**localStorage for v1. Pod storage (Turtle) for v2 — but as a separate UI preferences file, not in the profile RDF.**

Reasoning:
- Node positions are **user preferences**, not **semantic facts**. "Jeff's HBDI is D-dominant" is RDF. "Jeff likes the HBDI node on the left" is UI state.
- If/when we move to pod: create a `/pods/{webId}/preferences/mind-map-layout.ttl` — separate from profile data. This keeps the domain graph clean.
- Pod storage means positions travel with the user (SOLID portability). But that's a v2 concern — don't build it now.

### 4. Profile View Architecture

**Inline `<script>` block in `profile.ejs` — no component framework, no separate JS file yet.**

Reasoning:
- The mind map JS is self-contained: SVG rendering + drag handlers + localStorage read/write
- A `<script>` at the bottom of `profile.ejs` keeps it co-located with the markup
- If v2 grows significantly, extract to `public/js/profile-mindmap.js` and `<script src>` it
- No build step, no bundler, no framework — matches the existing EJS server-rendered pattern

### Responsive Behavior

**Fixed positions on mobile (no drag).** Use a CSS media query to disable pointer events on the drag handles below a breakpoint (~768px). The hub-and-spoke layout renders fine static on small screens — drag UX on touch is a different problem for v2.

### Accessibility

- Each node should be a `<g role="img" aria-label="Gathering quadrant">` for screen readers
- Keyboard support: `Tab` to focus nodes, arrow keys to reposition (small increments). Adds ~20 lines.
- Don't break tab order or screen reader flow — the SVG is decorative/interactive, not structural.

---

## Summary for Kade

When this goes to build:
- SVG-based, vanilla JS, ~120 lines
- 5 draggable nodes (center + 4 quadrants)
- localStorage persistence, flat JSON
- Inline script in profile.ejs
- Desktop only for drag; fixed on mobile
- Keyboard support for accessibility

Estimate: ~2 hours including tests.

— Silas
