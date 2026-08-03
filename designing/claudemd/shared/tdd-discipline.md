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

## No gate without a negative proof (#3734)

**A check that gates anything must ship with a NEGATIVE PROOF: a fixture where the guarded condition is VIOLATED and the check is shown to FAIL. No proof, no gate.** Reviewable at `/gate-code`.

This is stricter than red-green above, and it exists because red-green was not enough. Step 3 asks you to watch a test fail against *missing code*. This asks you to watch a check fail against a *violation* — the state it exists to catch. A gate can go red because the feature is absent and still be incapable of going red once the feature exists.

**Why mechanical and not a question.** Eleven instances of one shape — *a check that cannot distinguish the two states it exists to separate* — landed in eight weeks of a single subsystem: a substring assert that matched the negation of its own rule, a threshold counting content it never meant to include, a skip branch incrementing the pass counter, a verdict computed over whichever subset happened to report. The pattern was named repeatedly — in commit messages, in TD-024, in nudges both directions — and recurred *after* being named, by the people who named it. A question in a contract is a prompt to reflect, and reflection is what fails under deadline.

**The worked case, because it is the whole argument.** On #3725 the first fixture written to prove a block-extractor still caught a violation was itself bogus: the comment marking the omission contained the very string being grepped, so it "passed" for the wrong reason. No amount of asking *would this check be wrong?* catches that. Running it did, in seconds.

**What this does NOT cover.** Checks we author deliberately — assertions, gates, ratchets, guards. It would not have caught `write_all` without a flush, which was a correct-looking call to a buffered API: a wrong assumption about a dependency's semantics, needing a different defence. A contract claiming to cover both would be its own hollow gate.

**In practice:**
- When a check goes GREEN, ask what state it would have to be in to go RED, and confirm that state is reachable.
- When a check goes RED, do not widen the assertion — find which two states it can no longer separate.
- A guard whose target is deleted or renamed must fail loudly, never pass vacuously.
