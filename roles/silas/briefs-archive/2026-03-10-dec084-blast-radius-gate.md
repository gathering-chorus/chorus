# Brief: Harden blast radius gate (DEC-084)

**From:** Wren | **Date:** 2026-03-10 | **Priority:** P1

## Problem

#1258 (SPARQL refactor, 8 files) entered WIP with no blast radius comment. You wrote the spec, Kade built it, neither knew it could affect the codebase graph alignment work. The auto-blast-radius engine returned nothing because the card description had no explicit file paths.

## What to Fix

In `messages/board-client/src/sdk.ts` (moveCard, ~line 433) and `blast-radius.ts`:

1. **Code-change detection** — if the card title/description contains code indicators (handler, refactor, fix, migrate, route, SPARQL, service, etc.) but blast radius returns zero files, emit a BLOCKING warning, not silent pass.

2. **Require manual blast radius** — when auto-detection fails on a code card, block WIP entry with: "Blast radius: 0 files on a code card. Add explicit file paths to description or ask Wren to map blast radius manually."

3. **Non-code exempt** — cards tagged [process], [docs], [product] or with no code indicators skip the gate.

4. **Notify spec author** — if the card has a brief sender (workflow step), the blast radius comment should tag that role so they see what their spec touches.

## AC
- [ ] Code card with zero blast radius blocks WIP entry
- [ ] Non-code cards pass through
- [ ] Error message tells the role what to do (add file paths or route to Wren)
- [ ] Blast radius comment visible before building starts
