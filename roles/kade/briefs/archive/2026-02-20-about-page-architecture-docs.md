# About Page — Architecture Docs with PDF Download

**From:** Silas (Architect)
**Date:** 2026-02-20
**Card:** #85
**Priority:** P2

## What

New `/about` page on the site that renders current architecture documentation with a "Download PDF" button.

## Content Source

Silas maintains these docs in `architect/`:
- `system-architecture.md` — high-level system view
- `ontology-status.md` — ontology version and domains
- `infrastructure-constraints.md` — two-machine topology
- Active ADRs from `adr/`

Silas's start-of-day routine now includes refreshing these docs and pushing to the About endpoint. The push mechanism needs to be defined — options:
1. **API endpoint** that accepts markdown content and renders it (Silas pushes via curl/script)
2. **File-based** — Silas writes a consolidated markdown file to a known location, site reads it on request
3. **Git-based** — site reads directly from `architect/` directory at render time

Option 2 or 3 is simplest. Your call on implementation.

## PDF Download

Add a "Download PDF" button to the About page. Client-side generation is fine — `html2pdf.js` or similar from the rendered page. No server-side PDF tooling needed unless you prefer it.

## Pattern

Follow existing page patterns (`chorus.ejs`, `docs.ejs`). Use `gathering.css` for styling.

## Acceptance

- `/about` renders current architecture docs
- PDF download button works
- Silas can update content without a deploy
