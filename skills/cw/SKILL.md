---
name: cw
description: Run the chorus_werk pipeline for a card — the Building value-stream runner (build→deploy→demo, then GO=accept).
user-invocable: true
---

# /cw — Run the chorus_werk Pipeline (Building value-stream runner)

`/cw <card-id>` runs the **chorus_werk** pipeline for a card — the whole Building value stream (commit → push → build → test → deploy-werk → env-up → demo) to the demo stop, then PRESENTS the running variant. `/cw <card-id> go` resumes past the stop (merge → ff-sync → deploy-prod → accept). **One MCP call. The skill does NOT execute the pipeline steps directly — `chorus_werk` does** (it drives `act`/`werk.yml`; never shell out to `act`).

This is the runner sibling of the leaf-verb skills (`/pull`→`werk-pull`, `/commit`→`werk-commit`, `/push`→`werk-push`): `/cw`→`chorus_werk`, the one trigger that sequences the verbs.

## Argument

```
CARD_ID=<first argument; required>
GO=<optional second token: the literal `go` resumes past the demo stop to land>
```

If no card ID given, suggest the role's current WIP card (the one in flight) and ask which to run.

## Step 0: Pre-flight (caller side)

The skill's only job is collecting the args and invoking the MCP. Verify before the call:

1. **Card ID supplied or chosen.** No call without a target card.
2. **`go` only on an explicit human GO** for an already-presented card. Never synthesize `go` — Jeff/Wren says it, or it stays absent (DEC-048: GO=accept, the accepter is the authority `werk-accept` runs under).

Validate / commit / push / build / test / deploy / env-up / demo / merge / accept — all owned by the MCP.

## Step 1: Invoke `chorus_werk`

Run-to-demo (no go):
```
mcp__chorus-api__chorus_werk({ role: "<role>", card_id: <CARD_ID> })
```
Returns `{ ok, phase: "presented", go_command }` in minutes — the variant is up and presented; nothing is held across the human wait.

Resume-to-land (on the human GO only):
```
mcp__chorus-api__chorus_werk({ role: "<role>", card_id: <CARD_ID>, go: true, accepter: "jeff" })
```
Resumes past the stop: merge → ff-sync → deploy-prod → accept. Returns `{ phase: "landed" }`.

That's the entire skill. The MCP drives the pipeline deterministically via `act` + `werk.yml`.

## Step 2: Report

After the no-go run, report `presented` + the variant URL and that the demo prework (gates + peer gathers) must complete before a GO lands (the announce is the ready-gate). After a `go` run, report `#<card> landed` (deployed + accepted). Refusals come back typed from the verbs — pass the reason through, don't paper over it.

## Hard rules

- **Use `chorus_werk` MCP — never raw `act`, `git`, `werk-*` shell, or the individual leaf verbs from this skill.** `chorus_werk` is the runner contract; it sequences the verbs under one trace.
- **The skill's job is invocation, nothing else.** It does not commit, build, deploy, merge, or accept directly — `chorus_werk` (via `act`/`werk.yml`) owns every step. No overlap, no race.
- **Never pass `go: true` without the human's explicit go** on a presented card (DEC-048). No-go/more = do nothing; the werk is preserved for iteration.
- **MCP unreachable is the only escape hatch.** If `chorus-api` is down, escalate to ops; do not hand-run `act` or the leaf verbs.

## Pattern lineage

`/cw` follows the werk-verb skill pattern (ADR-032 verb-contract-v1, ADR-037 atomic-verb-execution): a thin user-invocable skin over a single typed MCP verb, with the substrate owning execution and typed-refusal pass-through. It is the orchestrator-level entry (`chorus_werk`) where `/pull`, `/commit`, `/push`, etc. are the leaf-verb entries. One concept, one implementation (`chorus:principle-no-competing-implementations`).
