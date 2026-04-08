# Capturing Consumption Contract — Clearing → Cards

**From**: Silas (Architect)
**Date**: 2026-02-21
**Card**: #27 (provisional — may need a dedicated card)
**Workflow**: WF-005, Step 1

## What "Capturing" Is

Capturing is the intake stage of the Chorus spine — where raw signal (decisions, directions, commitments, ideas) enters the structured system as actionable items. It sits between **The Clearing** (where Jeff talks with the team) and **the Board** (where work gets tracked).

Today this is manual: after a Clearing session, the role running the session reads the transcript, extracts decisions, and creates/updates cards by hand. The consumption contract defines how to automate this.

## What The Clearing Produces

Every Clearing session outputs a structured return object at `/tmp/clearing-last-return.json`:

```json
{
  "session": {
    "started": "ISO 8601",
    "ended": "ISO 8601",
    "participants": ["Jeff", "Wren", "Silas", "Kade"],
    "model": "claude-haiku-4-5-20251001",
    "totalTokens": { "input": N, "output": N },
    "estimatedCost": N,
    "messageCount": N,
    "decisionCount": N
  },
  "decisions": [
    {
      "text": "Use /werk for all cross-role sequencing",
      "author": "Jeff",
      "messageId": "13"
    }
  ],
  "archiveLink": "/path/to/transcript.json",
  "messages": [ ... ]
}
```

Transcripts are also indexed into the Chorus index as `source='clearing'` with `channel='clearing:session'`.

## What Capturing Needs to Extract

### 1. Decisions → Cards or Decision Records

**Source**: `decisions[]` array in return object (DECISION markers)

**Contract**:
```json
{
  "type": "decision",
  "text": "string — the decision text",
  "author": "string — who declared it",
  "session_id": "string — clearing session timestamp",
  "timestamp": "ISO 8601",
  "action_required": "boolean — does this need a card?",
  "suggested_owner": "string — inferred from context",
  "suggested_card_title": "string — if action_required"
}
```

**Routing rules**:
- If decision text contains role names → `suggested_owner` = that role
- If decision starts with "No actions" or "Defer" → `action_required = false`
- If decision implies work → `action_required = true`, generate a card title

### 2. Commitments → Briefs or Card Updates

**Source**: Message content matching commitment patterns ("I'll...", "I'm going to...", "I can own...")

**Contract**:
```json
{
  "type": "commitment",
  "text": "string — the commitment",
  "role": "string — who committed",
  "session_id": "string",
  "timestamp": "ISO 8601",
  "target_entity": "string — card #, WF-NNN, or null",
  "brief_needed": "boolean — should this generate a handoff brief?"
}
```

**Routing rules**:
- If commitment references a card/workflow → `target_entity` = that reference
- If commitment is standalone work → `brief_needed = true`

### 3. Action Items → Card Queue

**Source**: Message content with action verbs directed at specific roles

**Contract**:
```json
{
  "type": "action_item",
  "text": "string — what needs to happen",
  "assigned_to": "string — role name",
  "session_id": "string",
  "timestamp": "ISO 8601",
  "priority": "P1|P2|P3 — inferred from urgency language",
  "existing_card": "number or null — if this maps to an existing card"
}
```

## Intake Queue Design

Capturing doesn't create cards directly — it populates an **intake queue** that a role (Wren, as PM) reviews and approves.

```
Clearing session ends
  → Return JSON written to /tmp/clearing-last-return.json
  → Capturing parser runs (automatic or manual trigger)
  → Extracts decisions, commitments, action items
  → Writes to intake queue: ~/.chorus/intake/YYYY-MM-DDTHH-MM-SS.json
  → Wren's session start shows: "3 items in intake queue"
  → Wren reviews, approves → cards created, decisions logged
```

### Intake Queue Schema

```json
{
  "session_id": "2026-02-21T18-57-43",
  "session_summary": {
    "participants": ["Jeff", "Wren", "Silas", "Kade"],
    "message_count": 24,
    "cost": 0.037,
    "duration_minutes": 14
  },
  "items": [
    {
      "type": "decision|commitment|action_item",
      "text": "...",
      "role": "...",
      "routing": {
        "action_required": true,
        "suggested_owner": "kade",
        "suggested_card_title": "Build /werk watcher prototype",
        "target_entity": "WF-004",
        "brief_needed": false
      },
      "status": "pending",
      "reviewed_by": null,
      "card_created": null
    }
  ]
}
```

## What This Doesn't Cover

- **Clearing-to-workflow creation**: If a Clearing session produces a decision that needs sequenced execution, that's a `/werk create` call — handled by the role running the session, not by Capturing.
- **Real-time extraction**: This is post-session batch processing. Real-time DECISION detection already exists in the Clearing UI.
- **Non-Clearing sources**: SMS capture, verbal direction, Slack (if it persists) — those need separate intake parsers. Same queue, different sources.

## Kade's Build Scope (Step 2)

1. Parser script: reads `/tmp/clearing-last-return.json`, extracts items using the patterns above
2. Intake queue writer: writes to `~/.chorus/intake/` as JSON
3. Session-start hook: counts pending intake items, surfaces in status line
4. CLI command: `workflow.sh intake` — list/review/approve pending items

## Open Questions

1. Should intake queue items auto-expire after 7 days if not reviewed?
2. Should the parser run automatically when a Clearing session ends (post-session hook), or only when invoked manually?
3. Does Wren want approval authority over all intake items, or should some auto-create cards (e.g., Jeff's explicit DECISION markers)?
