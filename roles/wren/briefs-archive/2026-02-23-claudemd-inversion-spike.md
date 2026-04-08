# Brief: CLAUDE.md Inversion — Dynamic Bootstrap

**From:** Silas (Architect)
**To:** Wren (PM)
**Date:** 2026-02-23
**Cards:** #252 (service registry), #254 (bootstrap inversion)
**Priority:** P1

## Jeff's Direction

Jeff wants a total inversion of control on CLAUDE.md and MEMORY.md. Instead of 600 lines of static generated instructions, CLAUDE.md becomes a thin bootstrap — possibly as simple as `/werk init <role>`. MEMORY.md goes away entirely, replaced by `/chorus search`.

## What This Means

- **CLAUDE.md today**: 43 fragments → generator → static doc loaded before first tool call. Every change requires regeneration.
- **CLAUDE.md future**: Thin static bootstrap that tells the role "go read the live system." Role identity + one init command.
- **MEMORY.md today**: Manually maintained stale cache of things the chorus index already knows.
- **MEMORY.md future**: Deleted. `/chorus search` is the memory.

## Dependency Chain

1. **#252 — Service registry** (services.json): Base layer. Every port, URL, health endpoint in one queryable file. Silas builds, low effort.
2. **#254 — Bootstrap inversion**: Top layer. Redesign what CLAUDE.md contains vs what it fetches dynamically. Needs all three roles — Wren for what context matters, Silas for how it assembles, Kade for any app-side integration.

## What I Need From You

- Sequence this against the current backlog — Jeff said "let Wren work me on when to start"
- Decide if #252 (registry) should ship first as a standalone or if both ship together
- This touches the claudemd-gen.sh pipeline you own — the generator might become much simpler or unnecessary

## Constraint

Claude Code loads CLAUDE.md before any tool call. The bootstrap must be static text — but it can be very thin (role identity + init instruction). Everything else assembles dynamically.
