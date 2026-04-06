# Wren — Next Session

## What Happened (April 5, 2026 — full day)

Marathon session. Origin analysis → service designs → board reckoning → hook architecture → TCC fix.

**Shipped:** Origin analysis HTML, Loom service design, hook architecture standards surface (36 hooks mapped into 6-phase model), observing-value doc. Silas shipped #2225 (search consolidation), #2228 (deep health), #2224 (watchdog fix), #2229/#2100 (TCC — broke and fixed twice). Kade shipped #1820 (board validation tests, 14 green), #2171 (pagination fix via fetchBucketMapFromDB).

**TCC resolved:** Restored chorus-inject binary separation. Shim delegates osascript to stable binary. Jeff granted TCC for chorus-inject. Future shim rebuilds won't revoke. DO NOT rebuild chorus-inject or this breaks again.

## Critical State

1. **Card creation requires --origin, --type, and --desc (or --quick).** Missing any = silent exit code 1. Six cards (#2231-#2236) failed first attempt, succeeded second.
2. **Won't Do pile: 238 cards.** Includes real Done work from bulk-move accident. Needs manual triage — one card at a time.
3. **view() still shows Unknown for overflow cards.** Kade has failing test documenting it. findTaskBucket needs crossref fix.
4. **Chorus API was unreachable** (localhost:3340) for much of the evening. Check on boot.
5. **Clearing card tiles flicker.** Data churn from competing queries. Not fixed.
6. **Session indexer** was dead 2 days (fswatch segfault). Silas fixed root cause (518 junk dirs) + deep-health cron.

## WIP
- #1932 Standards surface — hook architecture HTML deployed at /gathering-docs/chorus-hook-architecture. Needs demo + accept.

## Gap Cards Created (#2231-#2236, sequence:framework)
- #2231 Prompt cycle ID — correlate UserPromptSubmit with PreToolUse
- #2232 Accepted-by attribution — card.accepted records who invoked /acp
- #2233 Nudge input separation — split Jeff text from injected nudges
- #2234 Bulk-move verification — verify each move against API response
- #2235 Consolidate Phase 2/3 — untangle memory_first and search_hierarchy
- #2236 TDD gate HTML exclusion — gathering-docs not production code

## Jeff's Direction — Carry Forward
- **Slow down.** Error → stop → read → understand → fix. Not error → reinterpret → keep going.
- **Verify before reporting.** Exit code 1 means failure. Don't narrate it as a warning.
- **Don't relay role claims.** Silas says X, verify X. Don't repeat it as fact.
- **UI depends on API, period.** No workarounds, no special paths.
- **Stop narrating frustration back.** He knows. Do the work.
- **One card at a time.** No bulk moves. No bulk anything.
- **Navigate by reading code.** Not by polling scripts every 60 seconds.
- **The harness is the product.** 29% stolen prompts, 50+ TCC popups, tools that lie about success. The system that constrains the agents is the thing to build.

## For Next Session
1. Verify chorus-inject survives a shim rebuild with no TCC popup
2. Demo #1932 (hook architecture)
3. Triage Won't Do pile — find Done cards misplaced by bulk move, one at a time
4. Pull from gap cards #2231-#2236
5. Do NOT touch chorus-inject binary or its crate
