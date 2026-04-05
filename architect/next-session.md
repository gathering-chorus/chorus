# Next Session — Silas

## Accomplished (2026-04-05 afternoon)
3 cards shipped (#2100, #2101, plus uncarted cleanup), 53→2 test failures fixed.

### Key Outcomes
- #2100: Reverted inject crate — osascript inline in process.rs, nudge survives rebuild
- #2101: Origin tag required at card creation, type inference (fix→reactive, new→reflective)
- Test suite: 53→2 failures. Fixes: wrong paths (seed, jdi-gate, clearing), missing mocks (client, spine-events), tests creating real cards on production board (rewrote to structural tests)
- Reverted accidental commit of Kade's #2171 client.ts changes
- Cleaned 46 junk cards from board (test pollution)
- Disabled watchdog (false alerts interrupting Jeff ~20x/day, #2224 carded)
- TCC Documents fix: session_cache only scans chorus project dirs

### Lessons This Session
- "Pre-existing, not mine" is not acceptable — trace and fix
- Tests that hit production APIs create real data — structural tests or error-path tests only
- `git add` by directory sweeps in other roles' in-progress work — be specific
- Don't declare things dead (Clearing) without checking if they're running
- The Clearing is at localhost:3470, NOT Bridge. Renamed weeks ago.
- Don't card things to skip them — fix them now

## Next Session
- #2224: Watchdog fires on active roles (disabled, needs team-scan activity check)
- origin-labels.test.ts: 1 test depends on card #2087 existing on board (fragile)
- Kade's #2171 client.ts changes need proper landing with matching test updates

## Carry Forward
- Wren flagged 132 unsequenced aging cards, post-accept sweep gap
- Documents TCC prompt may still fire from other code paths — audit needed
