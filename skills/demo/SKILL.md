---
name: demo
description: The proving ceremony — werk-demo runs the gates itself (headless claude -p) + present/feedback/verdict; the demoer presents (#3443).
user-invocable: true
---

# /demo — the proving ceremony

Since #3116, `/demo` is the **proving ceremony only**: present the already-running werk instance to a prover, gather quality + value feedback, record one verdict. It does **not** build or deploy — the act (build → deploy → env-up) is the prior steps in the one pipeline (`werk.yml`, run via act — #3236 retired `werk-mcp.sh` into it). Demo points at what is already up.

Two layers split the work (#3443):
- **`werk-demo` (the binary)** RUNS the 5 gates itself — one headless `claude -p` per gate, fed the gate's `SKILL.md` + the card AC + the branch diff — and records each `demo.gate.result`. It also handles the deterministic mechanics: present the variant, fire the verbatim feedback gather, hold the review window, emit `demo.verdict`. `claude -p` is a subprocess like git or cargo; the old "an LLM gate can't run in the zero-dep binary" excuse is retired.
- **The demoer (this agent)** presents the finished, already-gated state to Jeff and routes each recorded gate result to its owning role for review. It no longer spawns gate subagents — the binary owns gate execution.

## Invocation

```
/demo <card-id>
```

## The JX (Jeff, 2026-06-10): prework is sealed BEFORE the conversation

A demo is a **conversation that opens only when everything is already done.** Jeff is
never dragged into a demo that isn't ready to run. Two phases, never interleaved:

1. **Prework — silent, no Jeff, runs to completion:** gates + (the prior verbs already
   did build → test → deploy → env-up). Gates run HERE, before any announce.
2. **The announce IS the ready-gate.** It fires *only* when every gate is recorded AND
   the variant is up. Its existence is the guarantee that walking in won't waste Jeff's
   attention. If prework fails, there is **no announce** — Jeff sees `NOT READY: <what
   failed>`, never a demo invitation.
3. **The conversation — Jeff + demoer:** present the finished state, walk it, go/no.
   **Nothing computes during the conversation** — that's why it can never hang or lock
   Jeff out; all the slow work already happened in prework, before he was pulled in.

The old bug (what this replaces): gates ran *in the conversation turn* — 5 subagents
dripped serially for minutes while Jeff watched a spinner, and the pipeline announced
"presented" before gates had run at all. Gates are prework now, run BY THE BINARY. The
announce comes last.

## Step 1 — PREWORK: werk-demo runs the gates itself (silent, before any announce)

**You no longer spawn gate subagents.** `werk-demo` runs the 5 gates itself when you
invoke it (#3443): for each gate with no result yet for this round, it spawns one
headless `claude -p` — fed the gate's `skills/gate-<g>/SKILL.md` as the system prompt and
the card AC + branch diff (`origin/main...HEAD`) as context — and records each
`demo.gate.result`. A garbled or errored gate records `result=error` (VISIBLE on the
cockpit), never a silent pass.

| Gate | Owning role (reviewer) |
|---|---|
| product  | wren |
| code     | kade |
| quality  | kade |
| arch     | silas |
| ops      | silas |

The owning role still **reviews** its gate's recorded result after the fact (async,
non-blocking — a finished artifact, not work to do); it just no longer has to *run* it.

**Where gates self-run:** anywhere the `claude` CLI resolves — Jeff's machine and local
`act`. There the pipeline self-gates end-to-end, headless, with no demoer step. Only on
hosted CI (no `claude` binary, no auth) does the binary degrade: it skips gate-run AND
enforcement rather than break the build (#3284 lesson). The "an LLM gate can't run in the
zero-dep binary or headless `act`" claim is **retired** — `claude -p` is a subprocess.

**Cold-eyes review (#3193) — still a demoer step.** This is a *review-floor*, separate
from the 5 gates: spawn ONE Explore-type review subagent with FRESH context — give it
ONLY the card's AC and the branch diff (`git diff $(git merge-base origin/main HEAD)..HEAD`
in the werk), no build conversation, so it cannot rationalize. Prompt it ADVERSARIALLY
(find problems, default-skeptical), hunting: AC items the diff doesn't actually cover (the
#1 lie), aspirational-as-built, bugs, missing tests, scope creep. Record via `werk-review
verdict <card-id> pass|fail <findings>` — the binary REJECTS a fail with no specific
findings and any verdict recorded before the pipeline's review-floor ran (anti-ceremony,
#3193). Advisory today: a fail informs Jeff's go, it does not block the present.

**Ready-gate (the announce contract):** only after all 5 gates are recorded AND the
variant is reachable does the binary proceed to present → announce. If any gate FAILS or
the variant is down, **the demo STOPS — no present, no announce.** Jeff sees `NOT READY:
<gate or variant that failed>`, never an invitation to a demo that can't run.

The gate results are the quality gather. They are **not** the proving verdict — a green gate is not a verdict, and a peer's review is never the verdict (proving is Jeff or a machine, never peer-blessing). Recording a gate attests it RAN; the verdict is still the prover's.

**Gates are REQUIRED to present (#3284 — invariant execution).** `werk-demo` REFUSES to present unless all 5 gates have recorded a `demo.gate.result` for the card (it emits `demo.refused reason=gates-missing` and exits non-zero). With #3443 the binary *runs* the gates before this check, so the refusal now only triggers where gates genuinely couldn't run (no claude) — a demo that reaches present with no gates fails LOUD instead of silently showing "(none run)".

This is NOT a contradiction of #3263 ("the machine shows, it never vetoes your go"). The refusal blocks *presenting un-gated*; it never blocks Jeff's GO — gates inform the decision, they don't decide it. Jeff's go stays sovereign.

The flow (#3443, replacing the #3284 demoer-runs-gates flow):
- The pipeline (`chorus_werk` / `werk.yml`) builds → deploys → env-ups the variant, **then invokes `werk-demo`**, which runs the gates itself against the live variant and presents. There is no separate "standby, wait for a demoer to run gates" step.
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

**GO is one silent ceremony (#3327).** Jeff's GO at the demo IS the accept (GO=accept, #3311) — it's recorded once, at the demo. `werk-accept` then runs *inside* the same `go:true` pipeline (merge → deploy-prod → finalize); it records the go internally and finalizes **silently**. There is no second go to give and no copy-paste step — the only human-facing output is `#N finalized`. If you ever see `werk-accept` re-announce "go signaled… act continues to merge," that's the WIP-limbo recovery path (a transport drop left the card un-finalized), not the normal flow.

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
