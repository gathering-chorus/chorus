# Harvest Design Doc: Notes

**Scope card**: #442
**Owner**: Wren / Kade
**Date**: 2026-02-27
**Status**: Draft — needs review before next harvest run

## End-to-End Flow

```
Apple Notes (macOS)
       │
       ▼
   ┌────────┐     ┌────────┐     ┌────────┐     ┌──────────┐
   │Extract │────▶│Transform│────▶│  Load   │────▶│Reconcile │
   │ (JXA)  │     │ (dedup) │     │ (TTL)   │     │ (delta)  │
   └────────┘     └────────┘     └────────┘     └──────────┘
    823 notes       ~823 clean    823 TTL files   source=target?
    47 folders      dedup by       in pods/        deleted notes
                    title+created  jeff/notes/     detected?
```

**Current state**: Extract → Transform → Load works. Reconcile doesn't exist.

## Source

| # | Name | Location | Machine | Format | Count |
|---|------|----------|---------|--------|-------|
| 1 | Apple Notes | Notes.app SQLite / JXA | Library (192.168.86.36) | JSONL via osascript | 823 |

Single source. No backup or secondary copy to dedupe against.

## Fields

| Field | Extract? | Why / Why Not | Transform |
|-------|----------|---------------|-----------|
| title | ✅ | Primary identifier | Slugified for filename |
| body | ✅ | Content | Stored as-is (rich text stripped by JXA) |
| created | ✅ | Part of dedup key | ISO 8601 |
| modified | ✅ | Change detection (future) | ISO 8601 |
| folder | ✅ | Organization / filtering | Stored as string |

**Explicitly skipped fields (and why):**
- Attachments / embedded images — JXA can access them but they're heavy and scope is text-first
- Note ID (Apple internal) — not extracted, would help with rename detection
- Shared status — not relevant (all private)

## Transforms

- **Dedup key**: `lowercase(title) + created timestamp`
- **Slug generation**: `YYYY-MM-DD-slugified-title`
- **No enrichment** — notes are stored as extracted

## What's Missing (End-to-End Gaps)

### 1. No delta detection
Current: full extract → full write every run. 823 notes rewritten whether changed or not.
**Fix**: Compare `modified` timestamp against existing TTL. Only rewrite if modified > stored modified.

### 2. No delete detection
Current: if a note is deleted in Apple Notes, the TTL file persists forever.
**Fix**: After extract, compare slugs against existing TTL files. Flag orphans. Don't auto-delete — surface them for Jeff to decide.

### 3. No rename detection
Current: if a note title changes, a NEW TTL file is created (different slug) and the old one persists.
**Fix**: Extract Apple Note ID and store it. Use ID as stable key, title as display name. This requires a JXA script change.

### 4. No modified-only re-ingest
Current: can't tell which notes changed since last run.
**Fix**: Store last harvest timestamp. On next run, compare modified dates. Only process notes where `modified > lastHarvestTime`.

### 5. No spine events
Current: harvest runs silently.
**Fix**: Emit `harvest.pipeline.started` / `harvest.pipeline.completed` via chorus-log.sh.

## Acceptance Test

| Check | Method | Pass Criteria |
|-------|--------|---------------|
| Count match | SPARQL count vs JXA extract count | source = Fuseki ± explained delta |
| Folder coverage | SPARQL distinct folders | all 47 folders present |
| Spot check | 5 random notes | title, date, body, folder render correctly |
| Delta detection | Modify 1 note, re-harvest | only that note re-ingested |
| Delete detection | Delete 1 note, re-harvest | orphan flagged |
| Spine events | Check chorus.log after harvest | start + complete events with counts |

## Estimated Run Time

- Extract: ~2 min (823 notes via JXA, no artwork)
- Transform: <1 sec (in-memory dedup)
- Load: ~30 sec (823 TTL file writes)
- Total: ~3 min

## Risks / Open Questions

- **Apple Note ID stability**: Does the internal ID survive sync/iCloud changes? If not, rename detection breaks.
- **Rich text fidelity**: JXA strips formatting. Is plaintext body sufficient or do we need HTML?
- **Docker gap**: Same as music — JXA requires macOS host. Pre-extract + file ingest pattern works but adds a manual step.

## Prior Art

- Last full harvest: 2026-02-22, 823 notes, no errors reported
- No known data loss or corruption from full-replace approach
- Music pipeline (DEC-062): artwork bottleneck killed a 22-hour run. Notes extraction is fast (~2 min) so full-replace isn't as costly, but delta detection still matters for correctness (delete/rename cases)
