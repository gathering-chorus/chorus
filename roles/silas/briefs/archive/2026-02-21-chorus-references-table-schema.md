# Chorus References Table Schema

**From**: Silas (Architect) → Kade (Engineer)
**Date**: 2026-02-21
**Card**: #123 — Slack bridge deprecation
**Workflow**: WF-004, Step 1

## Context

The Chorus index needs a `references` table so agents can query relationships between messages and entities (cards, workflows, decisions, other messages) without fragile text matching. This is a hard blocker before bridge deprecation — the /werk watcher needs to link confirmation messages back to pending actions reliably.

## Current Index Schema (for reference)

Database: `~/.chorus/index.db`

```sql
-- Existing messages table (11 columns)
CREATE TABLE messages (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    source TEXT NOT NULL,
    source_id TEXT UNIQUE,
    channel TEXT,
    role TEXT,
    author TEXT,
    content TEXT NOT NULL,
    timestamp TEXT NOT NULL,
    session_id TEXT,
    thread_id TEXT,
    is_bridge INTEGER DEFAULT 0,
    metadata TEXT
);
```

**Message ID structure** (what you need to join on):
- `id` is the autoincrement INTEGER primary key
- `source_id` is the unique source-specific identifier:
  - Slack: `slack:{channel}:{ts}` (e.g., `slack:silas:1771699263.123456`)
  - Claude sessions: `claude:{session_uuid}:line_{N}` (e.g., `claude:62af6a9b-814f-4d36-8b3b-dce80fe328d5:line_245`)
  - Clearing: `clearing:{session_timestamp}:{msg_id}` (e.g., `clearing:2026-02-21T10-30-00:7`)
  - Artifacts: `artifact:{type}:{filename}` (e.g., `artifact:brief:2026-02-21-sensory-bridge.md`)
  - Decisions: `decision:{id}` (e.g., `decision:DEC-035`)

## New Schema: `references` table

```sql
CREATE TABLE IF NOT EXISTS references (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    message_id INTEGER NOT NULL,        -- FK to messages.id
    entity_type TEXT NOT NULL,          -- 'card', 'workflow', 'decision', 'adr', 'brief', 'message'
    entity_id TEXT NOT NULL,            -- '#115', 'WF-003', 'DEC-035', 'ADR-012', source_id of another message
    relationship TEXT NOT NULL DEFAULT 'mentions',  -- 'mentions', 'responds_to', 'confirms', 'blocks', 'resolves'
    extracted_at TEXT NOT NULL,         -- ISO 8601 — when the reference was extracted
    FOREIGN KEY (message_id) REFERENCES messages(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_refs_message ON references(message_id);
CREATE INDEX IF NOT EXISTS idx_refs_entity ON references(entity_type, entity_id);
CREATE INDEX IF NOT EXISTS idx_refs_relationship ON references(relationship);
```

## Entity Types

| entity_type | entity_id format | Example |
|---|---|---|
| `card` | `#N` | `#115`, `#123` |
| `workflow` | `WF-NNN` | `WF-003`, `WF-004` |
| `decision` | `DEC-NNN` | `DEC-035` |
| `adr` | `ADR-NNN` | `ADR-012` |
| `brief` | filename | `2026-02-21-sensory-bridge.md` |
| `message` | source_id of target | `slack:silas:1771699263.123456` |

## Relationship Types

| relationship | Meaning | /werk watcher use |
|---|---|---|
| `mentions` | Message references this entity | Default — extracted from text patterns |
| `responds_to` | Message is a reply to another message | Thread linking |
| `confirms` | Message confirms/acknowledges a pending action | **Critical for watcher** — confirmation logic |
| `blocks` | Message reports a blocker on this entity | Status tracking |
| `resolves` | Message resolves/closes this entity | Completion tracking |

## Extraction Patterns (for backfill + ongoing indexing)

```python
# Card references: #N where N is 1-4 digits
r'#(\d{1,4})\b'  →  entity_type='card', entity_id='#N'

# Workflow references: WF-NNN
r'WF-(\d{3})\b'  →  entity_type='workflow', entity_id='WF-NNN'

# Decision references: DEC-NNN
r'DEC-(\d{3})\b'  →  entity_type='decision', entity_id='DEC-NNN'

# ADR references: ADR-NNN
r'ADR-(\d{3})\b'  →  entity_type='adr', entity_id='ADR-NNN'

# Confirmation patterns (for watcher):
r'\b(confirmed?|acknowledged?|done|shipped|completed|approved)\b'
→  relationship='confirms' (requires context to determine what's being confirmed)
```

## Query Patterns the Watcher Will Need

```sql
-- Find all messages that reference WF-004
SELECT m.* FROM messages m
JOIN references r ON r.message_id = m.id
WHERE r.entity_type = 'workflow' AND r.entity_id = 'WF-004'
ORDER BY m.timestamp DESC;

-- Find confirmation messages for a specific card
SELECT m.* FROM messages m
JOIN references r ON r.message_id = m.id
WHERE r.entity_type = 'card' AND r.entity_id = '#123'
  AND r.relationship = 'confirms'
ORDER BY m.timestamp DESC;

-- Find all entities referenced in a message
SELECT r.* FROM references r WHERE r.message_id = ?;

-- Cross-reference: what cards does this workflow touch?
SELECT DISTINCT r2.entity_id FROM references r1
JOIN references r2 ON r1.message_id = r2.message_id
WHERE r1.entity_type = 'workflow' AND r1.entity_id = 'WF-004'
  AND r2.entity_type = 'card';
```

## Backfill Plan

1. Add `references` table to `chorus-init-db.sh`
2. One-time backfill script: scan all 17,325 messages, extract references using regex patterns above
3. Hook extraction into each indexer (`chorus-index-slack.sh`, `chorus-index-sessions.sh`, `chorus-index-artifacts.sh`) so new messages get references extracted at index time
4. Estimated backfill time: <5 seconds (regex over 17k rows in SQLite)

## What Kade Needs to Start

You can start building the watcher with text-pattern matching against `messages.content` today. When the references table lands, you switch from:
```sql
WHERE content LIKE '%WF-004%'
```
to:
```sql
JOIN references r ON r.message_id = m.id WHERE r.entity_id = 'WF-004'
```

Same logic, cleaner queries, no false positives on substring matches.
