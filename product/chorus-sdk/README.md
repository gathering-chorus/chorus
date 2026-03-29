# Chorus SDK

TypeScript SDK for the Chorus spine — the shared memory and event layer for the Gathering team.

Three functions. One import. Full access to team memory.

```typescript
import { emit, search, subscribe } from 'chorus-sdk';
```

## Why

Before this SDK, interacting with Chorus required shelling out to `chorus-log.sh` or hand-rolling HTTP calls. Every tool reinvented the same patterns. The SDK provides a typed, testable, consistent interface that any TypeScript process can use — from board-ts to future services.

## Functions

### emit(event, role, extra?, options?)

Write a structured spine event to `chorus.log`.

```typescript
import { emit } from 'chorus-sdk';

emit('card.accepted', 'silas', { card: '1148' });
// → appends JSON line to chorus.log
// → indexed by Chorus within seconds
```

**Parameters:**
- `event` — dot-namespaced event name (e.g. `card.accepted`, `role.nudge.sent`)
- `role` — who emitted it (`silas`, `wren`, `kade`, `system`)
- `extra` — arbitrary key-value metadata
- `options.appName` — override app name (default: `chorus-sdk`)
- `options.component` — override component (default: `sdk`)
- `options.logFile` — write to a different file

**Returns:** the `SpineEvent` object written.

### search(term, limit?)

Query the Chorus index — 67K+ messages across Slack, Claude sessions, briefs, decisions, spine events, and stories.

```typescript
import { search } from 'chorus-sdk';

const results = await search('nudge', 5);
console.log(`Found ${results.total} results`);
for (const r of results.results) {
  console.log(`[${r.source}/${r.role}] ${r.content}`);
}
```

**Parameters:**
- `term` — search query (full-text search)
- `limit` — max results (default: 20)

**Returns:** `Promise<SearchResponse>` with `results[]`, `total`, and `query`.

Hits the Chorus API at `http://localhost:3340` (configurable via `CHORUS_API_URL` env var).

### subscribe(filter, callback, options?)

Watch the spine for live events. Tails `chorus.log` and fires the callback when matching events appear.

```typescript
import { subscribe } from 'chorus-sdk';

const unsub = subscribe('card.accepted', (event) => {
  console.log(`${event.role} accepted card ${event.card}`);
});

// Later: stop watching
unsub();
```

**Parameters:**
- `filter` — string (exact or substring match), RegExp, or predicate function
- `callback` — called with parsed event object
- `options.pollInterval` — check frequency in ms (default: 1000)
- `options.logFile` — watch a different file

**Returns:** unsubscribe function.

## Consumers

- **board-ts** — the Kanban board CLI uses the SDK for spine event emission on every card mutation
- **nudge.sh / clearing-reply.sh** — shell scripts emit via `chorus-log.sh`; the SDK is the TypeScript equivalent

## Architecture

```
┌─────────────┐     emit()      ┌──────────────┐
│  Any TS     │ ──────────────→ │ chorus.log   │
│  process    │                 │ (spine)      │
│             │ ←────────────── │              │
│             │   subscribe()   └──────────────┘
│             │                        │
│             │     search()    ┌──────────────┐
│             │ ──────────────→ │ Chorus API   │
│             │ ←────────────── │ :3340        │
└─────────────┘                 │ (67K+ msgs)  │
                                └──────────────┘
```

The spine (`chorus.log`) is the write path — append-only JSON lines. The Chorus API is the read path — full-text search over the indexed corpus. Subscribe bridges both by tailing the log file directly.

## Development

```bash
npm run build    # compile TypeScript
npm test         # 6 tests, ~1.5s
node demo.js     # live walkthrough (emit + search + subscribe)
```

## Card History

- **#972** — SDK built: emit/search/subscribe, board-ts wired, 57 tests → 6 focused tests
- **#973** — This README + demo script + linked from System/About
- **#1148** — Nudge spine events wired using the patterns this SDK established
