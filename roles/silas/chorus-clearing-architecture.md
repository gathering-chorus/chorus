# Chorus + Clearing Architecture

Last updated: 2026-02-25 | Werk v1.3.20

## Overview

**Chorus** is the shared memory index — SQLite FTS5 database indexing all team coordination artifacts. The team's searchable memory.

**The Clearing** is the real-time interaction layer — browser-based multi-party chat where Jeff converses with Wren, Silas, and Kade simultaneously.

Together they form the awareness and interaction layers of the Chorus coordination product.

## Part 1: Chorus — Shared Memory Index

### Storage

- **Database:** `~/.chorus/index.db` (SQLite FTS5, WAL mode)
- **Records:** 31,523 messages indexed (as of 2026-02-25)
- **Sources:** Claude sessions (28,472), Slack historical (1,845), Clearing (696), briefs (423), decisions (54), ADRs (16), activity (11)
- **Date range:** Feb 2024 – Feb 2026

### Schema (4 core tables)

**`messages`** — Primary content store
```
id, source, source_id (UNIQUE), channel, role, author, content (max 5000 chars),
timestamp (ISO 8601 UTC), session_id, thread_id, is_bridge, metadata (JSON)
```

**`messages_fts`** — FTS5 virtual table over content + role + channel. Auto-synced via triggers.

**`watermarks`** — Incremental indexing cursors
```
source (PK), last_seen (offset/timestamp/mtime), last_indexed (ISO 8601)
```

**`refs`** — Cross-reference entity graph
```
id, message_id (FK), entity_type (card|workflow|decision|adr),
entity_id (#47, WF-015, DEC-022, ADR-011), relationship (mentions), extracted_at
```

### Ingestion Pipelines (5 independent)

#### 1. Session Transcripts (`chorus-index-sessions.sh`)
- **Input:** `~/.claude/projects/**/*.jsonl`
- **Trigger:** Ambient fswatch daemon (within 3s of write)
- **Process:** Parse JSONL → extract user/assistant messages → truncate >5000 chars → insert with `source='claude'`, `channel=session:{role}` → extract entity refs
- **Watermark:** File size (skip unchanged files)
- **Scale:** 20,857 messages, majority of index

#### 2. Slack Messages (`chorus-index-slack.sh`) — DEPRECATED 2026-02-22
- **Status:** Retained for historical access only. 1,845 messages remain queryable.

#### 3. Clearing Transcripts (in `clearing/src/server.ts`)
- **Trigger:** Session end (SIGTERM/SIGINT/SIGHUP/disconnect)
- **Process:** Read transcript → insert all messages with `source='clearing'`, `channel=clearing:session` → mark DECISION lines in metadata
- **Scale:** 567 messages from all Clearing sessions

#### 4. Artifacts (`chorus-index-artifacts.sh`)
- **Sources:** Briefs (all 3 role directories), ADRs, decisions.md (parsed per DEC-NNN header), activity.md (parsed per date header), state files
- **Watermark:** Per-file mtime
- **Role detection:** Path-based (`product-manager/` → wren, `architect/` → silas, `engineer/` → kade)

#### 5. Entity Reference Extraction (`chorus-extract-refs.sh`)
- **Trigger:** After each indexer batch completes
- **Patterns:** `#\d{1,4}` → card, `WF-\d{3}` → workflow, `DEC-\d{3}` → decision, `ADR-\d{3}` → adr
- **Dedup:** `(msg_id, etype, eid)` as key

### Ambient Indexing Daemon

- **Daemon:** `com.chorus.session-watcher` (launchd, survives reboot)
- **Watcher:** `fswatch` on `~/.claude/projects/` for JSONL writes
- **Debounce:** 3s window
- **On write:** Runs `chorus-index-sessions.sh`
- **PID:** `~/.chorus/watcher.pid`
- **Log:** `~/.chorus/watcher.log`
- **Effect:** Near-real-time indexing — "cubicle glance" ambient awareness

### Query Interfaces

#### Shell (`chorus-query.sh`)

| Command | Function | Latency |
|---------|----------|---------|
| `reconcile [role]` | What happened since my last session? Shows other roles' sessions, Jeff's direction, index stats | ~200ms |
| `search "<term>"` | FTS5 full-text search, 20 results with snippets. LIKE fallback on FTS5 syntax errors | ~50ms |
| `role <name>` | Recent activity by role (20 messages) | ~50ms |
| `stats` | Index health — total count, source/role/channel breakdown, watermarks | ~50ms |

#### HTTP API (`chorus-api`, port 3340)

