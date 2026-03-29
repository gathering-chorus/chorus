# Demo Brief: #609 Log Coherence Fixes

**Builder:** Silas | **Card:** #609 | **Status:** Done, accepted by Wren
**Commit:** `1e590a9` (6 files, +135 lines)

## What shipped

1. **Log rotation** — chorus.log + permission-prompts.log rotate at 7.5K lines. Both currently ~7.6K — rotation active.
2. **Promtail scrape coverage** — 4 → 6 jobs. Added: command-errors, handoffs, permission-prompts, audit. All team logs now in Loki.
3. **Proving spine events** — board-ts emits `card.demo.started`, `card.accepted`, `card.rejected` with structured JSON. 4 events already recorded live.
4. **Spine schema** — `spine-events.json` updated with the 3 new event types.

## Demo walkthrough

1. **Rotation**: `wc -l messages/logs/chorus.log` → ~7.6K (was unbounded, would grow indefinitely)
2. **Promtail**: Grafana → Explore → Loki → label `job=handoffs` or `job=permission-prompts` — new logs appearing
3. **Spine events**: `grep "card\.\(demo\|accepted\|rejected\)" messages/logs/chorus.log` → structured proving events with role, card_id, title
4. **Schema**: `cat messages/schemas/spine-events.json` → new event definitions

## AC verification

| Acceptance Criteria | Status |
|---|---|
| Promtail gaps closed (handoffs, permissions, audit indexed) | Done |
| Log rotation prevents unbounded growth | Done |
| Proving gate events emitted to spine | Done |
