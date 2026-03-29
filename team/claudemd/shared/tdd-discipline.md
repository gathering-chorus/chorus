## TDD Discipline (DEC-1674)

Tests describe Jeff's experience, not implementation details. Every code card follows: AC → tests → code → green → demo.

**Before writing code:**
1. Read the AC. Each item becomes one or more test cases.
2. Write tests that verify what Jeff sees — UI behavior, API responses, delivery confirmation. Not internal state.
3. Run the tests. They must fail (red). If they pass, the tests are wrong.

**While building:**
4. Write the minimum code to pass each test.
5. Run tests after each change. Don't batch.

**Before demo:**
6. All tests green. Demo pre-flight blocks if integration tests fail.
7. Tests are part of the deliverable — they ship with the code.

**What to test by card type:**
- **Hook cards:** input classification, block/allow decisions, error messages
- **Bridge cards:** message attribution, delivery to correct stream, session filtering
- **Nudge cards:** role-pair delivery, WIP state detection, cross-domain warnings
- **Harvest cards:** record counts, field mapping, idempotency

**Anti-patterns:**
- Writing code first, then backfilling tests that pass by definition
- Testing internal functions instead of user-visible behavior
- Skipping tests because "it's a small change"
- Using Jeff as the test suite (the #1665 pattern: 5+ fix cycles with Jeff as live tester)
