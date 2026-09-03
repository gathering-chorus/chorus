# Daily Morning Summary — 2026-09-03

**HEADLINE:** chorus-hooks compile error enters day 5 unpatched — Silas owns a one-line fix; ship it this session.

## OPS — 🔴 RED
- **Hooks**: `cargo check` fails (E0615, `signal_witness.rs:55`) — day 4 as of yesterday's review (day 5 now). Fix: `(*info).si_pid()` and `(*info).si_uid()`. Escalation line is today.
- **chorus-api**: OFFLINE day 4 — WIP board unverifiable. Git log as proxy until restored.
- **Domain-context**: 2 files behind shipped cards — `chorus.md` (#4040, pipeline kinds) and `infrastructure.md` (#4057, deep-health overhaul). Kade to update.
- **CLAUDE.md fragments**: 7-day staleness threshold met — Wren to refresh shared fragments today.
- **/tmp in plists**: 17+ LaunchAgent plists log to /tmp (lost on reboot) — existing card, no emergency.

## QUALITY — 🔴 RED
- **0 tests running** across all suites. Root blocker: `npm ci` unrun — **day 82** (ts-jest suites), **day 84** (lint). No owner.
- TS error counts unchanged: pulse **952**, clearing 239, mcp-server/sdk 28 each, workflow-engine 11.
- mcp-server: 31 suites blocked separately on babel TS preset missing.

## YESTERDAY (2026-09-02)
- 9 cards shipped across all three roles: #4065, #4063, #4071 (silas); #4073, #4078 (kade); #4075, #4077, #4028 (wren); #4079 (silas).
- **#4078 notable**: werk-test was counting skips as failures — fixed. "839 pass, 1 fail" days were false alarms.
- **#4057 notable**: deep-health now reads loaded agents from launchctl, not log files on disk — 36 findings → 12 real.

## TODAY — Recommended Priorities
1. **Silas**: Fix hooks compile error — `(*info).si_pid()`, `(*info).si_uid()` in `signal_witness.rs:55`. Done in minutes.
2. **Anyone**: Run `npm ci` at repo root + sub-packages — unblocks 82 days of zero tests with a single command.
3. **Wren**: Refresh shared CLAUDE.md fragments (7-day threshold now met).
4. **Kade**: Update `domain-context-chorus.md` and `domain-context-infrastructure.md`.
5. **Silas**: Restore chorus-api; file LaunchAgent /tmp migration card if not tracked.

## BLOCKERS — Jeff's Attention
- **Hooks compile, day 5**: One-line fix. If it doesn't land this session, needs explicit escalation.
- **npm ci, day 82**: Zero tests for 82 days is a quality dead zone. Needs an owner and a done-by date today.
