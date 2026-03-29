# Brief: Wire unclassified spine events into /hooks page + add enforcement tier

**From:** Silas (Architect)
**Card:** #1297
**Priority:** P2
**Context:** Jeff pointed at the existing `/hooks` and `/fitness-functions` Governance pages as where gate observability belongs. The hooks handler already classifies 11 event types — but 13 more gate-related events are emitting to chorus.log and invisible on the dashboard.

## Task 1: Add missing CLASSIFIERS to hooks.handler.ts

File: `src/handlers/hooks.handler.ts`

Add these event→category mappings to the `CLASSIFIERS` array:

| Event | Category (new or existing) | Action |
|-------|---------------------------|--------|
| `card.quality.blocked` | `card-quality` (new) | `block` |
| `card.quality.warned` | `card-quality` (new) | `flag` |
| `card.blast_radius.failed` | `card-quality` (new) | `block` |
| `build.queue.blocked` | `build-gate` (existing) | `block` |
| `build.prepush.timed` | `build-gate` (existing) | `log` |
| `build.prepush.started` | `build-gate` (existing) | `log` |
| `build.push.completed` | `build-gate` (existing) | `log` |
| `build.tsc.completed` | `build-gate` (existing) | `log` |
| `deploy.pipeline.skipped` | `deploy-gate` (new) | `flag` |
| `deploy.skipped` | `deploy-gate` (new) | `flag` |
| `guard.classify.decided` | `sensitive-paths` (new) | decision-based (deny→block, ask→flag, allow→allow) |
| `guard.scrub.blocked` | `credential-guard` (new) | decision-based (deny→block, warn→flag) |
| `ops.alert.fired` | `ops-health` (new) | `flag` |
| `ops.alert.resolved` | `ops-health` (new) | `allow` |

Add new categories to `HookCategory` type and the `categories` array in `buildSummaries()`.

## Task 2: Add enforcement tier badge to hook cards

Each hook card should show a badge: **Enforced** (green) or **Advisory** (amber).

Simple approach: add an `enforcement` field to the category definitions in `buildSummaries()`:

```typescript
{ key: 'search-hierarchy', label: 'Search Hierarchy', description: '...', enforcement: 'enforced' },
{ key: 'card-quality', label: 'Card Quality', description: '...', enforcement: 'advisory' },
```

In hooks.ejs, render the badge next to the category label:

```html
<span class="hook-enforcement hook-enforcement-<%= s.enforcement %>"><%= s.enforcement %></span>
```

CSS: green for enforced, amber for advisory.

## Why this matters

Jeff asked "how do we observe our hit/miss rate on each gate?" The hooks page is already the answer — but it's blind to 13 event types and doesn't distinguish enforced from advisory. These two changes close both gaps with existing infrastructure.

## AC

1. All 14 new event types appear on `/hooks` page with correct category grouping
2. Each hook card shows Enforced or Advisory badge
3. Existing categories unchanged (no regressions)
4. JSON API at `/api/hooks` includes new categories and enforcement field
