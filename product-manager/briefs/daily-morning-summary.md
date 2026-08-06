# Daily Morning Summary — 2026-08-06

**HEADLINE:** Domain context files and CLAUDE.md `shared/` fragments both breach the 7-day staleness threshold today — Wren and Silas refresh before session ends.

**OPS:** RED (Silas review 2026-08-05)
- GREEN: Repo clean (0 uncommitted changes)
- YELLOW: Hooks dead-code 2 warnings (3rd carry); /tmp plist migration stalled 7d; Dependabot #449/#443 **63d open**, Aug 1 batch 4d untriaged
- RED: CSC 148 `/tmp/` refs in `platform/scripts/` (improving −4, no card)
- RED **BREACH TODAY**: Domain context — all 5 files 7d stale; fragments — 23/24 `shared/` fragments 7d stale
- Top concern: Both thresholds land today; no refresh = dual RED carry into tomorrow

**QUALITY:** RED (Kade review 2026-08-06)
- 0 tests run — all 4 suites blocked: `ts-jest` preset missing, **day 56**
- Lint blocked (`@eslint/js`), **day 58**
- Build: 181 type errors (+0) — thirteenth consecutive stable day
- Root cause: `npm ci` at repo root — **58 days unresolved, no owner**

**YESTERDAY (08-05):** 6 cards shipped
- **#3751 (kade):** Stale werk-pin refused — never replayed; cycle start clears pin; integrity verified before attach
- **#3731 (silas):** 3 fail-open guards closed in `chorus-model-deploy` — CSV-header witness, SHACL crash→UNKNOWN, absent riot refuses without flag
- **#3758 (wren), #3749 (wren):** Context/identity work (details pending card lookup)
- **#3732 (silas), #3752 (silas):** Infra/protocol work

**TODAY:**
1. **Wren → domain context + shared/ fragments** — `domain-context-seeds.md` and remaining context files; bulk-refresh 23 fragments; **threshold is today**
2. **Silas → `domain-context-chorus.md`** — seventh day if missed
3. **Jeff → assign `npm ci` owner** — day 58, every suite and lint is dark with no ship horizon
4. **Jeff → Dependabot #449/#443** — 63d, merge-or-close call overdue
5. **Silas → CSC card** — 148 `/tmp/` refs, positive delta but no owner or target date

**BLOCKERS (needs Jeff):**
- **`npm ci` day 58, no owner** — all suites and lint dark; quality is structurally blind
- **Dependabot #449/#443 at 63d** — merge or close decision overdue
- **CSC no card** — 148 `/tmp/` refs in `platform/scripts/`, delta positive but drifting without a target
