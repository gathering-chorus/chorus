# Brief: Drive Media Census — #1364

**From:** Wren
**Date:** 2026-03-13
**Card:** #1364
**Type:** implementation spec
**Priority:** P2 — pick up after harvest freeze lifts

## Context

Drive harvest produced 491K "documents" but 316K are media (280K photos, 29K videos, 6K music) that likely overlap with existing Photos and Music domains. Jeff needs overlap numbers before deciding how to route anything. This is Phase 1 — read-only census, no mutations.

## What to build

### 1. `src/services/drive-analysis.service.ts`
Analysis service. Core logic:

1. Call `DocumentPodService.getDocumentIndex()` — already builds in-memory index with `contentCategory` on every doc
2. Group by contentCategory (photo/video/music/other/doc/etc)
3. Pull photo filenames from Photos graph via SPARQL (`SELECT ?filename WHERE { GRAPH <photos-graph> { ?s jb:filename ?filename } }`)
4. Pull music filenames from Music graph similarly
5. In-memory `Set` intersection to count filename overlaps
6. Cache results with 5 min TTL (same pattern as DocumentPodService uses)

Memory budget: ~280K filenames × ~50 bytes = ~14MB. Photos similar. ~30MB total. Fine on 16GB M1.

### 2. `src/handlers/drive-analysis.handler.ts`
Mounts at:
- `/admin/drive-analysis` — HTML report (EJS view)
- `/api/drive-analysis` — JSON response

Follow the handler pattern from `document.handler.ts`.

### 3. `views/admin-drive-analysis.ejs`
Report page showing:
- Drive media by type (photo: 280K, video: 29K, music: 6K)
- Filename overlap with Photos domain: X matches / Y unique to Drive
- Filename overlap with Music domain: X matches / Y unique to Drive
- "Truly new" count per category

## Key files to reuse
- `DocumentPodService.getDocumentIndex()` — `src/services/document-pod.service.ts`
- `contentCategoryFromMime()` — `src/services/document-pod.service.ts:43`
- Handler pattern — `src/handlers/document.handler.ts`
- SPARQL graph URIs — `src/constants/sparql-constants.ts`

## Acceptance criteria
1. `/admin/drive-analysis` loads and shows media breakdown by type
2. Overlap counts shown for Photos and Music domains
3. "Truly new" count per category visible
4. Counts cross-check against content type chips on `/documents`
5. No mutations to any graph — read-only

## Sequencing
This is Phase 1 of 4. Phases 2-4 will be carded separately after Jeff reviews these numbers:
- Phase 2: Confidence-scored per-item matching (exact/fuzzy/probable)
- Phase 3: Routing actions (after Jeff reviews Phase 2)
- Phase 4: Opaque binary classification (119K `application/octet-stream` items)

## Response needed
None — pick this up when harvest freeze lifts. Card is in Next.
