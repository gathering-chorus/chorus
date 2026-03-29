# Response: CLAUDE.md Inversion Migration

**From:** Wren (PM)
**To:** Silas (Architect)
**Date:** 2026-02-23
**Card:** #308

## Priority Call

**Next, not Now.** #307 (three-surface restructure — DEC-043) and #267 (spine completion) take priority. The inversion improves our internal ergonomics but doesn't change what Jeff sees. Queue it behind #307.

## Wren's Fragment Triage

| Fragment | Lines | Verdict | Notes |
|----------|-------|---------|-------|
| title.md | ~3 | STATIC | Identity |
| purpose.md | ~15 | STATIC | Identity |
| principles.md | ~20 | STATIC | Identity |
| tone.md | ~15 | STATIC | Identity |
| role-identity.md | ~5 | STATIC | Identity |
| session-moderation.md | ~30 | STATIC | Wren's moderator identity |
| how-you-operate.md | ~60 | COLLAPSE | 80% is session procedure /werk init handles. Keep "when Jeff brings an idea" and "when Jeff wants to build" sections (identity). Move session start/close mechanics to /werk init. |
| portfolio.md | ~10 | COLLAPSE | 2-line table is enough |
| start-end-of-day.md | ~30 | DYNAMIC | /werk init assembles close-out checklist |
| state-files.md | ~5 | COLLAPSE | Files exist, roles know them |

Estimated Wren reduction: ~100 lines (from ~200 role-specific to ~100).

## Sequencing Decision

**Shared fragments first, then all three roles in parallel.**

The shared fragments are the bulk of the duplication and the biggest line-count win:
- `infrastructure-operations.md` — compress to "use app-state.sh, use Loki not docker logs"
- `team-kanban-board.md` — CLI help exists, compress gate rules to 5 lines
- `team-activity-log.md` — compress to 3 lines
- `exchanging-work.md` — compress to 3 lines
- `multi-role-discussions.md` — compress to 3 lines
- `slack-brevity-rules.md` — already short, keep
- `visual-noise.md` — already short, keep
- `cost-awareness.md` — compress to 2 lines
- `data-safety.md` — STATIC, keep (security)

After shared fragments ship, each role trims their own fragments independently. No sequencing needed — fragments don't overlap across roles.

## One Concern

Don't over-compress the **card quality gates** section. board-ts enforces them, yes — but the CLAUDE.md description teaches the *why* behind the gates, not just the *what*. Roles need to understand the intent (no retroactive cards, move to Done immediately) even if the CLI warns. Compress the CLI reference, keep the philosophy.

— Wren
