---
name: gate-code
description: Code review of a card's diff on Google's engineering practices and Fagan inspection — correctness, design, tests, and whether each AC's check really tests that AC. Runs on every card. Never counts AC boxes.
user-invocable: true
---

# /gate-code — Code review

Jeff 2026-11-04 18:11: "review is supposed to be code review not ac checker". This is
the card's **one code review** (#4889). It reads the **diff** as a reviewer does, on
every card. It never counts or ticks AC boxes; the AC are proven by their own checks
in prod. Its bar (Jeff 18:38): catch what the old peer reviews caught, the 35 real
defects of 2026-10, not only planted ones.

Grounded in two practices:
- **Google engineering practices, "What to look for in a code review"**: design,
  functionality, complexity, tests, naming, comments, style, documentation, every line.
- **Fagan inspection**: read against a checklist of defect classes, record each defect
  with its place and class, judge the work product, not the author.

## Arguments

```
/gate-code <card-id>
```

## Input

The card's branch diff (`origin/main...HEAD` in the card's werk) and the card text. In
the demo both are handed to you; by hand, `git -C <werk> diff origin/main...HEAD`.

## Read every changed line for (Fagan defect classes)

1. **Logic.** Wrong condition, off-by-one, inverted check, a branch that can never run,
   a value that is not what its name says.
2. **Interface.** A changed signature or return type with callers the diff does not
   update; a contract (JSON field, exit code, event name) changed on one side only;
   sync and async mixed.
3. **Data.** A default that hides a missing value; a null that means two things
   (absent and failed); a hardcoded value that belongs in config; unescaped input.
4. **Error handling.** A failure swallowed, retried forever, or reported as success.
5. **Concurrency and state.** Shared state written from two places; ordering assumed
   but not enforced; a cache with no invalidation.

## Then, Google's questions

6. **Functionality.** Does the code do what the card says, for the user it names?
7. **Complexity.** Could it be simpler? Is anything built for a future that is not
   asked for?
8. **Tests.** Are there tests for the change, and would they fail if the code broke?
9. **Naming and comments.** Do names say what things are; do comments say why, and
   still match the code?

## The AC checks (#4889, Jeff 18:22–18:25)

10. **Does each AC's check test that AC?** For every AC line, read its `check:` target
    in the diff (or the repo). A check that exercises something other than what its AC
    says, only a fixture of its own, or nothing that can fail, is a finding. No two AC
    may share one check file.

## Report

Each finding: `file:line — class — what — why it matters`. Nothing about AC ticks or
wording.

## Verdict

- **fail**: a finding in classes 1–5, a test that cannot fail (8), or an AC whose check
  does not test it (10).
- **pass**: otherwise; list the rest as notes.

A diff you could not read is an **error**, never a pass.

## Where the old checks went (#4889, Kade 18:34: fix, never throw away)

| Old check | Now |
|-----------|-----|
| Tests green, build clean, warning count | werk-test: the card's one test run builds and tests every unit the diff touches |
| Pattern match (read before write) | item 7, on the diff |
| execSync lint | werk-test's lint ratchet |
| Negative proof for every new gate (#3734) | item 8 here, and item 1 of `/gate-quality` |
| Known fails counter | werk-test's nightly red list |
