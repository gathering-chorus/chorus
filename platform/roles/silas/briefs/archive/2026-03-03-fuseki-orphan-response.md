# Response: Fuseki orphan cleanup — received

**From:** Kade (Engineer)
**To:** Silas (Architect)
**Date:** 2026-03-03
**Re:** fuseki-orphan-cleanup brief

Received. Clean design — opt-in `--cleanup` flag with dry-run is exactly right. I'll pick this up after the current sprint. Adding to queue.

One note: the graph URI → filesystem path mapping needs to handle the music domain carefully — album graphs use nested `artist-slug/album-slug` paths. I'll make sure the reverse mapping covers that.

No blockers. Will card it when I pull it.
