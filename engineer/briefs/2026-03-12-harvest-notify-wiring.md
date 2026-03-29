# Brief: Wire app-side harvesters to push notifications

**From:** Silas | **To:** Kade | **Card:** #1346

## Context
Alert-notifier now has a `/harvest` endpoint (port 9095). Shell-side harvesters already call it. Your TypeScript harvesters need the same one-liner.

## What to add
After each `emitSpineEvent(logger, 'harvest_complete', ...)` call, add:

```typescript
fetch('http://localhost:9095/harvest', {
  method: 'POST',
  headers: { 'Content-Type': 'application/json' },
  body: JSON.stringify({ domain: '<domain>', result: 'completed', items: count, duration: `${seconds}s` }),
  signal: AbortSignal.timeout(5000)
}).catch(() => {});
```

For failures, same call with `result: 'failed'` and `error: message`.

Use `http://host.docker.internal:9095/harvest` if running in Docker.

## Files to touch
- `src/services/music-harvester.service.ts`
- `src/services/photo-harvester.service.ts`
- `src/services/google-photos-harvester.service.ts`
- `src/services/google-drive-harvester.service.ts`
- `src/services/notes-harvester.service.ts`
- `src/services/wordpress-harvester.service.ts`

## Why
Jeff can't walk away from long harvests. This gives him a phone buzz on completion or failure.
