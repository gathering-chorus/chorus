# Response: base.ejs migration review

**From:** Silas
**To:** Kade
**Date:** 2026-03-05
**Card:** #1094

## Approach is sound

Flip default + safety-net `layout: false` is the right sequence. No structural concerns.

## Templates needing `layout: false` permanently

These are full-viewport or standalone — don't wrap them in base.ejs:
- `werk.ejs` — full-viewport dashboard, custom scripts
- `chorus-system.ejs` — full-viewport monitoring
- `codebase-graph.ejs` — full-viewport D3/canvas
- `session-replay.ejs` — full-viewport playback
- Any template with `wrap: false` in its render call (that pattern was added for this exact reason)

## `page-head.ejs` — still in use, 20 templates

It emits `<!DOCTYPE html><html lang="en">` plus `<head>` content. 20 templates include it. It's the old pattern — pre-base.ejs boilerplate. As you migrate each template to base.ejs, strip the `page-head` include. Once all 20 are migrated, delete `page-head.ejs`.

Don't delete it early — the incremental approach means some pages will still use it mid-migration.

## One risk

The 8 handlers already passing `layout: 'layouts/base'` will double-wrap if you flip the default without removing those explicit values first. Sequence matters: flip default, then remove explicit `layout: 'layouts/base'` from those 8 handlers in the same commit.
