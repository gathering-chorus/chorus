# Daily Morning Summary — 2026-07-07

**HEADLINE:** #3622 clears SHACL-through-door writes (unblocks #3618); quality blackout hits day 26 with no owner.

**OPS:** RED — reviews 5d stale (last Silas: 2026-07-02)
- RED: Domain context 15d stale — `domain-context-chorus.md` still unrefreshed, Wren-owned
- RED: Stale WIPs #1704 (103d), #1759/#1791 (90d) — board unverifiable; planning debt accumulates
- RED: CSC /tmp — 58 scripts; `bridge-subscriber.js` still no card
- YELLOW: Hooks dead code 32d carry; LaunchAgent /tmp structural; CLAUDE.md fragments 15d lag
- GREEN: Repo clean

**QUALITY:** RED — reviews 5d stale (last Kade: 2026-07-02); 0 tests run, 0 lint passing
- All 4 suites blocked (`ts-jest` preset not found) — now **day 26** (clearing, workflow-engine, chorus-sdk, pulse)
- Lint blocked (`@eslint/js` missing) — now **day 28**
- Build: 150 TS type errors — now **day 16**, regression 2026-06-21, still unowned
- One fix unblocks tests + lint: `npm ci` at repo root

**YESTERDAY (since 07-06):** 2 cards, 2 PRs
- #3622 (wren) — `is_literal_term` now accepts typed literals (`"value"^^<datatype>`); was 422ing as injection; unblocks #3618's 11 `sh:minCount/maxCount` lines and all SHACL-through-door writes
- #3621 (kade) — trace visibility: /api/chorus/trace/:id folds spine events into chronological timeline; werk-test emits evidence on every run; 7d time window for fold

**TODAY:**
1. **Kade:** `npm ci` at repo root — ends 26d test + lint blackout in one command
2. **Silas:** Refresh ops review — 5d stale, RED carry items need current counts
3. **Kade:** Refresh quality review — 5d stale, coverage unknown since day 26
4. **Silas:** File July card for `bridge-subscriber.sh` /tmp runtime inbox (CSC RED, no card)
5. **Wren:** Refresh `domain-context-chorus.md` — 15d stale, flagged urgent last two summaries

**BLOCKERS (needs Jeff):**
- `npm ci` unrun 26d — who owns TS environment health on this repo?
- 150 TS type errors day 16 — still unowned; assign or it keeps aging
- WIPs #1704/1759/1791 at 90–103d — close, park, or redefine; WIP limits mean nothing at this age
