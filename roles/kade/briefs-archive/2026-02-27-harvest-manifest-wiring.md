# Brief: Wire harvest scripts to auto-update manifests

**From:** Silas (Architect)
**To:** Kade (Engineer)
**Date:** 2026-02-27
**Related cards:** #402 (spike, closing), #396 (music pipeline)

## What was built

Harvest manifest system — JSON state files per domain + a `harvest` CLI for status visibility.

- **Manifests:** `data/harvest/manifests/*.json` — one per domain (music, photos, notes, sexuality, stories, wordpress)
- **CLI:** `scripts/harvest` — `harvest` (dashboard), `harvest <domain>` (detail), `harvest update <domain> <stage> <field> <value>`, `harvest gaps`
- **Structure:** Each manifest tracks 4 stages: extract → transform → load → verify, with status, last_run, counts, method, notes, gaps

## What you need to do

Wire the harvester scripts/services to auto-update their manifest as they run. The goal: when a harvest stage completes, the manifest reflects it without anyone having to manually update.

### Music harvester (priority — you're already in #396)

In `music-harvester.service.ts`, at the end of each stage:

```bash
# After extract completes:
scripts/harvest update music extract status complete
scripts/harvest update music extract last_run "$(date -u +%Y-%m-%dT%H:%M:%SZ)"

# After transform (TTL write) completes:
scripts/harvest update music transform status complete
scripts/harvest update music transform output_count <N>

# After Fuseki sync:
scripts/harvest update music load status complete
scripts/harvest update music load fuseki_count <N>
```

You can either shell out from the TypeScript service or write the JSON directly — your call on what's cleaner. The manifest is just a JSON file at `data/harvest/manifests/music.json`.

### Other harvesters (lower priority, when you touch them)

Same pattern for photos, notes, etc. One line per stage completion.

### harvest-media.sh (sexuality)

This one lives in `architect/scripts/` — I'll wire it myself since it's my domain.

## Acceptance criteria

- Running a music harvest end-to-end updates `music.json` manifest automatically
- `scripts/harvest` dashboard reflects the real state after a run
- No manual `harvest update` calls needed for music pipeline
