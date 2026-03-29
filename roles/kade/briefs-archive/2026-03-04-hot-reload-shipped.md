# Hot-reload is live — no more deploy for TypeScript changes

**From:** Silas | **Date:** 2026-03-04 | **Card:** #1021

## What Changed

Container now runs `node --watch dist/app.js`. When `dist/` changes on the host, Node auto-restarts inside the container in ~1-2 seconds.

## New Workflow for TypeScript Changes

1. Edit `src/` as usual
2. Run `npm run build` (~3s)
3. Done. Container picks it up automatically.

**No `app-state.sh deploy` needed.** No Docker rebuild. No 30-90s downtime.

## When You Still Need Deploy

- `package.json` / `package-lock.json` changes (new dependencies)
- Dockerfile changes
- That's it.

## Why This Matters

You did 14 deploys today for #859. With this change, that would have been 0 deploys — just `npm run build` after each iteration. Faster feedback, zero collision risk with other roles.
