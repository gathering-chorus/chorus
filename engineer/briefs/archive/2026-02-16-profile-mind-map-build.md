# Brief: Build Interactive Profile Mind Map (v1)

**From**: Wren (PM)
**To**: Kade (Engineer)
**Date**: 2026-02-16
**Priority**: Next (Jeff requested — build when current tech debt pass is done)
**Architectural review**: `product-manager/briefs/2026-02-16-profile-mind-map-response.md` (Silas, approved)

---

## What

Make the profile page hub-and-spoke nodes draggable. Jeff wants to rearrange the four quadrants spatially and have positions persist.

## Spec (from Silas)

- **SVG-based, vanilla JS** — no libraries. ~120 lines of JS.
- **5 draggable nodes**: jeff (center), gathering, cultivating, harvesting, reflecting
- **Drag via mousedown/mousemove/mouseup** + touch equivalents. No HTML5 drag API.
- **Connector lines** update as nodes move (SVG `<line>` elements from center to each quadrant)
- **localStorage persistence**: key `gathering.profile.mindmap`, flat JSON:
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
- **On page load**: read localStorage, apply saved positions. If no saved state, use defaults.
- **Inline `<script>` in profile.ejs** — co-located with markup. Extract to separate file if it grows past v1.

## Responsive

- **Desktop**: full drag interaction
- **Mobile (< 768px)**: fixed positions, no drag. CSS media query disables pointer events on drag handles.

## Accessibility

- Each node: `<g role="img" aria-label="Gathering quadrant">`
- Keyboard: Tab to focus nodes, arrow keys to reposition in small increments (~20 lines extra)
- Don't break tab order or screen reader flow

## What NOT to Build

- No expandable children (v2)
- No adding/removing/renaming nodes (v3)
- No pod persistence (v2)
- No physics/spring simulation
- No zoom/pan

## Tests

- Unit: localStorage read/write, default positions fallback, position clamping (nodes stay in bounds)
- E2E: page loads with mind map, nodes are present, drag interaction works (if Playwright supports SVG drag)

## Design

Use gathering.css tokens for colors. The current profile page styling (purple center circle, white cards with shadows) should carry over to the SVG nodes. Match the existing look — just make it interactive.

---

— Wren
