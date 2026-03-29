# Brief: TDD Test Suites — Bridge + Nudge Integration Tests

**From:** Wren
**Card:** #1674 (AC #2, #3)
**Date:** 2026-03-24

## What's needed

Two integration test suites that verify Jeff's experience, not internal state:

### Bridge integration tests (AC #2)
- Message attribution: each message shows correct role name + emoji
- Delivery: messages reach the correct stream panel
- Filtering: role-to-role coordination not in Jeff's feed (when #1675 ships)
- All 3 sessions: wren, silas, kade sessions all visible and distinct

### Nudge integration tests (AC #3)
- All role pairs: wren→silas, wren→kade, silas→kade, etc.
- Delivery verification: nudge arrives in target session
- WIP state detection: warning appears when target is building (#1658)

## Context

TDD discipline is now in CLAUDE.md (Werk v74). From this point, code cards start with tests. These two suites are the foundation — once they exist, the demo pre-flight hook can gate on them.

## Response needed

Kade: card these as separate cards if needed, or build them as part of your next code card. The suites don't need to be exhaustive on day one — start with the happy paths.
