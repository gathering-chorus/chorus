---
name: demo
description: The proving ceremony — the demo is its own dagu pipeline (validate, 5 gates + 3 peer reviews in parallel, session-smoke, announce) that cicd calls as one step, then Jeff's go (#4502).
user-invocable: true
---

# /demo — the proving ceremony

`/demo` presents the already-running werk instance to Jeff and asks for his go. It does
**not** build or deploy: the prior steps of the card's run (werk v2,
`platform/pipelines/cicd.yaml`) already did commit → build → test → deploy-demo → env-up.

## The flow (#4502, Jeff 2026-10-18 11:31 "that looks right")

The demo is its own pipeline, `platform/pipelines/demo.yaml` (Jeff 2026-10-12 21:18: "the
demo gets its own dagu pipeline (a demo DAG), and cicd calls it as one step"). Each box is
its own dagu step inside it. /cws names the box a demo stopped at and why; that box can be
retried alone. Jeff's go stays in cicd, after the demo call: dagu refuses a human task in a
sub-DAG. cicd starts the demo from the card's own werk file
(`dagu start <werk>/platform/pipelines/demo.yaml -- CARD ROLE TRACE`), not by name: an
enqueued run finds a pipeline by name on main only, so a card that changes the demo still
runs its own. The demo is then its own "demo" run, its TRACE the cicd run's id.

```
cicd:  … env-up ─► demo ─────────────────────────────────────────────► Jeff's go ─► land
demo:  validate ─► [gate-product, gate-code, gate-quality, gate-arch, gate-ops,
                    review-1, review-2, review-3]   (in parallel)
                ─► session-smoke ─► announce
```

On "more", the demo reruns alone against the card's deployed variant, with no rebuild:
`dagu --config $CHORUS_HOME/platform/config/dagu.yaml start $CHORUS_WERK_BASE/<role>-<card>/platform/pipelines/demo.yaml -- CARD=<card> ROLE=<role> TRACE=`
(an empty TRACE gets a fresh trace; once demo.yaml is on main, `enqueue demo -- …` by name
works too). A stopped demo inside a card's run is retried with the cicd demo step
(`dagu retry --run-id=<cicd run> --step=skill-werk-demo-pipeline --downstream cicd`): the
gates and reviews already recorded for the round are kept, so only the stopped step works again.

| step | command | exit |
|---|---|---|
| validate | `werk-demo validate <card> <role>` | 1 when the card is not WIP/Now or has no AC |
| gate | `werk-demo run-gate <card> <role> <product\|code\|quality\|arch\|ops>` | 0 when a verdict is recorded (pass or concerns); 1 only when the gate could not run |
| review | `werk-demo run-review <card> <role> <1\|2\|3>` | same rule; the peers are every agent role but the builder (wren, silas, kade, abby-normal → 3 each) |
| session-smoke | `werk-demo session-smoke <card> <role>` | 1 when a changed role's session cannot start from the werk build and make one tool call; "nothing to smoke" is a named pass |
| announce | `werk-demo announce <card> <role>` | 1 when any gate or review has no result this round |

Gates and reviews are headless runs (one `claude -p` each, fed the gate's
`skills/gate-<g>/SKILL.md`, or the peer's persona, plus the card AC and the branch diff).
They **inform**; Jeff's go decides (#3263). A gate saying "concerns" never stops the run.

## One ask per round (Jeff 11:33 "really noisy and repetitive"; #3971)

`announce` sends ONE message for the round: card, round, gates, reviews x/3, tests, AC,
where to look, go / no / more. A second announce for the same round sends nothing, and
nothing is sent after Jeff's go is recorded. Nobody is asked to reply: there is no
feedback gather and no waiting on peers.

## Invocation

The pipeline runs these steps; a role does not. To retry a stopped step, use the retry
line /cws prints. `werk-demo <card> <role>` with no step refuses and lists the steps.

## What gates acceptance

The land half reads `demo.presented` and Jeff's go (`werk-demo go`, recorded by the go
step) from the witness, keyed by round and patch. DEC-048 (a builder can't self-accept)
stays enforced in `werk-accept`.

## Rules still enforced

- Validate: card exists, status WIP/Now, has AC.
- Every gate and every peer review has a result for this round before the announce.
- The announce is the ready-gate: if prework fails, there is no announce.
- Jeff's go stays sovereign: gates and reviews inform it, never decide it.
