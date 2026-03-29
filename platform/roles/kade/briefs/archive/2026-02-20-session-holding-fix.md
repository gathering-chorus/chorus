# Brief: Session-Holding Fix — Path A + Path C

**From:** Wren (PM)
**To:** Kade (Engineer)
**Date:** 2026-02-20
**Priority:** P1
**Decision:** DEC-028
**Board:** Card #80 (reusing demo card — will update title)

---

## Context

Jeff holds sessions open for days because closing feels like losing context. Decisions made in Slack don't flow back to state files. The @team conversation confirmed all three roles agree on the fix: Path A + Path C, built in sequence. ~3 hours total.

---

## Path A: Bidirectional Bridge — Decision Capture (~2 hours)

**What:** When a `[DECISION]` tag appears in a Slack message, the bridge writes it to `messages/decisions-backlog.md`. Each role checks this file on session start and processes new entries.

**Where to build:** `messages/slack-bridge/src/commands.ts` — add a new detection pattern alongside `@read-briefs`. Or add to the existing message processing pipeline in `index.ts`.

**Detection rule:** Any message in any monitored channel containing `[DECISION]` (case-insensitive).

**Write target:** `/team/messages/decisions-backlog.md` (needs a writable mount — same pattern as briefs and nudges).

**File format:**
```markdown
## [DECISION] 2026-02-20 21:36 — #all-gathering
**From:** Jeff (or role name if bridge message)
**Text:** [The full message text]
**Status:** pending

---
```

Append-only. Each role's session-start routine reads this file, processes entries marked `pending`, updates their own state files (decisions.md, backlog.md, etc.), and marks entries as `processed-by-{role}`. When all three roles have processed an entry, it can be archived.

**Docker mount needed:** Add to `messages/slack-bridge/docker-compose.yml`:
```yaml
# Decisions backlog — WRITABLE
- ../../messages:/team/messages-decisions:rw
```
Or better: mount just a `decisions/` subdirectory writable, same as briefs:
```yaml
- ../../messages/decisions:/team/messages/decisions
```
And write to `/team/messages/decisions/backlog.md`.

**Test:** Post a message with `[DECISION]` tag in #all-gathering. Verify it appears in the backlog file within one poll cycle.

---

## Path C: next-session.md Close-Out Enhancement (~1 hour)

**What:** During session close-out, each role writes a `next-session.md` file in their directory summarizing what's waiting for the next session. This removes the fear of losing context when closing.

**Where to build:** This is a Claude Code convention, not bridge code. Add it to each role's CLAUDE.md close-out checklist. But the bridge can help: auto-generate `next-session.md` from recent Slack activity when it detects a standup post (which signals session close).

**File location:** `engineer/next-session.md`, `architect/next-session.md`, `product-manager/next-session.md`

**Format:**
```markdown
# Next Session — Kade
**Generated:** 2026-02-20 21:40

## Pending briefs
- 2026-02-20-session-holding-fix.md (from Wren, P1)

## Recent decisions
- DEC-028: Session-holding fix approved

## In-progress work
- SMS capture enhancements (card #75)

## Commitments from Slack
- Document restart vs deploy decision tree in app-state.sh
- Add Whisper pre-build to tech-debt.md

## Notes
[Any context the role wants to preserve for next session]
```

**Bridge auto-generation (optional enhancement):** When the bridge sees a standup post (contains `#standup` or posted to #standup channel), it generates `next-session.md` for that role by scanning:
- Their briefs/ directory (pending items)
- Recent `[DECISION]` entries from the backlog
- Their commitments from the nudge log
- Recent @team conversation topics

This part is optional — the manual version (role writes it during close-out) works fine as a first pass.

---

## Build Order

1. **Path A first** — decision capture is the higher-value fix
2. **Path C second** — context continuity, simpler build
3. Both paths ship today if clean

## Acceptance Criteria

- [ ] `[DECISION]` messages captured to backlog file within one poll cycle
- [ ] Backlog file format is parseable (roles can read and process it)
- [ ] Docker mount is writable from bridge container
- [ ] `next-session.md` template exists and is documented in close-out checklist
- [ ] Screenshot posted to #kade before declaring done

---

— Wren
