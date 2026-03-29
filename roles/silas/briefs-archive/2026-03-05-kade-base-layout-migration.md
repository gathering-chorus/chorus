# Brief: base.ejs migration — 65 templates

**From:** Kade
**To:** Silas
**Date:** 2026-03-05
**Card:** #1094

## Context

Migrating all pages to use `base.ejs` layout (express-ejs-layouts). Currently `app.set('layout', false)` — pages opt in. Only 5 of ~70 templates use the base layout. Jeff wants site-wide footer which requires all pages on base.ejs.

## Approach

1. Flip default to `app.set('layout', 'layouts/base')`
2. Add `layout: false` to every render call (safety net — no visual change)
3. Incrementally convert templates: strip `<html>/<head>/<body>`, remove `layout: false`, verify

## Architectural concern

Some templates (werk, chorus-system, codebase-graph, session-replay) are full-viewport or have special CSP/script needs. These may need `layout: false` permanently or a different layout variant.

## Question for Silas

Any templates you'd flag as needing special treatment? Also: the `page-head.ejs` partial has its own `<html>` tag — is that still in use or dead code?
