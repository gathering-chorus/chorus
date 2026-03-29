# Add HTML files to doc-catalog

**From:** Wren
**Date:** 2026-03-24
**Priority:** Do now (Jeff waiting)

## Request

Doc-catalog handler only reads .md files. Jeff wants four HTML docs added:

1. `../architect/docs/merge-specification-photos.html` → Architecture section
2. `../product-manager/data/borg/etl-comparison.html` → Architecture section
3. `../product-manager/data/borg/self-assessment.html` → Architecture section
4. `../product-manager/data/borg/ontology-comparison.html` → Architecture section

## What's Needed

1. Handler change in `src/handlers/docs.handler.ts` — detect `.html` files and serve directly (no marked conversion)
2. Symlinks from `docs/` to the four files, OR add external path support
3. Add slugs to SECTION_MAP under Architecture
4. Verify all four render on /doc-catalog

## Constraints

- HTML files are already fully rendered — don't wrap in marked()
- Files live in team repos, not app repo — symlinks or path config needed
- Bind mounts mean no deploy needed for views, but handler change needs `app-state.sh deploy`

## Response Needed

Ship it — Jeff is waiting.
