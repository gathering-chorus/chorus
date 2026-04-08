# Brief: Wire Chorus System Viz to Ontology Data

**From:** Silas
**To:** Kade
**Date:** 2026-02-22
**Card:** Needs new card — "Wire /chorus/system viz to chorus-system.json"

## Context

The /chorus/system visualization currently has a hand-coded `connections` array in `chorus-system.ejs` (line 373). Every connection is manually defined — no relationship types, no vertebra targeting, just "node → spine."

C#40 formalized the Chorus ontology (v0.2.0). As part of that, I created a JSON data file at `architect/ontology/chorus-system.json` that defines all entities and relationships the viz should render. The viz should read this file instead of using the hard-coded array.

## What To Do

1. **Mount the JSON file** into the Docker container (read-only bind mount in `terraform/environments/dev/main.tf`):
   ```
   /app/data/chorus-system.json → architect/ontology/chorus-system.json
   ```

2. **Add an API endpoint** — `GET /api/chorus/system` that reads and returns the JSON.

3. **Update `chorus-system.ejs`** — Replace the hard-coded `connections` array with a fetch from `/api/chorus/system`. Generate connections from the `relationships` array, using `vizMapping` for line styles.

4. **Add vertebra-level targeting** — Currently all connections go to the "spine" compound node. The ontology defines which vertebra each tool/role connects to. The viz should draw connections to the correct vertebra position within the spine container (e.g., `/look → Capturing`, not `/look → spine`).

## Key Files

- **Ontology data**: `architect/ontology/chorus-system.json` — the source of truth
- **Current viz**: `jeff-bridwell-personal-site/views/chorus-system.ejs` — lines 373-384 are the connections to replace
- **Terraform**: `jeff-bridwell-personal-site/terraform/environments/dev/main.tf` — add volume mount

## Design Notes

- The JSON has a `vizMapping` section that maps relationship types to line styles (solid vs dashed, opacity levels, primary vs secondary affinity opacity)
- `operates-at` relationships have primary/secondary affinity — primary connections should be brighter than secondary
- Tools with `status: "planned"` should render with dashed circles (already done for /talk)
- The Gathering product node is NEW — needs to be added to the viz with output-of and input-to connections

## Not In Scope

- Changing the drag/layout system — that works fine
- Adding new visual node types — reuse existing circle patterns
- Real-time data — the JSON is static, refreshed on deploy
