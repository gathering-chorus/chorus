# Spike: Harvest Toolkit — Generalized ETL for Knowledge Graph

**Card:** #402 | **Owner:** Silas | **Date:** 2026-02-25

## Problem

Every new collection requires bespoke harvest scripting. We've built 4 pipelines (music, photos, sexuality, stories) with copy-paste-modify. Each has the same shape but different wiring. No incremental sync, no built-in observability, no verification step.

## Prior Art (what we've built)

| Pipeline | Source | Transform | Scale | Script |
|----------|--------|-----------|-------|--------|
| Music | Apple Music SQLite | Python → TTL | 71MB | `harvest-music.sh` |
| Photos | Apple Photos SQLite | Python → TTL | ~1.7GB | `harvest-photos.sh` |
| Sexuality | MongoDB (Bedroom) | mongosh → TTL | 1.1M items, ~400MB TTL | `harvest-media.sh` |
| Stories | Chorus index (JSONL) | Wren manual → TTL | 38 files | Manual |

### Common pattern across all four:
1. Connect to source (SSH + local tool: mongosh, sqlite3, jq)
2. Export to TTL (field mapping to ontology classes)
3. Chunk if large (>100K items)
4. Upload to Fuseki (HTTP GSP PUT, per-graph)
5. Verify count (SPARQL vs source)
6. Emit spine event (chorus-log)

## Proposal: `harvest` CLI tool

A team toolkit command that makes any harvest declarative.

### Manifest format (one per collection):

```yaml
# harvests/sexuality.yaml
name: sexuality
source:
  type: mongodb
  host: jeffbridwell@192.168.86.242
  db: media
  collections:
    - name: content
      class: jb:MediaItem
      graph_key: sourceVolume    # one graph per unique value
      fields:
        file_path: jb:filePath
        content_type: dc:format
        base_attributes.size: jb:fileSize
        base_attributes.created: dc:created
    - name: models
      class: jb:Model
      graph: https://jeffbridwell.com/pods/jeff/media/models

target:
  fuseki: http://localhost:3031
  dataset: pods
  graph_base: https://jeffbridwell.com/pods/jeff/media

options:
  chunk_size: 100000
  incremental: true             # track last-sync timestamp
```

### Commands:

```bash
harvest run sexuality          # full harvest
harvest run sexuality --dry    # export only, no upload
harvest run sexuality --incr   # only new/changed since last run
harvest status                 # show all harvests with last-run, counts
harvest verify sexuality       # SPARQL count vs source count
```

### What it replaces:
- `harvest-media.sh` (250 lines) → `harvests/sexuality.yaml` (30 lines)
- `harvest-music.sh` → `harvests/music.yaml`
- `harvest-photos.sh` → `harvests/photos.yaml`

### Built-in:
- Spine events (start/progress/complete/fail)
- Chunked upload with progress
- Incremental sync via timestamp tracking
- Post-load verification
- Dry run mode

## Sizing

**Spike output (1 session):** Evaluate whether YAML manifest + generic runner is viable, or if source diversity (Mongo vs SQLite vs JSONL) makes it impractical. Prototype one manifest (sexuality) and see if the generic runner can replace the bespoke script.

**Build (if spike confirms, 2-3 sessions):** Generic runner in bash or TypeScript, manifest schema, migrate existing harvests.

## Questions to answer in spike

1. Can one runner handle MongoDB, SQLite, and JSONL sources with just config?
2. Is the field→RDF mapping expressive enough without custom code?
3. Can incremental sync work across all source types (Mongo changestreams vs SQLite rowid vs file mtime)?
4. Should this be bash (simple, matches existing) or TypeScript (typed, testable)?

## Recommendation

Bash runner with YAML manifests. Keep it in `messages/scripts/` alongside the other team tools. TypeScript is overkill for what's essentially orchestrated shell commands (SSH + mongosh/sqlite3 + curl).
