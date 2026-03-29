# Harvest-in-Docker Gap — osascript unavailable in container

**From**: Kade (Engineer) → Silas (Architect)
**Re**: #311 — Music harvest fails inside Docker
**Date**: 2026-02-24

## Problem

Music harvester calls `spawn('osascript', ...)` to run JXA extraction from Apple Music. Since the Docker migration (#139), the app runs in Alpine Linux where `osascript` doesn't exist. Result: `spawn osascript ENOENT` — instant failure.

The last successful music harvest was 2026-02-16, before the docker-compose cutover. Photos harvester is unaffected (reads SQLite directly, no osascript).

## Options I See

1. **Host-side extraction script** — Run `osascript` on the Mac, pipe JSON lines to a file or endpoint. Container ingests from that output. Splits the pipeline: extract (host) → ingest (container).

2. **Hybrid bind-mount** — Mount the JXA script + a results directory. A host-side cron or manual trigger runs extraction, writes to the mounted dir. Container watches/reads the output.

3. **Host-side harvest API proxy** — A lightweight process on the host that the container calls out to for extraction. Container sends "extract please" → host process runs osascript → streams results back.

4. **Run harvest outside Docker entirely** — `node` on the host runs the full harvest pipeline, writes Turtle directly to the pod directory (which is bind-mounted).

## My Lean

Option 1 is simplest. The extraction script (`scripts/harvest-apple-music.js`) already outputs clean JSON lines to stdout. We just need to decouple "extract" from "ingest" — run extract on Mac, feed output to container's ingest path.

Photos harvest may have a similar issue if it needs Apple Photos SQLite — need to verify the SQLite path is accessible from inside the container.

## What I Need

Your architectural take on the right pattern. This affects both harvesters (music today, potentially photos) and any future harvesters that need macOS APIs (notes uses JXA too).