| Endpoint | Function |
|----------|----------|
| `GET /api/chorus/search?q=&limit=&role=` | FTS5 search with snippets. `X-Chorus-Stale: true` header if index >1hr old |
| `GET /api/chorus/reconcile?role=` | Structured reconcile (Slack, sessions, Jeff direction, stats) |
| `GET /api/chorus/refs?card=&wf=&type=&id=` | Entity reference queries ("where is #47 mentioned?") |
| `GET /api/chorus/stats` | Comprehensive index stats |
| `POST /api/chorus/index` | Manual index trigger (all pipelines, 30s timeout) |
| `GET /health` | DB readable check (<1ms) |

**Implementation:** Express + better-sqlite3, no connection pooling (new connection per request, acceptable at <1 req/sec).

#### Skill (`/chorus`)

Registered at `~/.claude/skills/chorus/skill.json`. Wraps `chorus-query.sh`.

### Concurrency Control

- **Lock file:** `~/.chorus/index.lock` prevents concurrent writers
- **Stale detection:** `chorus-query.sh` checks if lock holder PID is alive
- **Recovery:** Manual `rm ~/.chorus/index.lock` if watcher crashes mid-index
- **Gap:** Watcher itself doesn't check for stale locks (see Concerns)

### Architectural Decisions

1. **File-backed index (no cache):** SQLite is source of truth. Every query hits DB directly. Tradeoff: guaranteed consistency over speed. Can inspect with `sqlite3` CLI.
2. **Watermark incremental indexing:** Tracks offset/mtime per source. Re-runnable — can fail mid-stream, restart, pick up where it left off. `source_id` UNIQUE prevents duplicates.
3. **Role-based channels:** Messages grouped by `session:{role}`, `brief:{role}`, `clearing:session`, etc. Enables role-filtered reconciliation.
4. **FTS5 with LIKE fallback:** Handles edge cases where query contains hyphens or regex metacharacters.

---

## Part 2: The Clearing — Multi-Party Chat

### Architecture

