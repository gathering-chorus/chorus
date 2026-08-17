# Daily Morning Summary — 2026-08-17

**HEADLINE:** Three Wren deadlines are now overdue (CLAUDE.md v1.6.1+, zombie card #1962, domain-context content) — all were due yesterday and none landed; address before anything else.

---

**OPS:** RED (4 red, 2 yellow, 1 green — escalation from yesterday)
- RED: CLAUDE.md fragment — Wren's EOD Aug 16 deadline blown; PROTOCOL_VERSION still 1.6.0; TODAY
- RED: Zombie card #1962 — 126d in `building`; Wren asked to close yesterday, not done; TODAY
- RED: Domain-context breach — day 4; chorus + infrastructure content months stale; Wren; TODAY
- RED: CSC compliance — 70 platform/scripts + 6 role scripts carry `/tmp/`; no migration card; no movement
- YELLOW: Hooks dead-code — warnings 4→7, day 14 deadline blown; Wren/Silas to clean or card TODAY
- YELLOW: LaunchAgent `/tmp` refs — 17 plists, day 15+; Silas to open migration card
- GREEN: Git working tree clean; 41 commits landed since Aug 14

**QUALITY:** RED (all suites blocked — day 67 / lint day 69)
- Tests: 0 run — 4 suites blocked (`ts-jest preset not found`), day 67
- Lint: blocked (`@eslint/js`), day 69
- Build: 227 type errors (+1 from 226); +9 spike did not repeat, trend still upward
- Root cause: `npm ci` unrun at repo root — **69 days, no owner**

**YESTERDAY (2026-08-16):** Active shipping day — 9 cards landed across all roles
- wren: #3907 — relay tunnel host fix + Clearing reads relay (durable cursor replay, visible hole for Jeff)
- wren: #3909 — unreachable conditions deleted (type-safe cleanup)
- wren: #3889 — land-blocker cleared: cursor path validated, refuses non-cursor files
- wren: #3906 — landed
- silas: #3904, #3908, #3902 — ops/infra cards (6 commits across 3 PRs)
- kade: #3905 — landed

**TODAY: Recommended priorities**
1. **Wren** — bump CLAUDE.md fragment to v1.6.1+ (overdue)
2. **Wren** — close zombie card #1962 from state.json (overdue)
3. **Wren** — content-refresh domain-context-chorus.md + domain-context-infrastructure.md (overdue, d4 breach)
4. **Wren** — clean dead-code set in hooks (7 warnings, deadline blown) or file a card with new date
5. **Silas** — file CSC migration card (70+ `/tmp/` refs) and LaunchAgent migration card

**BLOCKERS (needs Jeff):**
- `npm ci` — **day 69**, no owner assigned; all tests + lint dark; lane needs an owner or a formal decision to close
- Wren has 3 consecutive missed deadlines — if pattern continues, consider re-prioritizing Wren's card load
