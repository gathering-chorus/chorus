# Memory Audit Layer — Design Brief

**Author:** Wren
**Date:** 2026-02-19
**Status:** Design — ready for team review
**Card:** TBD

## Problem

Conversations are audited (Loki via conversation-event-logger). Infrastructure is audited (chorus-audit, system-state.sh). But **role memory mutations are invisible** — when a decision gets written to `decisions.md`, when priorities shift in `backlog.md`, when a story gets captured in `stories.md`, there's no signal. We can't answer:

- "Did Wren capture that decision from the walkthrough?"
- "When did Kade last update current-work.md?"
- "Is Silas's architecture doc stale?"

Jeff's directive: the conversation in Slack is what matters. Logs should be invisible. But the *absence* of memory writes should be visible — staleness is a product failure.

## What Exists Today

| Layer | Instrumented? | How |
|-------|--------------|-----|
| Conversations | Yes | conversation-event-logger.ts → stdout → Promtail → Loki |
| Infrastructure | Yes | chorus-audit.sh, system-state.sh → chorus.log → Loki |
| Commits | Yes | git post-commit hook → chorus-log.sh → Loki |
| Briefs | Yes (partial) | post-commit hook detects brief files, emits brief_written |
| Board cards | Yes (partial) | board.sh/chorus-board.sh emit card events |
| **State file writes** | **No** | Not tracked anywhere |
| **Session ends** | **No** | session_start exists, session_end never fires |

## Design: Two-Layer Audit

### Layer 1: Git Hook Extension (Verification)

Extend the existing `post-commit` hook to detect state file modifications. When a commit touches a known state file, emit a `memory_write` event.

**Watched files:**

| Role | Files |
|------|-------|
| Wren | `product-manager/decisions.md`, `product-manager/backlog.md`, `product-manager/projects.md` |
| Silas | `architect/adr/*.md`, `architect/system-architecture.md`, `architect/ontology-status.md` |
| Kade | `engineer/current-work.md`, `engineer/tech-debt.md` |
| Shared | `messages/activity.md`, `messages/cost-log.md` |
| Memory | `~/.claude/projects/*/memory/MEMORY.md`, `*/stories.md` |

**Event format** (same structure as existing chorus-log.sh events):
```json
{
  "event": "memory_write",
  "role": "wren",
  "file": "decisions.md",
  "path": "product-manager/decisions.md",
  "commit": "abc1234"
}
```

**Pros:** Automatic, no role compliance required, catches all committed changes.
**Cons:** Only fires on commit — misses within-session writes that haven't been committed yet.

### Layer 2: CLAUDE.md Instruction (Real-Time Signal)

Add to each role's CLAUDE.md: after writing to a state file, emit a signal via chorus-log.sh.

```bash
../messages/scripts/chorus-log.sh memory_write role=wren file=decisions.md
```

**Pros:** Fires immediately on write, captures intent during session.
**Cons:** Relies on roles following instructions (compliance gap).

### Why Both Layers

- Layer 2 gives **real-time** visibility: "Wren just updated decisions.md"
- Layer 1 gives **verification**: "Did that update actually get committed?"
- Together they answer: "Is the team's memory current?" without requiring Jeff to check files

## Grafana: Memory Freshness Panel

Add one new row to the Chorus Activity Dashboard (row 7):

**Panel 1: Memory Freshness Table**
- One row per tracked file
- Columns: File, Role, Last Updated, Age (hours)
- Color coding: green (<24h), yellow (24-48h), red (>48h)
- LogQL: `{appName="chorus-events"} | json | event="memory_write" | line_format "{{.role}} {{.file}} {{.timestamp}}"`

**Panel 2: Memory Write Timeline**
- Time series of memory_write events per role
- Shows activity patterns: which roles are actively maintaining state vs. going quiet

**Panel 3: Staleness Alerts**
- Alert rule: if `decisions.md` hasn't been updated in 48h, fire warning
- Alert rule: if `activity.md` hasn't been updated in 24h, fire warning
- Alert rule: if any role's state files all go 72h+ without update, fire critical

## Session End Events

While we're extending the hook infrastructure, also add `session_end` events. The close-out protocol already includes committing and pushing — the post-commit hook can detect session-close commits (pattern: commit message contains "session" or "close-out" or "eod") and emit:

```json
{
  "event": "session_end",
  "role": "wren",
  "commit": "abc1234"
}
```

Combined with existing `session_start`, this gives us session duration tracking.

## Implementation Scope

| Item | Effort | Owner |
|------|--------|-------|
| Extend post-commit hook (memory_write detection) | 30min | Wren |
| Extend post-commit hook (session_end detection) | 15min | Wren |
| Add chorus-log.sh instruction to 3 CLAUDE.md files | 15min | Wren |
| Grafana dashboard row (3 panels) | 45min | Silas (or Wren) |
| Staleness alert rules | 15min | Silas |
| **Total** | **~2 hours** | |

## What's NOT in Scope

- Decision propagation tracking ("did the team act on DEC-023?") — future
- Cost-per-decision metrics — future
- Automated staleness remediation — future (roles get alerted, humans decide)
- Memory diffing ("what changed in decisions.md?") — git diff exists, no need to duplicate

## Cost

Minimal. chorus-log.sh writes one JSON line per event. Promtail already scrapes. Loki already indexes. Three new Grafana panels. No new services, no new containers, no new Docker resources.

## Kill Switch

Remove the hook extension lines from `post-commit`. Remove the CLAUDE.md instruction lines. Dashboard panels show "no data" — harmless.

## Success Criteria

1. After a session where Wren makes a decision, Grafana shows a `memory_write` event for `decisions.md`
2. If Kade hasn't updated `current-work.md` in 48h, an alert fires
3. Jeff can glance at the Memory Freshness panel and see which roles are maintaining state vs. going quiet
4. Total implementation cost under $5 in Claude API calls
