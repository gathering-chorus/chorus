# Context Service — Product Brief

**From:** Wren (PM)
**Date:** 2026-02-20
**Board:** Chorus #17 (proposed)
**Priority:** P1

---

## The Problem (In Jeff's Words)

> "In the action plane — which is what I'm calling Claude — you guys are already writing structured logs for a bunch of repeatable activities. In the interaction plane, I think we need something similar."

> "Maybe there's a way to register a new Claude command that I could run, which would go back to Slack and index all conversations on an incremental basis. You're basically harvesting the conversations back into a service... which is wrapped by some new command... and all I would do is make sure the index is updated based on your last read. Then kind of a process to reconcile your own state back to the context from our interaction layer."

> "If it's possible to also index all my visible interactions with you inside Claude, that is even better — basically all of our conversations both 1x1 and in groups."

**Translation:** Three roles maintain three separate realities. Slack conversations happen that roles don't see. Claude sessions happen that other roles don't see. There's no shared memory across the interaction plane (Slack) and the action plane (Claude sessions). Jeff is the only one who can see both — and he shouldn't have to be.

---

## Jeff's Two-Plane Model

```
┌─────────────────────────────────────────────────────┐
│  INTERACTION PLANE (Slack)                          │
│  - @team conversations                              │
│  - Role-to-role messages                            │
│  - Jeff's direction, decisions, feedback             │
│  - Voice memos (transcribed)                        │
│  - Group demos and reviews                           │
│  ⚠ Ephemeral. Not indexed. Roles see fragments.    │
└─────────────────────────────────────────────────────┘
        ↕  Jeff is the only bridge  ↕
┌─────────────────────────────────────────────────────┐
│  ACTION PLANE (Claude Sessions)                     │
│  - 1x1 sessions (Jeff ↔ Wren/Silas/Kade)           │
│  - Code commits, briefs written, decisions made      │
│  - State files updated, builds deployed              │
│  - Full JSONL transcripts persisted locally          │
│  ⚠ Isolated. Each role sees only its own sessions.  │
└─────────────────────────────────────────────────────┘
```

**The context service bridges both planes into a single searchable index that any role can query.**

---

## What We Have Today (Data Sources)

### Interaction Plane — Slack
- **Channels:** #all-gathering, #wren, #silas, #kade, #decisions, #standup, #chorus
- **Access:** Slack Bot Token (already in messages/.env)
- **API:** `conversations.history` — paginated, supports `oldest` param for incremental fetch
- **Volume:** ~100-200 messages/day across all channels

### Action Plane — Claude Sessions
- **Location:** `~/.claude/projects/` — one directory per project/role
- **Format:** JSONL — structured records with `type` (user/assistant/tool_use/progress), content, timestamps, session IDs
- **Volume:** 98 sessions to date, 20-40MB each for full sessions
- **Role mapping:** Directory names map to roles (product-manager → Wren, architect → Silas, engineer → Kade)

### Artifact Plane — Filesystem (bonus)
- **Briefs:** `*/briefs/*.md` — structured handoffs between roles
- **Decisions:** `product-manager/decisions.md` — numbered decisions with rationale
- **Activity log:** `messages/activity.md` — shared audit trail
- **next-session.md:** Per-role context continuity files

---

## Architecture: My Position

**SQLite with FTS5. Not Elasticsearch.**

Reasoning:
1. **No new Docker container.** The Macs run 15+ containers. Adding Elasticsearch (2GB RAM baseline) is irresponsible. SQLite is a single file.
2. **Already on every Mac.** No install. No config. No service to monitor.
3. **FTS5 is production-grade full-text search.** Ranking, phrase search, prefix search, boolean queries. It's what Apple uses in Spotlight.
4. **Incremental indexing is trivial.** Last-seen watermarks per source. SQLite handles concurrent reads from multiple processes without coordination.
5. **Portable.** The index file can be backed up, copied, or rebuilt from sources.

If we outgrow SQLite (unlikely for this volume), we upgrade later. But we won't.

**Location:** `~/.chorus/index.db` (outside the repo — this is local state, not shared code)

---

## Schema

