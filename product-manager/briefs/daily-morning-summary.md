# Daily Morning Summary — 2026-07-29

**HEADLINE:** 3 cards landed yesterday (model-version, value-stream contracts, chorus-oidc); `npm ci` hits day 50 and WIP cards rot at 112d — both need decisions today.

**OPS:** YELLOW/RED (Silas, 2026-07-28)
- GREEN: Git clean (0 uncommitted changes)
- YELLOW: Dead-code warnings at 1 (improving); 34 plists with `/tmp/` refs (no movement); CLAUDE.md role fragments still unrefreshed (5d lag)
- RED: CSC compliance static — 37 sh files with `/tmp/` refs, check-path mismatch unresolved; WIP cards #1759/#1791 at **112d** — escalating daily; all 5 domain context files **5d stale**, chorus context ~**100d** with 3 cards shipped yesterday into it

**QUALITY:** RED (Kade, 2026-07-29)
- All 4 suites blocked: `ts-jest` preset not found — **day 48**; lint blocked (`@eslint/js`) — **day 50**
- Build: **181 type errors (+0)** — stable, third consecutive day; no new regression
- Primary fix: `npm ci` at repo root — **50 days unresolved**

**YESTERDAY (07-28):** 3 cards shipped
- **#3704 (wren):** Model-version convention — `chorus:modelVersion` + `supersededBy` properties, Vertebra tagged v1→ValueStreamStep, owl-api projects modelVersion on every envelope
- **#3707 (kade):** Value-stream chunk contracts — stageOrder 1–6 pinned, tree depth≥4, v1 410 stays retired; athena-health catalog cleaned up
- **#3688 (silas):** chorus-oidc expansion + owl-api integration (+459 lines); spine events schema update

**TODAY:**
1. **Jeff → `npm ci`:** Day 50 milestone; quality fully dark for 7+ weeks — needs owner or go/no-go
2. **Jeff → #1759/#1791:** 112d in WIP; close or archive — board noise blocks backlog reads
3. **Silas:** `domain-context-chorus.md` critical at ~100d; 3 Chorus cards landed yesterday — update now
4. **Wren + Kade:** Role fragment refresh in CLAUDE.md — 5d since last touch

**BLOCKERS (needs Jeff):**
- **`npm ci` day 50** — all test suites and lint blind since early June; coverage unknown
- **#1759/#1791 at 112d** — longest-stale WIP on the board; must decide today
