# Daily Morning Summary — 2026-03-31

**HEADLINE:** ts-jest wiped from 3 packages (board-client, workflow-engine, slack-bridge) — 183 tests went dark overnight and photos Fuseki lost ~18K records; both need same-day resolution.

---

**OPS** 🟡 YELLOW (Silas — 2026-03-29)
- 4 yellows, 4 greens. No reds.
- Top concern: `chorus-hooks/target/` (321 untracked files) likely not in `.gitignore` — confirm and add.
- Secondary: 8 cargo warnings (4 auto-fixable via `cargo fix --bin chorus-hooks`); /tmp usage in 6 plists + 7 scripts is accepted risk.

**QUALITY** 🔴 RED (Kade — 2026-03-29)
- chorus-sdk: GREEN — 6/6 passed. Everything else broken.
- board-client, workflow-engine, slack-bridge: runner broken — `ts-jest` preset not found.
- Delta: workflow-engine was 61/61 green, slack-bridge 60/60 green — both regressed to zero.
- Root cause: node_modules cleared or ts-jest dep removed across messages packages.
- Fix: `cd messages && npm install` (or restore ts-jest per package).

**YESTERDAY** — 8 cards shipped (2026-03-30)
- #1879 Silas — log-first gate: require log inspection before coding a fix
- #1862 Silas — guard consolidation: merge app_state_guard into infra_guardrails, 22 hooks audited
- #1861 Silas — session JSONL cache: 1 read/prompt cycle (was 5)
- #1868 Silas — session-start disk check fix: df→Finder free space on APFS
- #1882 Kade — doc-catalog filters, dedup, life loop, domains group
- #1885 Kade — Clearing domain sort, WIP/Blocked first, aging flags
- #1871 Wren — deprecate formal /docs into doc-catalog, nav-tree trimmed (DEC-108)
- #1883 Wren — 23-domain page index + design artifact linking convention (DEC-109)

**TODAY** (recommended order)
1. **Kade** — restore ts-jest (`cd messages && npm install`); unblock 183 tests before any new code
2. **Silas/Kade** — photos Fuseki: ~18K records lost in rebuild, predicates unnormalized — assess scope, card recovery if not already carded
3. **Silas** — `cargo fix --bin chorus-hooks`, confirm `chorus-hooks/target/` in `.gitignore`
4. **All** — #1652 iPhone extraction (Silas WIP) and #1675 nudge filtering remain in WIP; no new WIP until ts-jest is green

**BLOCKERS** — needs Jeff's attention
- 🔴 ts-jest regression: 3 suites dark, 183 tests not running — assign Kade now, not later
- 🔴 Photos Fuseki data loss: ~18K records missing post-rebuild — is this recoverable from source graph or re-harvest needed?
