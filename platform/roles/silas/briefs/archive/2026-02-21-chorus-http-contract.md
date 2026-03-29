# Spec: Chorus Context Index HTTP Contract

**From**: Wren (PM) → Silas (Architect), WF-006 Step 1
**Date**: 2026-02-21
**Card**: C#30

## Current Consumers

1. **chorus-query.sh** — CLI queries (reconcile, search, refs, stats)
2. **Clearing server** — writes transcripts to index at session end (indexToChorus)
3. **session-start.sh** — calls chorus-query.sh reconcile on startup
4. **/werk integration** (planned) — card number joins against refs table
5. **The Borg** (future) — unknown query patterns, needs stable contract

## Database Schema (from chorus-init-db.sh)

**Tables**: `messages`, `messages_fts` (FTS5), `watermarks`, `refs`

**messages columns**: source, source_id, channel, role, author, content, timestamp, session_id, is_bridge, metadata

**refs columns**: entity_type, entity_id, message_source_id, relationship_type, context (from chorus-backfill-refs.sh)

## Proposed HTTP Endpoints

### GET /api/chorus/search?q={term}&limit={n}&role={role}
FTS5 full-text search across all indexed messages.
- Response: `{ results: [{ source, channel, role, author, content, timestamp, snippet }], total: N }`
- Maps to: `chorus-query.sh search`

### GET /api/chorus/reconcile?role={role}
What happened since this role's last session. Slack messages, other role sessions, Jeff's direction.
- Response: `{ slack: [...], sessions: { role: count }, jeffDirection: [...], stats: { total, bySource } }`
- Maps to: `chorus-query.sh reconcile`

### GET /api/chorus/refs?card={N}&type={entity_type}
References table query — find messages that mention a card, workflow, decision, or ADR.
- Response: `{ refs: [{ entity_type, entity_id, relationship_type, context, message: { content, timestamp, role } }] }`
- Maps to: `chorus-query.sh refs`

### GET /api/chorus/stats
Index statistics.
- Response: `{ total: N, bySource: { slack: N, claude: N, clearing: N, artifact: N }, lastIndexed: timestamp }`
- Maps to: `chorus-query.sh stats`

### POST /api/chorus/index
Trigger incremental index update (all sources).
- Response: `{ indexed: { slack: N, sessions: N, artifacts: N, clearings: N } }`
- Replaces: manual `chorus-index-*.sh` calls

## Stability Contract

- **Query shape**: field names in responses are frozen once shipped. Adding fields is OK, removing/renaming is breaking.
- **Latency**: search < 200ms, reconcile < 500ms, stats < 50ms (all reads from local SQLite, no network)
- **Error cases**: DB not found → 503, query syntax error → 400, index stale (>1hr) → 200 with `stale: true` header
- **Concurrency**: WAL mode (already set). Reads are concurrent. Writes use lockfile (already in chorus-query.sh).

## What Clearing Does NOT Query Today (Borg Gap)

- No cross-session threading (message A in session 1 relates to message B in session 2)
- No semantic similarity (only FTS5 keyword match)
- No structured extraction (decisions/commitments are in capture queue, not indexed as first-class)
- No codebase artifact linking (files mentioned in messages → actual files)

These gaps are where The Borg will need extensions to the contract.

## Implementation Notes

- Wrapper lives in `chorus/` repo
- Reads `~/.chorus/index.db` directly (file-backed, no cache)
- Express or plain Node HTTP — match Clearing's stack
- No auth for localhost (same as all our services per ADR-012)
