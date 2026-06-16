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

## Step 1: Invoke `chorus_werk` — launch, then POLL (#3458)

`chorus_werk` is now ASYNC: it spawns the pipeline DETACHED and returns immediately so the MCP call never holds open across the multi-minute run (that hold was the transport-drop root). It returns `{ phase: "running", launched: true, run_id }` in seconds. You then POLL by re-invoking the SAME call — it attaches to the live run (never double-acts) and reports the current phase, advancing to `presented` / `landed` / `failed` when the detached `act` finishes.

Run-to-demo (no go):
```
mcp__chorus-api__chorus_werk({ role: "<role>", card_id: <CARD_ID> })
```
First call → `{ phase: "running", launched: true }`. Re-invoke the same call to poll: `{ phase: "running", attached: true }` while in flight, then `{ phase: "presented" }` (variant up) or `{ phase: "failed", failureReason }`. Poll on a sane cadence (the run is minutes); each poll is cheap (it reads run-state, doesn't re-run).

Resume-to-land (on the human GO only):
```
mcp__chorus-api__chorus_werk({ role: "<role>", card_id: <CARD_ID>, go: true, accepter: "jeff" })
```
Same shape: launches the land DETACHED → `{ phase: "running" }`; poll the same call until `{ phase: "landed" }` (merged → ff-synced → deployed → accepted) or `{ phase: "failed", failureReason }`.

That's the entire skill. The MCP drives the pipeline deterministically via `act` + `werk.yml`; you launch and poll, nothing is held.

## Step 2: Report

Report what the CURRENT phase is, not a hoped-for one. On `running`, say it's in flight and you're polling. On `presented`, report the variant + that demo prework (gates + peer gathers) must complete before a GO lands (the announce is the ready-gate). On `landed`, report `#<card> landed` (deployed + accepted). On `failed`, surface the typed `failureReason` verbatim — don't paper over it. A re-invoke after a transport drop is a non-event: it attaches and reports the true phase (no double-act, no lost call).

## Hard rules

- **Use `chorus_werk` MCP — never raw `act`, `git`, `werk-*` shell, or the individual leaf verbs from this skill.** `chorus_werk` is the runner contract; it sequences the verbs under one trace.
- **The skill's job is invocation, nothing else.** It does not commit, build, deploy, merge, or accept directly — `chorus_werk` (via `act`/`werk.yml`) owns every step. No overlap, no race.
- **Never pass `go: true` without the human's explicit go** on a presented card (DEC-048). No-go/more = do nothing; the werk is preserved for iteration.
- **MCP unreachable is the only escape hatch.** If `chorus-api` is down, escalate to ops; do not hand-run `act` or the leaf verbs.

## Pattern lineage

`/cw` follows the werk-verb skill pattern (ADR-032 verb-contract-v1, ADR-037 atomic-verb-execution): a thin user-invocable skin over a single typed MCP verb, with the substrate owning execution and typed-refusal pass-through. It is the orchestrator-level entry (`chorus_werk`) where `/pull`, `/commit`, `/push`, etc. are the leaf-verb entries. One concept, one implementation (`chorus:principle-no-competing-implementations`).
