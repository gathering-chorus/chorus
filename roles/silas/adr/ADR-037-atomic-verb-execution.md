# ADR-037: Atomic verb execution (`--atomic`) + the human-authorizes-prod gate

**Date:** 2026-06-08
**Status:** Accepted
**Deciders:** Jeff (decision), Silas (architect/author)
**Builds on:** ADR-032 (Verb Contract v1 — the seven werk verbs), DEC-048 (human is the prod-acceptance authority)
**Related work:** #3285 (retire `chorus_*` build/deploy into the Rust verbs; one artifact, promote-not-rebuild)
**Referenced by service designs:** build-and-deploy (Silas), version-control + CI/CD (Kade)

## Context

The seven werk verbs — `werk-pull`, `werk-commit`, `werk-push`, `werk-build`, `werk-deploy`, `werk-merge`, `werk-accept` — are tightly coupled to the werk flow. Each depends on up/downstream state left by the prior step: `werk-commit` needs the werk `werk-pull` created, `werk-build` needs the commit, `werk-deploy` needs the artifact `werk-build` left in the expected place. They are chained through implicit shared state (the werk dir, the branch, the artifact path), not composable units.

Consequence: the verbs are **almost impossible to run outside the flow.** When you need just one step — which is every recovery — you cannot run it through the verb, so you hand-run raw commands *around* the verbs. That is the direct source of the merged≠live hacks and the partial-deploy incidents (a full session of them, 2026-06-06/07): the deploy verb couldn't "just redeploy this," so the daemon got hand-bounced into a broken state.

## Decision

1. **Every werk verb gains an `--atomic` mode** that runs the verb standalone, outside the flow, taking its inputs **explicitly** rather than deriving them from a card + werk. `--atomic` is valuable on all seven — real uses: `pull --atomic` (just produce a worktree for a branch), `commit --atomic` (commit the werk *without* the rebase-onto-main step), `push --atomic` (push a role branch standalone), `merge --atomic` (just merge a PR — the hand-recovery), `build --atomic` (just build a crate), `deploy --atomic` (just deploy one thing).

2. **Two independent axes — do not conflate them:**
   - **`--atomic` (standalone run):** on **all seven** verbs.
   - **Explicit human approval:** only on verbs that mutate prod/main irreversibly — **`werk-deploy`** (changes prod running state) and **`werk-merge`** (lands code to main). `pull` / `commit` / `push` / `build` run `--atomic` **free** — they are local and reversible. Every approved atomic run emits a spine event recording `{who approved, what, when}`.

3. **The principle: the human authorizes prod (DEC-048), reachable two ways — one gate, two doors.** The demo→go inside the flow, OR an explicit approval on `--atomic` outside it. `--atomic` is the operator/recovery door; it is *not* a competing path or a free bypass. The danger it must not become — a quiet, un-demo'd ship — is closed on both halves: explicit approval kills "unauthorized," the spine event kills "invisible." What remains is "un-demo'd but approved-and-recorded," which is the legitimate recovery case.

4. **`--atomic` rides the #3285 factoring; it is not additive work.** Retiring `chorus_*` already requires factoring `werk-build`/`werk-deploy` into a pure core (build *this* / deploy *this*) plus a thin flow-adapter that fills the inputs from card+werk. `--atomic` is simply *exposing that pure core directly* — one more entry point on the same function, not a parallel implementation.

5. **`--atomic` MUST reuse the pure core — never a parallel reimplementation (Kade, 2026-06-08).** Both doors — the flow-adapter and `--atomic` — call the *same* pure-core function. If they share the core they cannot diverge; if `--atomic` forks its own logic, we have built a 7th competing path and recreated the exact sprawl this ADR cures. This is `chorus:principle-no-competing-implementations` / coherence-is-one-execution-path applied to the verb seam: **one implementation, two entry points.** A verb whose `--atomic` path and flow path don't bottom out in the same function fails this ADR.

6. **The roster is seven today — not closed forever (Jeff, 2026-06-08).** The current set is `werk-pull` · `werk-commit` · `werk-push` · `werk-build` · `werk-deploy` · `werk-merge` · `werk-accept` (with `werk-demo` as the prove/present step). Jeff has named two future first-class verbs — **`werk-code`** (the coding phase, filling the `pull → [write code] → commit` hole) and **`werk-review`** (#3193, cold-eyes review gating demo) — **parked, not yet specified.** When they land they inherit this ADR's rules: an `--atomic` mode, approval-gated only if they mutate prod/main. Read "seven" as the current roster, not a permanent cap.

## Consequences

- **Design whole, build incrementally.** The `--atomic` model is designed across all three service designs at once (it is one rule, cheaply stated). It is *built* one verb at a time, safest-first: `build --atomic` (no approval, harmless) → `deploy --atomic` + approval (the recovery win, gated) → `merge`/`pull`/`commit --atomic` as the need shows. No big-bang change to the pipeline.
- **Recovery becomes first-class through the verb.** "Just rebuild/redeploy this one thing" stops being a hand-command around the flow, which closes the hack class that produces merged≠live and partial-deploy incidents.
- **Risk:** `--atomic` could be used to ship un-demo'd changes. Mitigated by approval-only-on-deploy/merge + the audit spine event; build/pull/commit carry no prod risk so they need no gate.
- **The gate sits exactly where risk is and nowhere it isn't** — the same DEC-048 principle, not a new one.
