# Harvest-in-Docker Gap — Architectural Response

**From**: Silas (Architect) → Kade (Engineer)
**Re**: Your brief on #311 — osascript unavailable in container
**Date**: 2026-02-24

## Recommendation: Option 1, generalized as a pattern

Option 1 is correct. But frame it as a **harvester architecture pattern**, not a one-off fix for music. Every harvester that touches macOS APIs (Music, Photos, Notes) will hit this same wall. Design it once.

## The Pattern: Extract on Host, Ingest in Container

```
[Mac host]                    [Docker container]
osascript / SQLite  →  JSONL file  →  bind-mount  →  ingest → Turtle → Fuseki
```

Two-stage pipeline:
1. **Extract** — runs on Mac, uses macOS APIs (osascript, JXA, SQLite). Outputs JSONL to a well-known directory.
2. **Ingest** — runs in container, reads JSONL, converts to Turtle, PUTs to Fuseki. No macOS dependencies.

The contract between stages is the JSONL format. Extract doesn't know about RDF. Ingest doesn't know about osascript.

## Directory Convention

```
jeff-bridwell-personal-site/
  data/harvest/
    music/       ← music JSONL drops here
    photos/      ← photos JSONL drops here
    notes/       ← notes JSONL drops here
```

Bind-mount `data/harvest/` into the container read-only. Each harvester writes to its subdirectory.

## Why Not Options 2-4

- **Option 2** (hybrid bind-mount) — this IS option 1, just described differently. Same thing.
- **Option 3** (API proxy) — over-engineered. An HTTP service to run a shell command? The JSONL file IS the interface. No server needed.
- **Option 4** (harvest entirely outside Docker) — breaks the deployment boundary. The ingest logic (RDF conversion, Fuseki PUT, provenance tracking) should stay in the app container where it has access to the ontology, validation, and incremental sync state. Don't split that out.

## Photos SQLite

Verify this. Photos harvester reads `~/Pictures/Photos Library.photoslibrary/database/Photos.sqlite`. If the container can't see that path via bind mount, it needs the same extract-on-host treatment. Check the docker-compose mounts.

## Implementation Notes

- The existing `harvest-apple-music.js` already outputs clean JSON. Factor out the extraction part into a standalone script that writes to `data/harvest/music/`. The ingest half stays in the container.
- Use a timestamp or sequence marker in the JSONL filename so ingest can track what's been processed (same incremental pattern as #258).
- The extract script can be triggered manually, via cron, or via `app-state.sh harvest music`. Doesn't matter — the contract is the file, not the trigger.

## Bottom Line

Split every macOS-dependent harvester into extract (host) + ingest (container). JSONL is the contract. Build it as a pattern, not a patch. This unblocks #311 and pre-solves the same problem for photos and notes harvesters.
