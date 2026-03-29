# Brief: Auto-update harvest manifests after each pipeline stage

**From:** Silas (Architect)
**To:** Kade (Engineer)
**Date:** 2026-03-02
**Card:** #436 (Music harvest scope) — or create a small child card if you prefer

## Context

Jeff reviewed `/harvest-manifests` and wants manifest data closer to real-time — at least updated between pipeline jobs rather than hand-edited. The view is already 100% manifest-driven (`data/harvest/manifests/*.json`), so if the JSON stays current, the page stays current.

## What

Each pipeline stage should write its results back to the manifest JSON as its last step. No polling, no separate sync job — the stage owns its own update.

### Suggested approach

1. **Add a shared helper** — something like:
   ```ts
   updateManifest(domain: string, sourceId: string, stage: string, results: {
     status: 'complete' | 'in_progress' | 'not_started';
     output_count?: number;
     fuseki_count?: number;
     last_run?: string; // ISO timestamp
     notes?: string;
   })
   ```
   Reads the manifest JSON, finds the source by `id`, updates `stages[stage]`, writes it back. ~20 lines.

2. **Wire it into existing stages:**
   - **Extract** (XML parse / JSONL generation) — after completion, write `extract.output_count` + `extract.last_run`
   - **Transform** (dedup / TTL generation) — write `transform.output_count` + `transform.last_run`
   - **Load** (fuseki-sync) — write `load.fuseki_count` + `load.last_run`
   - **Target** — update `target.fuseki_count` from the load stage result

3. **Reconcile progress** (Source #3 import) — if the import loop already tracks `imported` count, write `reconcile.imported` after each batch.

## What NOT to do

- No new services, no cron, no Prometheus exporter for this
- Don't change the manifest schema — the existing fields are right
- Don't worry about other domains yet — music first, pattern replicates

## Acceptance

- Run a music harvest stage (extract or transform or load)
- Reload `/harvest-manifests?domain=music`
- Counts and timestamps reflect the run that just finished
