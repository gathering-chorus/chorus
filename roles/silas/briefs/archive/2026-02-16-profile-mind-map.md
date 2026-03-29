# Brief: Interactive Profile Mind Map

**From**: Wren (PM)
**To**: Silas (Architect)
**Date**: 2026-02-16
**Priority**: Next (Jeff requested, carded up)

---

## Context

The profile page now has a hub-and-spoke layout: Jeff at center, four quadrants (Gathering, Cultivating, Harvesting, Reflecting) connected to him. The navbar mirrors these categories. Jeff likes this and wants the nodes to be **draggable** — arrange them spatially, have positions persist across sessions.

This is v1. Scope is intentionally small:
- Four quadrant nodes + center node are draggable
- Positions saved to localStorage
- That's it — no expandable children, no adding nodes, no pod persistence

## What I Need From You

1. **Interaction model**: What library/approach for drag-and-drop on the profile page? The app already has vis.js in the stack (WebVOWL). Is that the right tool here, or is something lighter better (e.g., plain HTML5 drag, interact.js, a lightweight canvas lib)?

2. **Data shape**: What does the position data look like? `{ nodeId: string, x: number, y: number }[]` saved to localStorage? Or something more structured that anticipates v2 (expandable children, connections)?

3. **v2 persistence question**: When we eventually move beyond localStorage, should positions live in the pod (Turtle) or in app-level storage? Pod storage means it's part of Jeff's SOLID data graph. App storage is simpler but doesn't travel with the user.

4. **Profile view architecture**: The current profile.ejs is server-rendered. An interactive mind map needs client-side JS. What's the right boundary — a lightweight client script in the EJS page, or does this warrant a small client-side component approach?

## Constraints

- Must work with existing gathering.css design tokens
- No new heavy frameworks (no React, no Vue — we're server-rendered EJS)
- Responsive: should degrade gracefully on mobile (maybe lock positions on small screens?)
- Accessibility: draggable nodes need keyboard support or at minimum don't break screen readers

## Scope Boundary

**v1 (this card)**: Drag four nodes, save positions, load on next visit.
**v2 (future)**: Nodes expand to show children (menu items). Children draggable. Pod persistence.
**v3 (future)**: Add/rename/connect nodes. True mind map with full CRUD.

Jeff explicitly approved v1 scope. v2+ earns its way in based on whether v1 changes how he uses the page.

---

— Wren
