# Brief: Hashtag Auto-Routing for SMS Capture (#429)

**From**: Wren | **To**: Kade | **Date**: 2026-02-26
**Card**: #429 — Add hashtag auto-routing to SMS capture
**Type**: commitment | **Size**: small

## What and Why

Jeff captures thoughts via SMS from walks, kitchen, garden. Currently every SMS lands as `pending` and requires manual triage. Adding hashtag parsing lets him auto-route from his phone: `Had a thought about the kitchen #idea` goes straight to Ideas collection, no triage needed.

## Design (kept simple per Jeff + Wren)

- **Single hashtag, anywhere in the message, first match wins**
- **No hashtag = pending** — triage page unchanged, zero risk to current flow
- **Strip the hashtag from stored content** — routing is metadata, not part of the thought
- **Flat routing only** — no colon syntax, no sub-destinations

## Routing Map

```typescript
const HASHTAG_ROUTES: Record<string, string> = {
  'idea': 'ideas',
  'ideas': 'ideas',
  'glimmer': 'glimmers',
  'garden': 'garden-bed',
  'project': 'projects',
  'read': 'reading-list',
  'watch': 'watch-list',
  'wren': 'wren',
  'silas': 'silas',
  'kade': 'kade',
  'team': 'team',
};
```

Case-insensitive. `#Idea` = `#idea` = `#IDEA`.

## Acceptance Criteria

1. SMS with `#idea` anywhere in body → capture created with status `routed`, `routedTo` pointing to new Idea resource
2. SMS with no hashtag → capture created with status `pending` (unchanged behavior)
3. SMS with unknown hashtag (e.g., `#foo`) → capture created with status `pending`, hashtag preserved in content
4. Hashtag stripped from stored `content` field for matched tags only
5. Works with mixed content: `Check this out #idea https://example.com` → type=link, routed to ideas
6. Works with media: photo + `#garden` in body → type=photo, routed to garden-bed
7. Unit tests for hashtag extraction (adapter) and auto-routing (handler)

## Implementation Points

**Hashtag extraction** — in `SmsCaptureAdapter.extractContent()`:
- Regex: `/#(\w+)/i` — first match
- Add to `CaptureContent` interface: `hashtag?: string`
- Strip matched tag from `text` field

**Auto-routing** — in `CaptureHandler.processCapture()`:
- After `extractContent()`, check `content.hashtag` against routing map
- If match: call existing `routeToDestination()` with mapped destination, set status to `routed`
- If no match: proceed as today (status = `pending`)

**Files to touch:**
- `src/adapters/sms-capture.adapter.ts` — hashtag extraction
- `src/interfaces/capture.interface.ts` — add `hashtag?` to CaptureContent
- `src/handlers/capture.handler.ts` — auto-route logic in processCapture()
- `tests/unit/adapters/sms-capture.adapter.test.ts` — extraction tests
- `tests/unit/handlers/capture.handler.test.ts` — routing tests

**Files NOT touched:** triage page, TTL schema, routes, app.ts

## Edge Cases

- Multiple hashtags: first match wins, others stay in content
- Hashtag is the entire message: `#idea` → routed with empty content (valid — sometimes you just want to flag a type)
- Hashtag adjacent to URL: `#idea https://x.com` → type=link, routed to ideas

— Wren
