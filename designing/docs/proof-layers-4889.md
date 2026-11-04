# Proof layers, measured (#4889 AC1)

Window: the 30 days to 2026-11-04 17:50 Boston. Sources: the demo witness
(`ops/logs/werk-demo.jsonl`, 612 demo rounds with gates or reviews), werk-test's run
records (`~/.chorus/werk-test-runs/*/verdict`, 588 runs) and borg-prove's land
verdicts (`ops/borg-prove/*.verdict`, 118 lands).

"Alone" means the layer was the only one that did not pass in its round. Each alone
red was sorted into a real defect in the card, or process (an unticked box, a
post-land AC, a role mix-up) or box state (a machine-wide condition such as the
127.0.0.1 binding, not the card's). A random 10 of the process reds were checked by
hand (Kade 17:57): 10 of 10 held.

| # | Layer | Owner | Ran | Not pass | Alone, real defect | Alone, process or box | Notes |
|---|-------|-------|-----|----------|--------------------|-----------------------|-------|
| 1 | Pair green | navigator | per card | — | — | — | a human green; no record of a refused green |
| 2 | Pair local proof | dagu (werk-test steps) | per pair | — | — | — | the same werk-test steps cicd runs again; not measured separately |
| 3 | cicd werk-test | werk-test | 588 | 138 red, 21 unmeasured | the real catches | — | the gates run only after it is green, so no later layer re-catches its reds |
| 4 | Gate: arch | silas | 607 | 21 | 0 | 0 | never the only red |
| 5 | Gate: code | kade | 609 | 80 | 0 | 0 | never the only red |
| 6 | Gate: quality | kade | 609 | 41 | 0 | 2 | one owner mix-up, one process |
| 7 | Gate: ops | silas | 607 | 220 | 0 | 37 | box state: health warnings, the 3470 binding, binary drift |
| 8 | Gate: product | wren | 609 | 358 | 1 (#4465: an AC with no code) | 71 | unticked or post-land boxes; an AC check catches #4465's case only when the AC has a check, and 67 of 118 lands had none (AC2) |
| 9 | Peer reviews (3) | peers | 1529 | 542 | 35 | 10 | concerns, never blocking; the layer that finds code defects |
| 10 | AC verify (borg-prove, demo) | borg-queen | per round | — | — | — | as strong as each AC's `check:` (#4888's checks went missing at 14:41) |
| 11 | Demo session smoke | werk-demo | 518 | 1 fail, 184 none | 0 | 1 | the one fail (#4691) was "no headless agy smoke: unmeasured", not a defect |
| 12 | Navigator vouch | navigator | 17 | 0 refused | 0 | 0 | repeats the pair green |
| 13 | Owner vouch | owner | 25 | 0 refused | 0 | 0 | never refuses either; kept because it is the one place "what is not proven" reaches Jeff's go page (#4888's AC5) |
| 14 | Jeff's go | jeff | 426 | 0 no | — | — | the land authority (DEC-048) |
| 15 | Prod verify (borg-prove, land) | borg-queen | 118 | 0 Not met | 0 | — | 51 Proven, 67 unproven (no checks). A planted failing check reads "Not met" (17:59), so it can go red |

Rows 4–8 are the five gates and row 9 the three reviews: the card's "eleven
layers" counts each as one.

## What this says

- The real defects are caught by werk-test (row 3) and the peer reviews (row 9).
- The five gates found one real defect alone in 30 days. An AC's own check finds
  that case too, but only when the AC has one (67 of 118 lands had none, AC2).
  Their other alone reds are boxes and box state.
- The vouches and Jeff's go never refused; they record decisions, they do not find
  defects. Jeff's go is the land authority and stays.
- Prod verify can go red but never did; 67 of 118 lands had no checks to run. That
  is AC2's gap: every AC names one check.

## Retire (Kade 18:00, 18:03)

| Layer | Why it goes | What covers it |
|-------|-------------|----------------|
| The five gates | one real defect alone in 30 days | the AC's own check (AC2), run in werk-test |
| Navigator vouch | repeats the pair green, never refused | pair green (the navigator's sign) |
| AC verify at the demo | repeats the same checks prod verify runs | werk-test runs each AC's check before the go; prod verify runs them after the land |
| Demo session smoke | no catch in 30 days; its one fail was unmeasured | — |

Kept: pair green, pair local proof (the only red before cicd), cicd werk-test, the
peer reviews, owner vouch, Jeff's go, prod verify. Eleven become seven.

