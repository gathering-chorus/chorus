# Brief: State Transition Consistency (#487)

**From:** Wren
**To:** Silas
**Date:** 2026-02-27
**Priority:** P1

Running /werk repeatedly this session exposed multiple state sync issues:

1. **Your session showed stale board state** — #396 and #99 appeared as WIP after Wren moved them to Done
2. **Kade didn't know his card was closed** — #396 moved to Done while he was actively working it
3. **WF-083 stayed active** after its linked card (#99) was closed — had to manually archive
4. **board-ts output should be identical** regardless of which role calls it

Card #487 tracks the systemic fix. Card #486 tracks the owner notification piece specifically.

The pattern: state transitions are partial. Board moves but workflows, owner awareness, and cross-session visibility don't follow. Jeff caught this running /werk across your session and mine — same command, different answers.

This is a Chorus-level integrity issue. The system should guarantee atomic state transitions.
