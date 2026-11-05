---
name: gate-quality
description: Quality review of a card's diff — hollow checks, error paths, dead code, tests that test the change. Runs on every card. Never reads AC boxes.
user-invocable: true
fail-items: 1-3, 7
---

# /gate-quality — Quality review

Jeff 2026-11-04 18:11: "review is supposed to be code review not ac checker and same
for quality and architecture". This gate reviews the **diff** for quality. It runs on
**every** card. It never reads, counts or judges AC boxes: the AC are proven by their
own checks in prod, not here.

Grounded in two practices:
- **Test desiderata (Kent Beck)**: tests are isolated, composable, deterministic, fast,
  writable, readable, behavioural, structure-insensitive, automated, specific,
  predictive and inspiring. Read each changed test against them.
- **Mutation testing**: a test suite is as good as the mutants it kills. For each
  changed line, ask whether flipping a condition, dropping a call or changing a constant
  would turn a test red. A mutant that survives is a missing test.

## Arguments

```
/gate-quality <card-id>
```

## Input

The card's branch diff (`origin/main...HEAD` in the card's werk), and the card text
for context only. In the demo the diff is handed to you; by hand, read it with
`git -C <werk> diff origin/main...HEAD`.

## What to look for in the diff

1. **Hollow checks (#3734).** A test, gate or guard that cannot go red: it greps a
   string its own comment contains, counts a skip as a pass, passes when its target is
   missing, or asserts only on an empty input. Does every new check ship a negative
   proof that fails on the violation?
2. **Error paths.** A failure swallowed silently (`|| true`, empty catch, an ignored
   exit code), or a fallback that hides a real failure as a success.
3. **Tests that test the change.** Do the changed tests exercise the changed code, or
   only a fixture of their own? Were tests deleted or skipped without a replacement?
4. **Dead code.** Code, flags, files or helpers left unused by this change.
5. **Debug leftovers.** `console.log` in production paths, commented-out code, TODOs
   that hide a gap the card claims to close.
6. **Observability.** A new failure mode with no spine event or log line to find it
   by; a changed handler, route or service with no structured log call.
7. **Mutants.** Pick the riskiest changed lines; would a mutant of each (inverted
   condition, deleted call, off-by-one) survive the changed tests? Name the line and the
   surviving mutant.
8. **Desiderata.** A changed test that is nondeterministic (time, order, network,
   shared files), structure-sensitive (asserts on internals, not behaviour) or not
   specific (a failure would not say what broke).

## Where the old checks went (#4889, Kade 18:34: fix, never throw away)

| Old check | Now |
|-----------|-----|
| Hooks pass (chorus-hooks build current) | werk-test: the card's run builds and tests every crate the diff touches |
| Regression (tests deleted or skipped) | item 3, on the diff; werk-test still refuses a suite deleted without its unit |
| console.log in production code | item 5, on the diff |
| Observability present | item 6, on the diff |
| Smoke check (the app answers) | werk-test and the demo's env-up, which start the werk's services and fail when they don't answer |
| Demo session smoke (each changed role signs in and makes one tool call) | werk-test: `platform/tests/role-session-smoke.bats`, in the card's one test run |
| Bats (shell-script integration tests) | werk-test: every bats suite the diff touches runs in the card's one test run |
| "Any new technical debt?" | the whole review |

Report only what you can point to in the diff: each finding is
`file:line — what — why it matters`. No finding about the card's AC boxes, ticks or
wording.

## Verdict

- **fail**: at least one finding in 1–3 or 7 (a hollow check, a swallowed failure, tests
  that do not test the change, or a surviving mutant on a risky line).
- **pass**: otherwise; list minor findings as notes.

A diff you could not read is an **error**, never a pass.
