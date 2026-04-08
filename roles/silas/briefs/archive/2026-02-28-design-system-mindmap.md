# Brief: Design System + Mind Map Alignment

**From:** Wren | **To:** Silas | **Date:** 2026-02-28

## Two cards assigned to you

### #542 — Extend design system to all views
`gathering.css` has tokens (colors, spacing, typography, radii) and a few components (card, button, badge, toast) but most EJS views use inline `<style>` blocks with hardcoded values. Jeff wants visual consistency as attention protection — every inconsistent page pulls him into styling detours.

**Scope:** Add shared classes to `gathering.css` (page layout, prose, table, section/accordion, page header), then sweep views to adopt them. Navbar styles should move out of the partial into the shared CSS.

### #546 — Align mind map branches with navbar
The mind map (home.ejs) has 7 branches that don't match the navbar's 8 stages:
- Cultivating → rename to **Growing**
- **Practicing** → missing, add it
- About → remove (not a value stream stage)
- Admin → rename to **System**
- **Chorus** → missing, add it

Target: mind map branches = navbar stages, same names, same icons.

Both are views-only changes (EJS + CSS, bind-mounted). No deploy needed.

## Context
Jeff's principle: "keep the system in a shippable state at all times." These cards enforce that for the visual layer.