```sql
-- Messages from all sources
CREATE TABLE messages (
    id INTEGER PRIMARY KEY,
    source TEXT NOT NULL,        -- 'slack', 'claude', 'brief', 'decision'
    source_id TEXT UNIQUE,       -- slack ts, session UUID, file path
    channel TEXT,                -- slack channel or 'session:wren', 'brief:silas'
    role TEXT,                   -- wren, silas, kade, jeff, system
    author TEXT,                 -- slack user ID or 'user'/'assistant'
    content TEXT NOT NULL,
    timestamp TEXT NOT NULL,     -- ISO 8601
    session_id TEXT,             -- claude session UUID (null for slack)
    thread_id TEXT,              -- slack thread_ts or conversation_id
    is_bridge BOOLEAN DEFAULT 0,
    metadata TEXT                -- JSON blob for extras
);

-- Full-text search index
CREATE VIRTUAL TABLE messages_fts USING fts5(
    content,
    role,
    channel,
    content='messages',
    content_rowid='id'
);

-- Watermarks for incremental indexing
CREATE TABLE watermarks (
    source TEXT PRIMARY KEY,     -- 'slack:all-gathering', 'claude:product-manager'
    last_seen TEXT NOT NULL,     -- timestamp or cursor
    last_indexed TEXT NOT NULL   -- when we last ran
);

-- Triggers to keep FTS in sync
CREATE TRIGGER messages_ai AFTER INSERT ON messages BEGIN
    INSERT INTO messages_fts(rowid, content, role, channel)
    VALUES (new.id, new.content, new.role, new.channel);
END;
```

---

## Indexing Scripts

### 1. Slack Indexer (`chorus-index-slack.sh`)
- Reads SLACK_BOT_TOKEN from `messages/.env`
- For each channel: fetch messages since watermark using `conversations.history(oldest=watermark)`
- Insert into SQLite, update watermark
- Handles bridge messages (tagged with `is_bridge=1` and `role` extracted from `··bridge:rolename`)
- **Incremental:** Only fetches new messages. First run backfills last 7 days.

### 2. Claude Session Indexer (`chorus-index-sessions.sh`)
- Scans `~/.claude/projects/` for `.jsonl` files
- Maps directory to role (project path → role name)
- For each session: parse JSONL, extract `type=user` and `type=assistant` messages
- Skip tool_use, progress, and system messages (noise)
- Insert human-readable content with session_id, role, timestamp
- **Incremental:** Track file modification time + byte offset as watermark

### 3. Artifact Indexer (`chorus-index-artifacts.sh`)
- Scan `*/briefs/*.md` — index filename, first 5 lines (title + context), modification date
- Parse `decisions.md` — extract individual decisions as searchable records
- Parse `activity.md` — index each entry as a separate message
- **Incremental:** Track file modification timestamps

### 4. Master Indexer (`chorus-index.sh`)
- Runs all three indexers in sequence
- Reports: "Indexed N new Slack messages, M new session turns, K new artifacts"
- Can be run manually or via cron (`*/15 * * * *` — every 15 minutes)
- Idempotent. Safe to run repeatedly.

---

## The `/chorus` Skill (Claude Code Integration)

Claude Code supports custom skills via `.claude/skills/`. A skill named `chorus` becomes the `/chorus` command.

### File: `.claude/skills/chorus/SKILL.md`
```markdown
---
name: chorus
description: Query the context index — Slack conversations + Claude sessions + artifacts
---

# Chorus Context Service

Query the indexed history of all team interactions.

## Current context
!`~/.chorus/scripts/chorus-query.sh "$ARGUMENTS"`

## Usage
- `/chorus` — Show what changed since your last session (default: reconcile mode)
- `/chorus search <term>` — Full-text search across all sources
- `/chorus decisions` — Recent decisions and their context
- `/chorus role <name>` — What has this role been doing?
- `/chorus session <date>` — What happened on this date?
- `/chorus brief <keyword>` — Find briefs related to a topic

Analyze the results and tell me what's relevant to my current work.
```

### Query Script: `~/.chorus/scripts/chorus-query.sh`

The script interprets the argument and runs the appropriate SQLite query:

- **No argument (reconcile mode):** Find everything since this role's last session end. Group by source. Highlight decisions, commitments, blocked items.
- **`search <term>`:** FTS5 query across all content. Rank by relevance + recency.
- **`role <name>`:** Filter by role. Show recent activity chronologically.
- **`decisions`:** Filter source='decision'. Show status and related discussion.

Output is formatted as markdown that Claude can analyze in context.

---

## Reconciliation: The Killer Feature

When a role starts a session, the **reconcile** operation answers: "What happened while I was offline?"

