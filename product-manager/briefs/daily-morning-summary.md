# Daily Morning Summary — 2026-06-24

**HEADLINE:** Quality tooling has been dark for 15 days while yesterday's sprint shipped ~10 cards — one `npm ci` at repo root restores test visibility across all packages.

---

**OPS:** YELLOW overall (2 REDs)
- RED: Domain context drift — chorus 66 days stale, infra 91 days; both domains had heavy shipping this week. Wren to update today.
- RED: Stale WIP cards — 2 cards untouched 78 days; board snapshot still inaccessible, status unknown.
- YELLOW: 22-day carry on hooks dead-code (8 warnings), LaunchAgent /tmp refs (30+ plists), CLAUDE.md fragment lag (4 days), CSC /tmp audit pending.
- GREEN: Git working tree clean.

**QUALITY:** RED across all packages
- 4 test suites BLOCKED: `ts-jest` preset not found (node_modules incomplete) — day 15.
- Build: 150 TypeScript type errors, unchanged — day 3 regression from Jun 21.
- Lint: BLOCKED (`@eslint/js` not found) — day 17.
- Root cause: `npm ci` at repo root fixes tests + lint simultaneously.

**YESTERDAY:** Heavy shipping sprint — ~10 cards merged.
- Wren: #3570 (domains.* spine, 56+ tests), #3432 (chunk add-time fix), #3391 (Chorus-as-platform shaping), #3575 (board-as-domain spike), #3567.
- Silas: #3573, #3574, #3566, #3579.
- Kade: #2819.

**TODAY (recommended priorities):**
1. Run `npm ci` at repo root — unblocks all 4 test suites and lint immediately. (Kade or Jeff)
2. Resolve 150 TS type errors — regression is 3 days old, unowned. (Kade)
3. Update `domain-context-chorus.md` + `domain-context-infrastructure.md` — both RED overdue. (Wren)
4. Run claudemd pipeline — role CLAUDE.md not regenerated after sprint. (Wren)

**BLOCKERS (needs Jeff):**
- Quality dark Day 15 (RED): `npm ci` needs someone with a working npm environment — who runs it?
- Board state unknown (RED — 78d): "Framework service design — OWL entity model" + "Restore chorus product boundary" — closed or stalled?
- LaunchAgent /tmp migration requires host LaunchAgent access; no path forward without it.
