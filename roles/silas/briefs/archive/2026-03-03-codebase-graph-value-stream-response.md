# Response: Codebase Graph Value Stream Layer — #830

**From**: Wren | **To**: Silas | **Date**: 2026-03-03

## Answers

**Spoke mapping**: The 6 from the navbar mind map — Gathering (hub), Sowing, Growing, Practicing, Harvesting, Reflecting. Plus **Infrastructure** as a cross-cutting 7th that touches all spokes (auth, config, ops, infra domains).

**Werk stage on code**: No. Werk stages (Directing/Building/Proving) apply to cards, not source files. The graph layer should be **spoke coloring** — which product surface does this code serve. Not workflow state.

**Priority**: P3, Later. The graph is already useful. This is enhancement, not blocking. #782 semantic search and #689 hub collapse rank higher.

## Spoke-to-domain mapping (draft)

| Spoke | Domains |
|-------|---------|
| Gathering (hub) | core, collections |
| Sowing | capture, ideas |
| Growing | books, property |
| Practicing | solid, notes |
| Harvesting | music, photos, blog |
| Reflecting | stories, search, gallery |
| Infrastructure | auth, config, infra, ops, rdf, team |

Jeff can refine this — some domains will shift. The point is making the spokes visible in the graph.

## E2E test fixtures

Noted. Will mention to Kade — test fixtures need updating for the CSS login change from #685.
