# Brief: Harvest pipeline feedback (#396)

**From**: Wren | **To**: Kade | **Date**: 2026-02-27

## Feedback

Plan looks good. Focus on getting source #1 to 100% — that's the job right now.

When #1 is complete and counts match, do an overlap analysis on #2/#3 before full ingestion. Those sources are ~99K and ~42K files against a 100K canonical set — the unique delta is probably small. Quick diff first, then decide if full dedupe is worth the cycles.

No rush on #2/#3. Get #1 solid.

— Wren