```
┌─ Since your last session (2026-02-20 09:44 UTC) ──────────┐
│                                                              │
│  SLACK (12 new messages)                                    │
│  • #all-gathering: Jeff posted @team about cockpit scope    │
│  • #all-gathering: 3-role response re: DEC-029 enforcement  │
│  • #kade: Brief notification for SMS capture                │
│  • #wren: 4 commitment brief notifications                  │
│                                                              │
│  CLAUDE SESSIONS (2 new sessions)                           │
│  • Kade session (1h 23m): DEC-028 shipped, both paths built │
│  • Silas session (45m): communication-flows.md, PNG diagrams│
│                                                              │
│  ARTIFACTS (3 changes)                                       │
│  • New brief: engineer/briefs/session-holding-fix.md         │
│  • Decision updated: DEC-028 → Shipped                      │
│  • Activity: 5 new entries you haven't seen                  │
│                                                              │
│  ⚠ STALE STATE WARNING                                      │
│  • Your decisions.md still shows DEC-028 as "Approved"      │
│    but Kade shipped it (commit 827ef00)                      │
│                                                              │
└──────────────────────────────────────────────────────────────┘
```

This is the exact problem that caused the "three realities" failure today. If reconcile had existed, Wren would have known DEC-028 was shipped before the bridge incorrectly told Jeff otherwise.

---

## Integration Points

### 1. SessionStart Hook (Automatic)
Add to each role's `settings.local.json`:
```json
{
  "hooks": {
    "SessionStart": [
      {
        "type": "command",
        "command": "~/.chorus/scripts/chorus-query.sh reconcile wren",
        "timeout": 15
      }
    ]
  }
}
```
Every session starts with automatic context reconciliation. No manual step.

### 2. Manual `/chorus` Command
Jeff types `/chorus search cockpit` in any session → gets cross-plane search results.

### 3. Bridge Enhancement (Future)
The bridge's `context-assembler.ts` could query the index instead of reading raw Slack messages. This replaces the stale 10-message window with relevance-ranked context.

### 4. Cockpit Page (Future)
The cockpit we designed today could query the same SQLite index instead of hitting multiple APIs. Single data source for the single pane of glass.

---

## Build Sequence

### Phase 1: Index + Query (Wren specs, Kade builds)
1. Create `~/.chorus/` directory structure
2. Build `chorus-index-slack.sh` — Slack message indexer
3. Build `chorus-index-sessions.sh` — Claude transcript indexer
4. Build `chorus-query.sh` — query interface with reconcile + search modes
5. Create `.claude/skills/chorus/SKILL.md` — register the `/chorus` command
6. Wire into SessionStart hook for auto-reconcile
7. Manual test: Jeff opens a session, sees reconciled context

### Phase 2: Artifact Indexing
8. Build `chorus-index-artifacts.sh` — briefs, decisions, activity
9. Add stale state detection to reconcile mode
10. Add cron job for background indexing (every 15 min)

### Phase 3: Bridge Integration
11. Replace bridge's raw Slack reads with index queries
12. Add conversation threading to index (link replies to threads)
13. Cockpit page reads from index instead of multiple APIs

---

## What's NOT In Scope

- **Vector embeddings / semantic search.** FTS5 keyword search is sufficient for this volume. Semantic search adds a model dependency and inference cost. If keyword search fails, we add it later.
- **Real-time streaming.** This is a pull model (Jeff's principle #4 from the cockpit brief). Index refreshes every 15 minutes or on-demand.
- **Cross-machine sync.** Index lives on the primary Mac only. The secondary Mac doesn't run Claude sessions.
- **Message editing/deletion sync.** We index what was said, not what was later changed. This is an audit trail.

---

## Infrastructure Cost

- **Disk:** ~50-100MB for the SQLite database (all messages + FTS index). Negligible.
- **CPU:** Indexing runs in seconds (incremental). No persistent process.
- **RAM:** Zero ongoing. SQLite opens on query, closes after.
- **New containers:** Zero. Shell scripts + SQLite.
- **Dependencies:** `sqlite3` (already on macOS), `python3` (already installed), `curl` (already used by slack-read.sh).

This is the lightest infrastructure decision we can make. It costs the Macs nothing.

---

## Open Questions for Jeff

1. **How far back should we index?** Suggest: Slack = 7 days on first run (then incremental forever). Claude sessions = all 98 existing sessions (one-time backfill, ~15 min).
2. **Should reconcile run automatically on SessionStart, or should you invoke `/chorus` manually?** I recommend automatic — the whole point is you shouldn't have to ask.
3. **Privacy:** Claude session transcripts contain everything — including your personal messages, stories, and private thinking. The index is local-only and never leaves the Mac, but should we exclude any content categories from indexing?

---

— Wren
