# Daily Morning Summary — 2026-06-09

**HEADLINE:** Quality is fully dark — all 4 TS suites blocked and 140 new TypeScript errors — needs same-day fix before any code signal is trustworthy.

---

## OPS — YELLOW (Silas, 2026-06-08)
- GREEN: CLAUDE.md clean, git dirty state, CSC compliance, domain context (5 files, 3 days old, within threshold)
- YELLOW: 8 dead-code warnings in `chorus-hooks` (`load_role_sections`, `chorus_worktree_override`) — unresolved
- YELLOW: 18 LaunchAgent plists using `/tmp` log paths — known gap, low urgency
- YELLOW: No perf-baseline snapshot on host — scripts exist, never run
- No reds. Board WIP not queryable from worktree — verify Vikunja directly.

## QUALITY — RED (Kade, 2026-06-08)
- **All 4 suites BLOCKED** — `ts-jest` missing from node_modules; 0 tests run (was 492 passing on 2026-06-06)
- **Build RED** — 140 TypeScript errors (was 0 on 2026-06-06); new regression, root cause unknown
- **Lint RED** — persistent; `@eslint/js` missing at root (unchanged since 2026-06-06)
- Root cause hypothesis: `npm ci` stripped devDependencies across packages
- Fix path: `npm ci` at repo root + each of `directing/clearing`, `platform/workflow-engine`, `platform/chorus-sdk`, `platform/pulse`

## YESTERDAY — 6 cards merged (Jun 8)
- **Wren:** #3291, #3293, #3275
- **Silas:** #3285, #3278 — #3277 notable: `chorus_werk` now detaches from MCP request (fixes client-transport drop class)
- **Kade:** #3287, #3254 — #3270 also landed
- **Wren infra:** #3279 — untracked `workflow-engine/dist` self-loop symlink (was recurring board-outage root cause)

## TODAY — Recommended priorities
1. **Restore quality signal (P0)** — `npm ci` workspace-wide; confirm suites recover; investigate 140 TS errors separately
2. **Root-cause TS regression** — 0→140 errors since Jun 6; may be a type-breaking commit; bisect before more code ships
3. **Perf baseline** — run `perf-baseline.sh`, commit output; carried 3+ reviews with no action
4. **Hooks dead-code** — resolve warnings or add `#[allow(dead_code)]`; small lift, cleans ops yellow

## BLOCKERS — Needs Jeff
- **Quality suite (RED):** Zero test signal since Jun 6 on an active shipping week. `npm ci` fix is mechanical but someone must own it today.
- **140 TS errors (RED):** New regression with unknown root cause. If not a node_modules artifact, a type-breaking change shipped without detection.
