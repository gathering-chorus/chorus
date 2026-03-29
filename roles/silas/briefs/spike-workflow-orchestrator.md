# Spike: Manifest as Workflow Orchestrator

**Card**: #118
**Author**: Silas
**Date**: 2026-02-21
**Status**: Spike complete — ready for discussion

## The Problem

Jeff plays tag between roles. A decision happens in the Clearing or in conversation, then Jeff has to:
1. Open a Silas session → "do the architecture piece"
2. Open a Kade session → "now implement it"
3. Open a Wren session → "verify it shipped"

Jeff is the relay. That's the anti-pattern we named in DEC-022. The system should move work through the team, not Jeff.

## What Exists Today (Coordination Primitives)

| Primitive | Status | Location |
|-----------|--------|----------|
| Decision capture (Clearing) | ✅ Working | `chorus/clearing/` — DECISION markers auto-parsed, structured return |
| Decision capture (Slack) | ✅ Working | Bridge → `messages/decisions/backlog.md` |
| Brief routing | ✅ Working | Write to recipient's `briefs/` dir, signal via Slack |
| Commitment extraction | ✅ Working | `commitment-brief-writer.ts` — auto-extracts "I'll do X" from Slack |
| Card management | ✅ Working | `board-ts` CLI — create, move, comment, block/unblock |
| Pipeline pattern | ✅ Working | `claudemd-gen.sh --pipeline` — validate → execute → verify → report |
| Role definitions | ✅ Working | Vertical/horizontal ownership (DEC-030) |

## What's Missing

### 1. Decision → Work Sequence
No mechanism to say: "This decision requires Silas to do X, then Kade to do Y, then Wren to verify Z — in that order."

Today decisions sit in `backlog.md` with status `pending` and nothing drives them forward.

### 2. Cross-Role Dependency Graph
Board has `block`/`unblock` but no way to express "Card A (Silas) must complete before Card B (Kade) can start." Dependencies are implicit — carried by Jeff's memory.

### 3. Handoff Triggers
When Silas finishes his piece, nothing signals Kade to start. The brief lands in `briefs/` but Kade only reads it at session start — and only if Jeff opens a Kade session.

### 4. Orchestration State Machine
No persistent tracking of: "Which decisions are pending → in-flight → verified?" No state transitions. No audit trail of the flow.

## Proposal: The Workflow Manifest

A **workflow manifest** is a JSON document that describes a decision's execution plan:

```json
{
  "id": "WF-001",
  "decision": "DEC-036: Migrate WordPress to docker-compose",
  "source": "clearing:2026-02-21T17-30-57",
  "created": "2026-02-21T12:30:00Z",
  "status": "in_progress",
  "steps": [
    {
      "seq": 1,
      "role": "silas",
      "action": "Update system-architecture.md, write ADR-015",
      "card": null,
      "status": "completed",
      "artifacts": ["architect/adr/ADR-015-iac-discipline.md"],
      "completed_at": "2026-02-21T11:00:00Z"
    },
    {
      "seq": 2,
      "role": "kade",
      "action": "Migrate wordpress-blog from Terraform to docker-compose",
      "card": null,
      "status": "pending",
      "blocked_by": [1],
      "artifacts": []
    },
    {
      "seq": 3,
      "role": "wren",
      "action": "Verify migration, update roadmap",
      "status": "pending",
      "blocked_by": [2],
      "artifacts": []
    }
  ],
  "verification": {
    "role": "silas",
    "criteria": "docker-compose up starts WordPress, data persists across restarts"
  }
}
```

### How It Would Work

1. **Decision captured** (Clearing or Slack) → Jeff or any role says "orchestrate this"
2. **Workflow created** → Steps defined with role assignments, sequence, dependencies
3. **Cards auto-created** → Each step becomes a board card, linked and sequenced
4. **Handoff signals** → When step N completes, a brief is auto-routed to step N+1's role with context + artifacts
5. **Session start picks it up** → `session-start.sh` checks for pending workflow steps assigned to this role
6. **Status dashboard** → Visualization shows workflow progress across roles

### What This Replaces

| Today (Jeff as relay) | Tomorrow (orchestrated) |
|----------------------|------------------------|
| Jeff opens Clearing, decision made | Same |
| Jeff opens Silas session, says "do the arch piece" | Silas's session-start shows: "WF-001 step 1 ready — update architecture for WordPress migration" |
| Jeff waits, opens Kade session, says "Silas is done, now implement" | Kade's session-start shows: "WF-001 step 2 ready — Silas completed ADR-015, brief attached" |
| Jeff opens Wren session to verify | Wren's session-start shows: "WF-001 step 3 ready — verify WordPress migration" |

Jeff still **directs** (sets the decision, approves the plan). But he doesn't **relay**.

## What's Buildable Now vs. Later

### Now (no architectural change needed)
- **Workflow manifest schema** — the JSON format above
- **Workflow creator** — takes a decision + role sequence, generates the manifest
- **Session-start integration** — `session-start.sh` scans for pending workflow steps
- **Step completion hook** — when a role marks a step done, auto-routes brief to next role
- **Workflow status command** — `workflow status WF-001` shows progress

### Later (needs Chorus card #15 — autonomous role activation)
- **Autonomous handoff** — role sessions start automatically when their step is unblocked
- **Real-time status dashboard** — live visualization of workflow progress
- **Parallel steps** — multiple roles working simultaneously on independent steps
- **Timeout/escalation** — if a step stalls, escalate to Jeff

## The Key Constraint

Today, roles only activate when Jeff opens a session. The workflow manifest can **prepare** work (briefs, cards, signals) but can't **activate** a role. That's Chorus #15.

However, even without autonomous activation, the manifest eliminates the "what am I supposed to do?" problem. When Jeff opens any role's session, it immediately knows what's next instead of Jeff having to explain the context.

## Recommendation

Build the "Now" pieces as a thin orchestration layer:
1. Workflow manifest schema + creator (~1 hour)
2. Session-start integration (~30 min)
3. Step completion + handoff (~1 hour)
4. Status command (~30 min)

This gives Jeff immediate relief from the relay pattern. Autonomous activation (Chorus #15) makes it fully self-driving later, but even without it, the workflow manifest is a significant improvement.

## Connection to Existing Work

- **CLAUDE.md pipeline** (`claudemd-gen.sh --pipeline`): Narrow instance of this pattern. The orchestration, step tracking, and card attachment patterns port directly.
- **Commitment brief writer**: Already extracts role commitments. Can feed workflow step creation.
- **Clearing decisions**: Already structured. Can trigger workflow creation.
- **Board-ts**: Already supports card creation, comments, blocking. Workflow steps map to cards.

The foundation is there. The workflow manifest is the missing layer that connects decisions to sequenced execution.
