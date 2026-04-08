# Brief: #629 Login Fix + CSP Google Fonts — Kade → Silas

**Card:** #629 (Login session lost after SOLID callback)
**Status:** Fixed and deployed

## What happened

Static page routes and session replay routes (#626) were registered at line 584 in `app.ts` — **before** session middleware at line 616. `req.session` was undefined for those routes, so `adminMiddleware` always saw "unauthenticated" and redirected to login.

## What I did

1. Removed `express.static` from line 584 (before session middleware)
2. Re-added static page routes, session replay routes, and `express.static` after `optionalAuth` (line 646+)
3. Order is now: session middleware → optionalAuth → static page routes → session replay routes → `express.static`

## CSP change (heads up)

Added Google Fonts domains to CSP — pages wrapped by the static wrapper now get CSP headers they didn't have as raw static files:
- `style-src`: added `https://fonts.googleapis.com`
- `font-src`: added `https://fonts.gstatic.com`

5 pages use Google Fonts (gathering-chorus, chorus-consulting, lightlife, business-plan, wardley-map). Without this, fonts failed silently.

## SHA idempotency guard note

`app-state.sh deploy` skipped because git SHA hadn't changed (uncommitted work). Had to use `app-state.sh restart` instead. The bind-mounted `dist/` had the new code. May want to consider a `--force` flag that bypasses the SHA check for this scenario.

## Files changed
- `src/app.ts` — route registration moved, CSP updated
- `views/partials/session-recorder.ejs` — fixed `rrwebRecord.record()` → `rrwebRecord()` (global IS the function)
