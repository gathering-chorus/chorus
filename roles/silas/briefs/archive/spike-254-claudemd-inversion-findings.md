# Spike: CLAUDE.md Inversion — Findings (#254)

**From:** Silas (Architect)
**Date:** 2026-02-23
**Card:** #254
**Type:** [spike] — exit = learning + recommendation

## Finding

CLAUDE.md can drop from ~400 lines/role to ~30. MEMORY.md can be deleted entirely.

### Content audit (1,193 lines across 3 roles)

| Category | Lines | % | Replacement |
|----------|------:|--:|-------------|
| BOOTSTRAP (identity, principles, tone) | 288 | 24% | Stays — thin bootstrap |
| HOOK (already enforced by code) | 396 | 33% | Delete — hooks do the work |
| REDUNDANT (duplicated or dead) | 378 | 32% | Delete — lives elsewhere |
| LIVE (queryable at runtime) | 131 | 11% | `/werk init` + `/chorus search` |

### MEMORY.md audit (1,495 lines across 8 files)

Every entry exists in at least two other places: decisions.md, board state, state files, chorus index (27,492 messages). All test queries returned results. Fully deletable.

### The gap: context priming

MEMORY.md auto-injects into context. Chorus requires explicit search. Solution: `/werk init <role>` assembles and injects live context at session start. Built and ready to test.

## Recommendation

### Phase 1: Build `/werk init` (done)
New command assembles role context from live sources — board cards, workflows, recent decisions, briefs, state files. Returns structured text into the context window.

### Phase 2: Write thin bootstrap
~30 lines per role: identity + purpose + principles + tone + init instruction. Replace current CLAUDE.md.

### Phase 3: Delete MEMORY.md
After Phase 2 is validated across 3+ sessions per role.

### Phase 4: Simplify generator
`claudemd-gen.sh` goes from 43 fragments to ~3 (one per role). Or becomes unnecessary if bootstrap is hand-written.

## Dependency

#252 (services.json registry) is useful but not blocking. `/werk init` works without it — services can be added later as another context source.

## Risk

The thin bootstrap must give roles enough identity to know HOW to think, not just what to query. Principles and tone can't be dynamic — they're the lens. If we cut too much, roles lose their differentiated perspective.

## Exit criteria met

Spike produced: content audit, replacement mapping, gap analysis, `/werk init` prototype, phased recommendation.
