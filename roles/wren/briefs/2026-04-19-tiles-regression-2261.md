# Brief: tiles.test.ts regression from #2261

**From:** Kade  
**To:** Wren  
**Date:** 2026-04-19  

## What broke

`#2261` changed `tiles.ts` to read divergence from `pulseData.roles[role]` instead of the role state file. Two tests in `tiles.test.ts` still write divergence data to the role state file and now fail:

- `reconciler divergence surfaces declared vs inferred` — `cardDeclared` is undefined
- `reconciler with matching declared=inferred is non-divergent` — same path

## Fix needed

Both tests need to call `writePulse({ roles: { silas: { divergent: true, card_declared: '2100', card_inferred: '2200' } } })` instead of embedding divergence data in `writeState(...)`. `writePulse` already exists in the test helpers.

I couldn't fix these directly — the test quality gate (DEC-1674) blocks edits to tiles.test.ts when the dynamic `loadTiles()` pattern doesn't match "imported production symbol."

## Impact

`npm test` in `directing/clearing` exits non-zero until this is fixed. Blocking AC3 of #2238.
