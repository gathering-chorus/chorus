---
name: cw
description: Start the chorus-werk pipeline for a card — werk v2, the Building value stream (commit → build → test → demo, then Jeff's go, then the land).
user-invocable: true
---

# /cw — Start the werk pipeline for a card

`/cw <card-id>` starts **chorus-werk** for the card: the werk v2 pipeline chorus-make generates from the graph (`platform/pipelines/cicd.yaml`, run by dagu). One tool call. The skill does not run any step itself.

The steps are the pipeline's: werk-commit → werk-push → werk-build → werk-test → werk-review → werk-deploy-demo → env-up → /demo → Jeff's go → the land (cicd-land: werk-merge → werk-sync → werk-deploy-prod → werk-accept).

## Argument

```
CARD_ID=<first argument; required>
```

If no card ID is given, suggest the role's WIP card and ask which to run.

## Step 1: Start it

```
mcp__chorus-api__chorus-werk({ card: <CARD_ID> })
```

The run is enqueued (dagu runs it under launchd, so it outlives this session) and the call returns at once. It runs as you, the verified caller.

Then end the turn. Do not poll. The pipeline nudges you when the demo is ready or a step fails (`/cws <card>` answers where it is if someone asks).

## Step 2: Report

Say what happened: enqueued, or the refusal verbatim. Nothing else.

## Hard rules

- **One call: `chorus-werk`.** Never raw `dagu`, `act`, `git`, or the leaf verbs from this skill.
- **Jeff's go is the pipeline's own go step** (a human task in the run, DEC-048: GO = accept). The skill never gives a go.
- **A failed run is started again with `/cw <card>`** after the fix, once. `/cws` first if unsure whether one is live.

## Pattern lineage

`/cw` is the pipeline-level entry where `/pull`, `/commit`, `/push` are the step entries (Jeff 2026-10-14: `chorus-<product>` starts that product's pipeline; `<product>-<step>` runs one step).
