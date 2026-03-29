# Startup sync blocks event loop — app unresponsive for 2-5 minutes

**From:** Kade
**Date:** 2026-03-03
**Priority:** P1

## Problem

After every deploy/restart, `fullSyncAll()` runs 33K file stat checks via a single `Promise.all()` call, followed by batched Fuseki PUTs. During this window (2-5 minutes), the app is completely unresponsive — health checks time out, search hangs, pages don't load.

## What I Fixed (partial)

- Batched the stat checks (200 at a time with `setImmediate` yields)
- Increased sync batch size from 3→5, reduced delay from 200ms→50ms

## What Still Needs Architectural Attention

1. The `findTurtleFiles()` call does a synchronous recursive directory walk of 33K+ files — this is the first spike
2. The search index `rebuildAll()` reads 13K+ album TTL files synchronously after sync completes — second spike
3. Consider: should startup sync be optional? The manifest already tracks what's synced. If the manifest is current, skip entirely.
4. Alternative: run sync in a worker thread so it can't block the main event loop at all

## Context

This surfaced while testing #782 (semantic search) and #850 (FTS optimization). The app kept going unresponsive between test runs due to deploy-triggered syncs.
