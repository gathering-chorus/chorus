---
name: demo
description: The proving ceremony — the demo is its own dagu pipeline (validate, the code, architecture and quality reviews in parallel, announce) that cicd calls as one step, then Jeff's go (#4502).
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
demo:  validate ─► [gate-code, gate-arch, gate-quality]   (in parallel) ─► announce
```

On "more", the demo reruns alone against the card's deployed variant, with no rebuild:
`dagu --config $CHORUS_HOME/platform/config/dagu.yaml start $CHORUS_WERK_BASE/<role>-<card>/platform/pipelines/demo.yaml -- CARD=<card> ROLE=<role> TRACE=`
(an empty TRACE gets a fresh trace; once demo.yaml is on main, `enqueue demo -- …` by name
works too). A stopped demo inside a card's run is retried with the cicd demo step
(`dagu retry --run-id=<cicd run> --step=skill-werk-demo-pipeline --downstream cicd`): the
reviews already recorded for the round are kept, so only the stopped step works again.

| step | command | exit |
|---|---|---|
| validate | `werk-demo validate <card> <role>` | 1 when the card is not WIP/Now or has no AC |
| review | `werk-demo run-gate <card> <role> <code\|arch\|quality>` | 0 when a verdict is recorded (pass or fail); 1 only when the review could not run |
| announce | `werk-demo announce <card> <role>` | 1 when any review has no usable result this round |

The reviews are headless runs (one `claude -p` each, fed `skills/gate-<g>/SKILL.md` plus
the card and the branch diff). They read the **diff**, never the AC boxes (Jeff 11-04
18:11). A failed review does not stop the run: the owner's brief names it, and nothing
goes to Jeff while a review is red (the owner's vouch refuses). Jeff's go decides (#3263).

#4889 (Jeff 11-04 18:21) retired the product and ops gates, the three peer reviews and
the session smoke. Where each of their checks went is in the "Where the old checks went"
table of `/gate-code`, `/gate-arch` and `/gate-quality`.

## One ask per round (Jeff 11:33 "really noisy and repetitive"; #3971)

`announce` sends ONE message for the round: card, round, reviews, tests, AC,
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
- Every review has a result for this round before the announce.
- The announce is the ready-gate: if prework fails, there is no announce.
- Jeff's go stays sovereign: the reviews inform it, never decide it.
