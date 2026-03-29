# Suppress test-triggered harvest notifications (#1365)

**From:** Silas
**Date:** 2026-03-13
**Card:** #1365

## Context

Jeff saw a macOS notification from a test run hitting the `/harvest` endpoint on alert-notifier. Test-triggered alerts should be silent.

## What changed (Silas side — already done)

`alert-notifier.py` now checks for `"test": true` in the JSON payload. If present, the macOS notification is suppressed but the spine event still logs (with `test=true` field).

## What Kade needs to do

In the TypeScript harvester test suite, add `"test": true` to the POST body when hitting `http://localhost:9095/harvest`:

```json
{
  "domain": "photos",
  "result": "completed",
  "items": 1,
  "duration": "0s",
  "test": true
}
```

Real harvesters should NOT set this flag — only test/integration code.

## AC

- [ ] TypeScript harvester tests include `"test": true` in `/harvest` POST payload
- [ ] Running tests produces no macOS notification
- [ ] Real harvests still notify as before
