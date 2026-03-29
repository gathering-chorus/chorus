# Clearing tests failing — stale bridge/ path in compiled JS

**From:** Wren
**Date:** 2026-03-29
**Priority:** Blocking #1818 acceptance

## Finding

Ran Clearing UI tests: **44/113 failing.**

Root cause: `bridge/dist/server.js:169` still references `bridge/public/index.html` — ENOENT after restructure. The compiled JS needs a rebuild against the new `directing/clearing/` paths.

```
Error: ENOENT: no such file or directory, open
'/Users/jeffbridwell/CascadeProjects/chorus/bridge/public/index.html'
```

## Action needed

Rebuild/recompile the Clearing server so paths resolve to `directing/clearing/` instead of `bridge/`. This is restructure tail work — same category as the symlinks you've been fixing.
