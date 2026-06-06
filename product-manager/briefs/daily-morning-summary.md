# Daily Morning Summary — 2026-06-06

**HEADLINE:** Quality suite is fully blocked across all TS packages — `npm ci` needed before any test signal is trustworthy.

---

## OPS — YELLOW (Silas, 2026-06-03)
- YELLOW: 8 dead-code warnings in `chorus-hooks` (`load_role_sections`, `chorus_worktree_override`) — cleanup card open
- YELLOW: 20+ `/tmp` log paths in LaunchAgent plists — known gap, migrate to `~/Library/Logs/Chorus/`
- **RED: Board snapshot 56+ days stale (last: 2026-04-07)** — WIP visibility gone; daily refresh still not wired
- GREEN: CLAUDE.md, git dirty state, domain context (5 files, 4 days old, within threshold), CSC compliance

## QUALITY — RED (Kade, 2026-06-01)
- **All TS suites blocked**: `ts-jest` not found — `npm ci` not run post-clone in any package
- **platform/api build: 419 errors** — `@types/node` missing (was 0 errors 2026-05-29)
- Last known good: clearing 309p/53f · workflow-engine 62/62 · chorus-sdk 51/51 · pulse 57/57 · mcp-server 13 suites now failing
- Fix is one command: workspace-level `npm ci` or per-package installs

## YESTERDAY — High card volume across all three roles
- **Wren:** #3235 fixed nudge fold double-render (live bug, 279 pulse-surfaced affected), #3227 removed redundant demo gate (simplified accept flow), #2317 /pair-heartbeat-check skill, #3205, #3202
- **Silas:** #3250 deploy-everything autobins (werk-do-more + werk-finalize), #3243 deploy_canonical now ships TS services, #3247/#3242 HTML living-system docs, #3239/#3238/#3234/#3232/#3222
- **Kade:** #3219, #3240, #3241, #3236

## TODAY — Recommended priorities
1. **Unblock quality signal** — run `npm ci` workspace-wide; confirm test counts recover to 2026-05-29 baseline
2. **Board snapshot** — wire daily refresh (LaunchAgent or chorus-ops); 56-day gap is the ops red
3. **Dead-code cleanup** — burn down the 8 hooks warnings before they compound
4. **Perf baseline** — run `platform/scripts/perf-baseline.sh`, commit output; carried 2+ reviews

## BLOCKERS — Needs Jeff
- **Board snapshot staleness (RED):** No WIP visibility for 56 days. Someone needs to own the daily refresh wiring.
- **Quality suite (RED):** 5 days of zero test signal on a busy shipping week. If no one owns the `npm ci` fix, escalate now.
