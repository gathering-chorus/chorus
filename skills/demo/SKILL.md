---
name: demo
description: Proving gate — thin wrapper around `werk-demo` (#3046). All logic in the binary.
user-invocable: true
---

# /demo — thin wrapper for `werk-demo`

Since #3046, `/demo` carries no logic of its own. The proving gate — validate → AC-preflight → gate-chain → smoke → signal → build → deploy → verify, plus the DEC-048 non-builder-confirm chain via the `demo:preflight-pass` evidence comment — lives in the one `werk-demo` Rust binary (`platform/services/werk-demo/`), zero-dep, mirroring the verb-contract blueprint (werk-pull #3045).

## Invocation

```
/demo <card-id>
```

The harness invokes `werk-demo <card-id>` with `DEPLOY_ROLE`, `CHORUS_HOME`, `CHORUS_WERK_BASE`, and (if present) `CHORUS_TRACE_ID` in the env. The binary handles everything: posts the `demo:preflight-pass` evidence comment (the single gate-evidence #2910), writes `/tmp/demo-trace-<card>.txt` (#2897), invokes `werk-build` (#3061) + `werk-deploy` (#3062), emits the spine events `card.demo.started` / `demo.show.completed`, writes its jsonl witness to `ops/logs/werk-demo.jsonl`. Loki ingests; gh records `chorus/demo/<card>=success` with the trace.

## What this replaces

- The prose orchestration of steps 1–7 (collapsed into the binary).
- `skills/demo/gates/{preflight,done-gate,provenance,show-gate}.sh` — collapsed.
- `platform/services/chorus-hooks/src/hooks/{demo_gate,demo_preflight,demo_provenance,demo_show}.rs` — were dispatch-only to the gates above; replaced by direct invocation of `werk-demo`.
- The implicit "evidence token" pattern (replaced by the card-comment `demo:preflight-pass` + gh status, no separate token).

## Rules still enforced (none dropped)

- Validate: card exists, status WIP/Now, has AC.
- AC pre-flight: all AC checked → posts `demo:preflight-pass` card comment + writes trace file.
- Gate chain: all five role gates (product/code/quality/arch/ops) commented on the card.
- Smoke check: `smoke-check.sh --all` (type:swat exempt, non-code skipped).
- Signal: `cards demo` + `card.demo.started` spine event + Bridge post + feedback nudges to the other roles (the 3 questions, ACK 10m).
- Act: `werk-build` → `werk-deploy` (which owns running==built verify + all-or-nothing rollback).
- Show.completed: emits `demo.show.completed` so `accept_gate` admits `/acp`.
- DEC-048 non-builder confirm: enforced *downstream* at `/acp` via the `demo:preflight-pass` evidence chain + `werk-accept`'s `can_accept` truth table (wren cannot self-accept her own card; jeff is exempt as human authority).

## Faithful-port deferral (named, not dropped)

`show-gate.sh` checked the spine for `jeff.input.delivered` within the demo window before emitting `demo.show.completed`. `werk-demo` currently emits unconditionally on successful deploy (more permissive in the human-presence axis). Closing the window-check is a follow-on once the spine-query path is wired into the binary.
