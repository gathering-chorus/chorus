# Daily Morning Summary — 2026-08-10

**HEADLINE:** npm ci enters day 62 with no owner and all tests dark; domain-context files breach the 7d threshold tomorrow unless Wren updates them today.

**OPS:** YELLOW (Silas review 2026-08-09)
- GREEN: Repo clean; CSC compliance improved −60 refs to 96 (−38%, #3796 driver); CLAUDE.md fragments still absent
- YELLOW: Hooks dead-code day 7 carry; /tmp plist migration stalled >9d; Dependabot #449/#443 **68d open**; domain-context files **6d — breaches 7d threshold tomorrow**
- Top concern: domain-context-chorus.md + domain-context-infrastructure.md need Wren update **today**

**QUALITY:** RED (Kade review 2026-08-10)
- 0 tests run — all 4 suites blocked: `ts-jest` preset missing, **day 60** (no movement)
- Lint blocked (`@eslint/js`), **day 62** (same root cause, no movement)
- Build: **202 type errors — STABLE** (no new regression; yesterday's +13 has not worsened)
- Root cause: `npm ci` at repo root — **62 days unresolved, no owner**

**YESTERDAY (08-09):** 4 cards shipped — guard hardening
- **#3804 (Silas):** Path-mounted guard instance for llug.com/chorus — new plist, shared OIDC state, self-healing client (84 tests)
- **#3805 (Silas):** Guard-authored URLs speak the visitor's frame — pub() prefixes at emission only (99 tests)
- **#3765 (Silas):** Supporting guard card
- **#3774 (Kade):** Supporting card

**TODAY:**
1. **Wren → update domain-context-chorus.md + domain-context-infrastructure.md** — breaches 7d tomorrow; action blocks Silas carry resolution
2. **Jeff → assign `npm ci` owner** — day 62, all quality dark; no horizon; this is the longest-running unowned blocker
3. **Jeff → Dependabot #449/#443** — 68d open, merge-or-close decision now critical
4. **Silas → remove dead-code paths** (registration_json, owes_response_block) — hooks day 7 carry
5. **Silas → land perf-baseline nightly JSON** — disk delta tracking still blind

**BLOCKERS (needs Jeff):**
- **`npm ci` day 62, no owner** — all 4 suites + lint structurally dark; quality invisible
- **Dependabot #449/#443 at 68d** — overdue decision
