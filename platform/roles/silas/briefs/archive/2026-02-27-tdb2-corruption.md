# Brief: TDB2 NodeTable corruption after interrupted ADD GRAPH

**From:** Kade (Engineer)
**To:** Silas (Architect)
**Card:** #504
**Date:** 2026-02-27

## What happened

The VideosNew `ADD GRAPH` operation (13.3M triples) was interrupted when Jeff force-restarted Fuseki to clear the write lock. After restart, all PUT operations to blog/posts graphs fail with:

```
NodeTableTRDF/Read
```

HTTP 500 on 42/43 blog post TTL files. The TTL content is valid (confirmed manually — also fixed a `schema:author 1` bare integer). Notes (823 files) loaded fine in the same session, so the corruption is scoped — possibly to graphs/nodes touched by the interrupted ADD.

## Evidence

- `harvest-sync-fuseki.sh blog/posts` → 1 ok, 42 failed (500)
- Error body: `NodeTableTRDF/Read` (TDB2 node table read failure)
- Notes loaded 823/823 with zero failures immediately after the same restart
- The one blog post that succeeded (`index.ttl` or similar) may not share node table entries with the others

## What I need

1. Is this repairable without data loss? (`tdb2.tdbloader --rebuild`? compact?)
2. Should we just blow away the blog graphs and re-PUT? They're only 43 files, all on disk.
3. Did the interrupted ADD corrupt the VideosNew/sexuality graphs too, or just the node table entries it was writing?

## Impact

Notes and stories load fine. WordPress is the only domain blocked. Low urgency — 43 files, easy to re-load once the store is healthy.
