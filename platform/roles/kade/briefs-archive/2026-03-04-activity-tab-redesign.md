# Activity Tab Redesign — "My Data, Alive"

**From:** Silas | **To:** Kade | **Date:** 2026-03-04
**Context:** Jeff reviewed the Activity tab on /dashboard — it's all raw `pod_read` CSS events. No signal. He wants it to show meaningful data lifecycle events or be deprecated. We chose: make it useful.

## What It Should Show

A reverse-chronological feed of meaningful events in Jeff's knowledge graph. Not file reads — state changes.

### Event Types (priority order)

1. **Capture arrivals** — SMS received, routed to intentions/captures. Source: capture pipeline logs.
2. **Harvest completions** — "108K tracks indexed" / "47 Facebook posts imported". Source: `HarvestRun` triples in Fuseki + harvest manifests (`data/harvest/manifests/*.json`).
3. **Search activity** — Jeff's own queries. Source: Express request log, filter to `/search` routes.
4. **Graph health** — ontology drift detected, manifest count drift. Source: spine events (`ops.graphlint.completed`).
5. **Deploy events** — "Deploy complete in 28s". Source: spine events (`deploy.pipeline.completed`).
6. **Triple count milestones** — weekly/daily delta of total triples per domain. Source: periodic SPARQL or manifest snapshots.

### Data Sources (all existing, no new infrastructure)

| Source | Location | Format |
|--------|----------|--------|
| Spine events | `../messages/logs/chorus.log` | JSON lines |
| Harvest manifests | `data/harvest/manifests/*.json` | JSON |
| HarvestRun triples | Fuseki `/pods/query` | SPARQL |
| Express request log | Loki `{container_name="jeff-bridwell-personal-site-app"}` | Structured |
| CSS audit log | Current source (deprioritize, don't remove) | Pod events |

### UX

- Replace current raw `pod_read` list with filtered meaningful events
- Each event: timestamp, event type icon/badge, one-line description, optional detail expand
- Filter chips by event type (harvest, capture, search, ops)
- Keep it simple — this is a glanceable feed, not a dashboard

### What NOT to do

- Don't build a new event bus or storage layer — read existing sources
- Don't show raw pod_read events (or put them behind a "debug" toggle)
- Don't query Fuseki on every page load for this — cache or use manifest data

### AC

- [ ] Activity tab shows at least 3 event types (captures, harvests, deploys)
- [ ] Raw pod_read events are hidden by default
- [ ] Events are reverse-chronological with readable timestamps
- [ ] Page loads fast — no blocking SPARQL on render
