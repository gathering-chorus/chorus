# Brief: Board State Reconciled

**From**: Silas (Architect)
**To**: Wren (PM)
**Date**: 2026-02-13
**Priority**: Informational — board is now current

## What Changed

I've updated the kanban board directly to reflect today's work. Jeff asked that we stay aligned on board state since architectural priorities led today. Here's what I did:

### Moved to Done (4 items)
- **Playwright e2e tests — ACL/visibility page flows** — Kade completed 73 E2E tests
- **Private/shared/public access enforcement — resources & data** — ADR-003 shipped, middleware live
- **Build visibility-aware middleware — enforce graduation model on routes** — same work as above, completed
- **Stabilize test foundation — 32 failing tests, CI enforcement, coverage** — 1,613 unit tests passing, ACL at 100%

### Added to Done (1 item)
- **Fuseki TDB2 verification — confirmed persistent storage** — I verified this today. TDB2, Docker volume, 1GB heap. No action needed.

### Added to In Progress (1 item)
- **Pod data backup — verify existing script, add restore verification and observability** — Kade briefed. Existing script found at `scripts/backup-pods.sh`. Scope is verify + fill gaps, not build from scratch.

### Added to Todo (3 items)
- **CI pipeline enforcement — test failures must block builds** — Kade's next after backup
- **YASGUI dashboard integration — replace SPARQL textarea (ADR-004)** — first feature work after foundation
- **Collection fitness tests — SPARQL-based data quality scorecard** — small deliverable, can slot in anywhere

### Unchanged (4 items in Todo)
- Developer Workspace / Cockpit (BL-001) — needs scoping
- GitHub Projects → RDF/OWL bridge (future)
- Product vision refinement — graduation model + SOLID capabilities
- Ideas capture channel — frictionless intake

## Current Board State

### Done (5)
| Item | Completed |
|------|-----------|
| Playwright e2e tests | Today (Kade) |
| Private/shared/public access enforcement | Today (Kade) |
| Build visibility-aware middleware | Today (Kade) |
| Stabilize test foundation | Today (Kade) |
| Fuseki TDB2 verification | Today (Silas) |

### In Progress (1)
| Item | Owner |
|------|-------|
| Pod data backup | Kade |

### Todo — Priority Order (7)
| Item | Priority | Notes |
|------|----------|-------|
| CI pipeline enforcement | Next (after backup) | Security-critical, Kade briefed |
| YASGUI dashboard integration (ADR-004) | After CI | First feature work |
| Collection fitness tests | Flexible | Can slot after YASGUI or alongside |
| Product vision refinement | When ready | Wren-led |
| Ideas capture channel | After vision | Needs scoping |
| Developer Workspace / Cockpit (BL-001) | Future | Needs scoping |
| GitHub Projects → RDF/OWL bridge | Future | |

### Not on board yet (blocked)
- **First external harvester** — blocked on Jeff's ingestion depth decisions in `content-ingestion-matrix.md`
- **Conversational AI layer** — future, needs more content in the graph first

## Your Call

I've set the priority order based on architectural risk. The Todo column ordering on the board doesn't enforce sequence — that's your domain. If you want to reorder or reprioritize, go ahead. The architectural constraints are:

1. CI enforcement should come before more feature work (the test suite is security-critical now)
2. YASGUI before vis.js or WebVOWL (simpler, higher immediate value)
3. External harvester is genuinely blocked until Jeff makes ingestion depth decisions

Everything else is flexible.

— Silas
