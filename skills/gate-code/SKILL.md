---
name: gate-code
description: The code review's one addition to Claude Code's /code-review and /security-review — whether each AC's check really tests that AC. Runs on every card. Never counts AC boxes.
user-invocable: true
fail-items: 1-2
---

# /gate-code — Code review: do the checks test the AC

Jeff 2026-11-04 18:21: "one code review". Jeff 19:04 ("fine do that"): the code review
**is** Claude Code's `/code-review` (correctness, on the whole branch, read with its own
tools) with `/security-review` beside it. The demo's code step runs both, then this
skill, and fails when any of the three finds a defect (#4889). This skill holds only
what those two do not read. It is not a copy of them: correctness, design and tests are
theirs.

## Arguments

```
/gate-code <card-id>
```

## Input

The card text (its AC lines, each ending `check: <kind> <target>`) and the card's branch
diff (`origin/main...HEAD`). In the demo both are handed to you; by hand,
`git -C <werk> diff origin/main...HEAD`.

## What to look for (Jeff 18:22–18:25)

1. **Does each AC's check test that AC?** For every AC line, read its `check:` target in
   the diff (or the repo). A check that exercises something other than what its AC
   says, only a fixture of its own, or nothing that can fail, is a finding. Name the AC
   and the check file.
2. **One check per AC.** No two AC may share one check file. (The demo's validate
   already refuses a shared or missing check; a finding here is a check that is
   distinct by name but tests the same thing.)

## Report

Each finding names its item (1 or 2), where (`<check file>:<line>`) and what (`AC <n>: ...`).
The output format is the demo's, given after this skill.
Nothing about AC ticks or wording. The verdict is derived from the findings: any
finding fails.

## Where the old checks went (#4889, Kade 18:34: fix, never throw away)

| Old check | Now |
|-----------|-----|
| Fagan defect classes (logic, interface, data, error handling, concurrency) | `/code-review`, on the whole branch |
| Google's questions (functionality, complexity, tests, naming) | `/code-review` |
| Security (injection, secrets, auth) | `/security-review` |
| Tests green, build clean, warning count | werk-test: the card's one test run builds and tests every unit the diff touches |
| execSync lint | werk-test's lint ratchet |
| Negative proof for every new gate (#3734) | `/gate-quality` item 1 |
| Known fails counter | werk-test's nightly red list |
| The old floor (werk-review: unchecked boxes, src without tests, removed symbols) | removed symbols and missing tests: `/code-review`; boxes: never a review's job (Jeff 18:11) |
