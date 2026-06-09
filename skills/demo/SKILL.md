---
name: demo
description: The proving ceremony — demoer runs gates as subagents + invokes werk-demo for present/feedback/verdict (#3116).
user-invocable: true
---

# /demo — the proving ceremony

Since #3116, `/demo` is the **proving ceremony only**: present the already-running werk instance to a prover, gather quality + value feedback, record one verdict. It does **not** build or deploy — the act (build → deploy → env-up) is the prior steps in the one pipeline (`werk.yml`, run via act — #3236 retired `werk-mcp.sh` into it). Demo points at what is already up.

Two layers split the work:
- **The demoer (this agent)** initiates the 5 gates as **subagents** and routes each result to its owning role for review. An LLM gate-review can't run inside the zero-dep `werk-demo` binary, so it lives here.
- **`werk-demo` (the binary)** handles the deterministic mechanics: present the variant, fire the verbatim feedback gather, hold the review window, emit `demo.verdict`.

## Invocation

```
/demo <card-id>
```

## Step 1 — Run the gates as subagents (demoer-initiated)

Spawn one subagent per gate. **Do not nudge a role to go run its gate** — you run it; the role *reviews the result*. Map gate → owning role:

| Gate | Owning role (reviewer) |
|---|---|
| product  | wren |
| code     | kade |
| quality  | kade |
| arch     | silas |
| ops      | silas |

For each gate: spawn a subagent that reads the card + the diff + the running variant and produces a `{gate, result: pass/fail, findings}` verdict. Then do **two** things with the result:
1. **Record it** — run `werk-demo gate <card-id> <gate> <pass|fail>` (the gate witness). The recorded results surface on the decision surface so Jeff sees what was checked before he decides.
2. **Route it for review** — a result-carrying review-nudge to the owning role (async, non-blocking): it carries a finished artifact to review, *not* work to do. The role reviews the output; it does not re-run the gate.

The gate results are the quality gather. They are **not** the proving verdict — a green gate is not a verdict, and a peer's review is never the verdict (proving is Jeff or a machine, never peer-blessing). Recording a gate attests it RAN; the verdict is still the prover's.

**Gates are REQUIRED to present (#3284 — invariant execution).** `werk-demo` REFUSES to present unless all 5 gates have recorded a `demo.gate.result` for the card (it emits `demo.refused reason=gates-missing` and exits non-zero). This is the forcing function that makes gate execution invariant: a demo that reaches present with no gates fails LOUD instead of silently showing "(none run)".

This is NOT a contradiction of #3263 ("the machine shows, it never vetoes your go"). The refusal blocks *presenting un-gated*; it never blocks Jeff's GO — gates inform the decision, they don't decide it. Jeff's go stays sovereign.

Because an LLM gate-review can't run in the zero-dep binary (or headless `act`), the gates are run by the **demoer (this agent, an LLM)** — and that's why the demo is demoer-driven, not pipeline-headless. The flow (#3284):
- The pipeline (`chorus_werk`) builds → deploys → env-ups the variant. It does **not** run a headless demo (that would refuse — no LLM ran the gates).
- The **demoer** then runs the 5 gate subagents *against the live variant* (it's up after env-up), records each via `werk-demo gate <card> <gate> pass|fail`, **then** invokes `werk-demo` to present.
- `werk-demo` present shows the cockpit (verb checklist) + each gate's verdict. Jeff says **go** → it runs to completion (land). "Test in demo, accept in demo."

Still true: the machine-checkable floor (work **tested** via `werk-demo test-result`, variant **up and reachable**) feeds the decision surface; an *uninformed* go is what the surface guards against.

## Step 2 — Invoke the binary (present → feedback → window → verdict)

```
werk-demo <card-id>
```

The harness invokes it with `DEPLOY_ROLE`, `CHORUS_HOME`, `CHORUS_WERK_BASE`, `CHORUS_TRACE_ID` in env. The binary:
- **Presents** the running werk variant (announces the variant URL/ports to Bridge; emits `card.demo.started`).
- **Fires the product/domain feedback gather** to peers — VERBATIM (the 4 questions; pinned by `feedback_message_is_verbatim`). Re-nudges unacked peers within the ACK window.
- **Holds the review window** so Jeff can react and peers can answer (the binary sleeps; the agent must not jump into /acp prose).
- **Records one `demo.verdict`** `{card, pass|fail, prover, gates, feedback}`. Prover defaults to **Jeff** (his eyes on the variant during the window); `CHORUS_DEMO_PROVER` selects the machine prover (run the card's Experience test against the deployed instance) when the card has a machine-checkable assertion.

## What gates acceptance

`werk-accept` (step 8 of the flat sequence) finalizes a card **only** if a `demo.verdict=pass` exists for it on record. There is no `demo:preflight-pass` comment chain anymore and no 4-event `demo.*.completed` chain — one verdict, read from the werk-demo witness. DEC-048 (a builder can't self-accept a code card; Jeff is the human authority) stays enforced in `werk-accept`.

## What this replaces (#3116)

- The in-binary **gate-chain + go-run-your-gate nudge relay** → gates are now subagents the demoer initiates; results route to roles for review.
- The in-binary **act** (`werk-build` / `werk-deploy` / env-up) → the prior verbs do that; demo never builds or deploys.
- **smoke / stakes-brief / signal-mechanics** → smoke folds into the machine prover; the verdict is the output.
- The **#2864 cascade** (`demo-service-design.html`) and its 5 cascading hooks → retired. Design of record: `designing/docs/demo-service-design.html` (the proving-ceremony model).
- `accept_gate`'s `demo:preflight-pass` + `demo.show.completed` evidence → `demo.verdict=pass`.

## Rules still enforced

- Validate: card exists, status WIP/Now, has AC; AC pre-flight all checked.
- The product/domain feedback gather fires to peers, verbatim, with re-nudge on silence.
- The review window holds before the verdict is recorded (no premature /acp prose).
- DEC-048 self-accept block — enforced at `werk-accept`.
