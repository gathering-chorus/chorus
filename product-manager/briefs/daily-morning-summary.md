# Daily Morning Summary — 2026-08-11

**HEADLINE:** Domain-context breach is TODAY (7d threshold hit); CSC compliance spiked RED at 156 /tmp refs; npm ci enters day 63 still ownerless.

**OPS:** YELLOW/RED (Silas review 2026-08-10)
- RED: CSC compliance — 156 /tmp refs in platform/scripts/ (up from 96; possible #3805/#3804 regression or measurement artifact; Silas to recount)
- YELLOW: Domain-context files — **breach day today** (Aug 4 + 7d = Aug 11); chorus + infrastructure need update now
- YELLOW: Hooks dead-code day 8 carry (registration_json, owes_response_block); /tmp plist migration >10d stalled; Dependabot #449/#443 **69d open**
- GREEN: Repo clean; #3805 + #3804 shipped yesterday

**QUALITY:** RED (Kade review 2026-08-11)
- 0 tests run — all 4 suites blocked: `ts-jest` preset missing, **day 61**
- Lint blocked (`@eslint/js`), **day 63** (same root cause)
- Build: **202 type errors — STABLE** (no new regression)
- Root cause: `npm ci` at repo root — **63 days unresolved, no owner**

**YESTERDAY (08-10):** 5 cards shipped
- **#3811 (Kade):** chorus:Test registered Hydratable with bespoke hydrator; crawler dispatches declared hydrators per cycle; HydrationStamp on corpus; tagger + lifecycle proofs
- **#3810 (Wren):** 2 commits landed
- **#3809 (Silas):** Supporting ops card
- **#3808 (Kade):** Supporting card
- **#3776 (Silas):** Guard allow-set as generated projection of Principals (ADR-057); render-share-principals.sh; fitness row 8 diff enforced; 84/84 tests
- **Kade:** Removed stray curl cookie jar from roles/kade (backslash-pipe filename was breaking every crawler chorus:File batch since April)

**TODAY:**
1. **Wren → update domain-context-chorus.md + domain-context-infrastructure.md** — breach is today, not tomorrow
2. **Jeff → assign `npm ci` owner** — day 63, all quality dark, no horizon
3. **Jeff → Dependabot #449/#443** — 69d, decision overdue
4. **Silas → recount CSC refs clean from repo root** — confirm if 156 is regression or artifact
5. **Silas → remove dead-code hooks paths** — registration_json, owes_response_block; day 8 carry

**BLOCKERS (needs Jeff):**
- **`npm ci` day 63, no owner** — all 4 suites + lint structurally dark
- **CSC RED at 156** — possible regression from #3805/#3804; needs clean recount before escalation
- **Dependabot #449/#443 at 69d** — merge or close