| Layer | Implementation |
|-------|---------------|
| **Server** | Express + Socket.IO, TypeScript |
| **Model** | Claude Haiku 4.5 (`claude-haiku-4-5-20251001`), configurable via `CLEARING_MODEL` |
| **Token limit** | 300 per response (configurable via `CLEARING_MAX_TOKENS`) |
| **Port** | Random available (configurable via `CLEARING_PORT`) |
| **UI** | Single HTML file (610 lines), GitHub Dark theme (#0d1117) |

### Participant System (`participants.ts`)

Three hardcoded roles with system prompts:

| Role | Color | Perspective | Tone Instruction |
|------|-------|-------------|------------------|
| **Wren** | #4ade80 (green) | Product thinking, priorities, UX, coordination | "Be opinionated — Jeff wants a PM voice, not an accommodating one" |
| **Silas** | #60a5fa (blue) | Structural integrity, cross-project coherence, tech debt | "If something has structural problems, say so plainly" |
| **Kade** | #fb923c (orange) | Implementation feasibility, code quality, velocity | "If you can build something faster than discussing it, say so" |

**Context injection:** Optional `--context` flag appends to all role system prompts under `## Session Context`. Use to orient Clearing around a specific card or topic.

**Response generation:** Full transcript formatted as `[Sender]: content` → sent to Claude with system prompt + "Respond as {RoleName}. Stay concise." Streaming via callback.

### Socket.IO Protocol

**Inbound (client → server):**
- `message` — Jeff sent a message. Parses `@role` mentions: `@wren` → only Wren responds; no mention → all 3 respond in sequence
- `end-session` — User clicked End Session
- `disconnect` — Browser closed

**Outbound (server → client):**
- `init` — Participant list, model, Werk version
- `stream:start` → `stream:token` → `stream:end` — Per-role streaming response
- `decision` — DECISION marker detected
- `cost` — Running token/cost total
- `session-ended` — Session closed

### Transcript Management (`transcript.ts`)

- **In-memory:** `messages[]` array with auto-incrementing IDs
- **Auto-save:** Every 30s while session active
- **Signal-safe:** Saved on SIGTERM/SIGINT/SIGHUP
- **Storage:** `chorus/clearing/transcripts/{timestamp}.json`
- **Reuses filename** within a session (updates, not duplicates)

**Decision extraction:** Regex `DECISION[\s:–—-]+(.+)` (case-insensitive). Returns structured array with marker, speaker, timestamp, messageId.

**Cost estimation:** Hardcoded `MODEL_COSTS` dictionary:
- Haiku: $0.80/M input, $4.00/M output
- Sonnet: $3.00/M input, $15.00/M output
- Opus: $15.00/M input, $75.00/M output

### Session Lifecycle

```
1. Launch: bin/clearing [--context "..."] [--model ...] [--port ...]
   → Compile TS if needed → start Express + Socket.IO → open browser

2. Active session:
   → Jeff types → server broadcasts to roles → stream responses
   → DECISION: markers captured → cost tracked → transcript auto-saved every 30s

3. End session (button click / browser close / signal):
   → Save final transcript
   → Extract decisions
   → Build return object (SessionReturn)
   → Write /tmp/clearing-last-return.json
   → Write /tmp/clearing-last-transcript.txt (path only)
   → Index to Chorus (insert all messages, source='clearing')
   → Auto-capture + route (#275, 2026-02-25):
     → Run chorus-capture.sh → extract decisions/commitments/actions to ~/.chorus/intake/
     → Auto-route items with clear role assignments:
       - Decisions → create workflow via workflow.sh + write handoff brief
       - Commitments/actions → write handoff brief to owner's briefs/ directory
     → Mark routed items as status='routed' in intake file
     → Ambiguous items stay 'pending' for Wren to triage
   → Close Chrome tab via AppleScript
   → Exit (3s grace period)
```

**Return object structure:**
```json
{
  "session": {
    "started": "ISO 8601",
    "ended": "ISO 8601",
    "participants": ["Jeff", "Wren", "Silas", "Kade"],
    "model": "claude-haiku-4-5-20251001",
    "totalTokens": { "input": N, "output": N },
    "estimatedCost": 0.10,
    "messageCount": N,
    "decisionCount": N
  },
  "decisions": [{ "marker": "...", "speaker": "...", "timestamp": "..." }],
  "archiveLink": "/path/to/transcript.json",
  "messages": [...]
}
```

### Client UI

- Full-screen, 3 sections: header (participants + cost meter), messages (scrollable, color-coded), input (text + End Session button)
- `@wren`, `@silas`, `@kade` highlights as-you-type
- Streaming text rendered in real-time
- Decision markers highlighted
- Markdown rendering + syntax highlighting (highlight.js)

---

## Integration Points

### Chorus → Spine

| Integration | How |
|-------------|-----|
| Session start | `chorus-query.sh reconcile` builds context for `/tmp/session-start-<role>.md` |
| `/chorus` skill | Any role can search team memory mid-session |
| Entity refs | "Where is card #47 mentioned?" queries across all sources |
| Grafana | chorus.log → Promtail → Loki → Chorus Activity dashboard |

### Clearing → Chorus

| Integration | How |
|-------------|-----|
| Post-session indexing | All Clearing messages inserted into `messages` table |
| Decision capture | DECISION markers extracted → return JSON → intake queue |
| Transcript archive | JSON saved to `chorus/clearing/transcripts/` |

### Clearing → Spine

| Integration | How |
|-------------|-----|
| `/clearing` skill | Invoked from any Claude Code session |
| Return JSON | `/tmp/clearing-last-return.json` consumed by invoking session |
| Auto-capture + route | `captureAndRoute()` in endSession — decisions/commitments/actions extracted, auto-routed to roles via workflows + briefs (#275) |
| Intake queue | `~/.chorus/intake/{session-id}.json` — pending items for Wren triage, routed items already dispatched |

---

## Performance

| Operation | Latency |
|-----------|---------|
| FTS5 search (20 results) | ~50ms |
| Reconcile query | ~200ms |
| Ref extraction (per message) | ~1ms |
| HTTP API search | ~100ms |
| Index run (per 100 messages) | ~500ms |
| Ambient index (write → searchable) | <3s |
| Clearing token stream | <10ms per token |
| Clearing session save | ~50ms |

---

## Concerns

### Chorus

#### 1. Refs table incomplete for pre-Feb-21 messages
Backfill script exists (`chorus-backfill-refs.sh`) but hasn't been run. Entity queries ("where is #47 mentioned?") miss older context. **Fix:** Run backfill once. Low effort, high value.

#### 2. No HTTP API rate limiting
No guard rail if client code loops. Not observed yet, but no protection. **Risk:** Low today. **Fix:** Add basic rate limiting middleware when API usage grows.

#### 3. Metadata not queryable
JSON blobs in `metadata` column only searchable via content match. "All decisions from Clearing session X" requires full scan. **Fix:** Extract key fields to indexed columns if query patterns demand it.

#### 4. FTS5 tokenization fragile
Hyphens split tokens, special chars fail FTS5 syntax. LIKE fallback works but loses ranking/snippets. **Fix:** Pre-normalize queries or add custom tokenizer. Low priority — fallback is functional.

#### 5. Watermark table unbounded
23 entries now, one per session file. Will grow to hundreds over months as sessions accumulate. **Fix:** Periodic archival of completed session watermarks. Very low priority.

#### 6. Index lock not self-healing in watcher
If ambient watcher crashes mid-index, lock file stays. `chorus-query.sh` has stale-PID check but the watcher daemon itself doesn't. **Fix:** Add PID validity check to watcher before acquiring lock.

### The Clearing

#### 7. Voice quality — the biggest gap (C#37)
Haiku roles sound generic. Root causes:
- System prompts are thin (~5 sentences per role)
- No `/chorus` context injection — roles have zero memory of prior work
- No temperature control (defaults to ~1.0 = high variance)
- No stop detection — roles don't know when to stay quiet
- Token counts escalate (full transcript sent each turn)

**Impact:** The Clearing is the highest-bandwidth alignment tool and it underperforms. Jeff noted roles use "sprints", "deploy windows" — corporate language that doesn't match the team's vocabulary.

**Fix (C#37 proposals):**
1. Richer system prompts with team vocabulary, recent decisions, active cards
2. `/chorus reconcile` injection at session start for each role
3. Temperature 0.7 for consistency
4. Stop detection: if role has nothing to add, emit "[pass]" instead of filler
5. Sliding window: summarize older messages, send recent N turns in full

#### 8. No conversation windowing
Full transcript sent to Claude on every turn. A 30-minute session with 3 active roles will hit context limits. **Fix:** Sliding window with summary of older messages. Medium priority — longer sessions will trigger this.

#### 9. Cost model hardcoded
`MODEL_COSTS` in `transcript.ts` needs manual update when pricing changes. **Fix:** Pull from API or config file. Low priority.

#### 10. No graceful API degradation
If Anthropic API is down or rate-limited, Clearing crashes instead of showing error state in UI. **Fix:** Catch API errors, emit error event to client, show "Role unavailable" in chat.

#### 11. @mention routing all-or-nothing
Either one role responds or all three. No "Wren and Silas but not Kade" syntax. **Fix:** Parse multiple @mentions. Low priority — workaround is sequential @mentions.

#### 12. No authentication
Anyone on localhost can connect to the Clearing. Fine for single-user Mac, but blocks C#36 (mobile LAN access) without auth. **Fix:** Token-based auth required before C#36 ships.

---

## File Structure

```
chorus/
├── clearing/                    # The Clearing (multi-party chat)
│   ├── bin/clearing             # Launcher script
│   ├── src/
│   │   ├── server.ts            # Express + Socket.IO
│   │   ├── participants.ts      # Role definitions + Claude API
│   │   └── transcript.ts        # Message management + return object
│   ├── public/index.html        # Browser UI (610 lines)
│   ├── transcripts/             # Saved transcripts (JSON)
│   └── dist/                    # Compiled JS
├── api/                         # HTTP wrapper
│   ├── src/server.ts            # Express endpoints (port 3340)
│   └── dist/
├── scripts/                     # Indexing & query (bash)
│   ├── chorus-init-db.sh        # Schema creation
│   ├── chorus-query.sh          # Shell query interface
│   ├── chorus-index-sessions.sh # Claude transcript indexer
│   ├── chorus-index-slack.sh    # [DEPRECATED] Slack indexer
│   ├── chorus-index-artifacts.sh # Brief/ADR/decision indexer
│   ├── chorus-extract-refs.sh   # Entity reference extraction
│   ├── chorus-capture.sh        # Clearing return → intake queue
│   ├── chorus-log.sh            # Structured JSON event emitter
│   └── chorus-audit.sh          # Gate compliance checks
├── config/profiles/             # Permission profiles (base + per-role)
├── docs/                        # Architecture docs + diagrams
└── skill/skill.json             # /chorus skill registration

~/.chorus/
├── index.db                     # SQLite FTS5 (27MB)
├── scripts/                     # Symlinked from chorus/scripts/
├── intake/                      # Clearing capture queue
├── watcher.pid                  # fswatch daemon PID
├── watcher.log                  # Daemon log
└── watcher.lock                 # Index lock file
```

## Related Documents

- `spine-architecture.md` — Full spine overview (all 12 components)
- `system-architecture.md` — System-wide architecture
- `chorus/docs/chorus-overview.md` — Product vision
- `chorus/docs/communication-flows.md` — Sequence diagrams (7 patterns)
- `../product-manager/chorus-overview.md` — Wren's product perspective
