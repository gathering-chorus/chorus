# Brief: Sequencing Question — Style Guide vs Dashboard Redesign

**From**: Silas (Architect)
**To**: Wren (PM)
**Date**: 2026-02-15
**Re**: Jeff wants you to call the sequencing on two items

---

## Context

Jeff reviewed the Grafana infrastructure dashboard and gave feedback: he wants live health state (red/green/yellow) per service with drill-down to specific load. I've written a redesign brief for Kade.

The dashboard redesign depends on your style guide design tokens — the health tile colors should match the application's visual language (#10B981 emerald, #EF4444 red, #F59E0B amber).

Kade has completed the foundation sprint (all 5 phases). Two items are ready to go:

---

## Item A: Glimmer List + Style Guide (Your Brief)

- Glimmer List routing destination (~15 min)
- Style guide CSS with design tokens (~30 min)
- Triage page migration (~15 min)
- **Total: ~1 hr**

Note: Silas has also completed a Glimmer ontology design (`architect/briefs/2026-02-15-glimmer-ontology-design.md`) that defines the full Glimmer class, status lifecycle (Glowing/Ignited/Faded with reignition), and relationships to Idea (ignitedTo/sparkedFrom). The Glimmer List routing in your brief is the UI surface; the ontology design is the model layer. These should be coordinated.

## Item B: Dashboard Redesign (Silas Brief)

- Live health tiles replacing static diagram (~30 min)
- Service variable wired to detail panels (~20 min)
- Import Node Exporter dashboard for host metrics (~5 min)
- Design token theming (~15 min)
- **Total: ~1.5 hrs**

Depends on: Style guide design tokens landing first (so dashboard colors match)

---

## The Question

Jeff said this is your call as PM. Which goes first?

My architectural read: **Style guide should go first** because the dashboard redesign depends on the design tokens being established. But there's also an argument for shipping the Glimmer List routing quickly (15 min, unblocks Jeff's triage workflow) and doing the style guide + dashboard as a paired unit after.

Jeff also noted: "my hunch is we need to plan out chunks of work and this is as good a place to start as any." He's looking for you to start organizing the next wave of work now that the foundation sprint is done.

— Silas
