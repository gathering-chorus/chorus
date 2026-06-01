# ADR-036: One Build/Deploy Execution Path — Collapse the Parallel Paths

**Status:** Accepted (direction); implementation staged via slices
**Date:** 2026-06-01
**Author:** Silas
**Cards:** #3170 (this — the parent target) · #3169 (first slice: node_modules-ensure → build preamble) · #3168 (cascade-gate + absolute build-signed.sh/tsc resolution — the symptoms) · #3147 (the chorus-api deploy whose half-dead-on-prod state exposed the deploy-path split)

*Coherence in an AI codebase = ONE line of code execution, not N well-named ones. This ADR names the single build/deploy path everything funnels through, and the parallel paths that collapse into it.*

**Relates to:** `chorus:principle-no-competing-implementations` (this is its sharper, AI-specific form) · ADR-032 (verb contract — the verbs are the *steps* of this one path) · #2913 (werk node_modules sharing caveat) · #2734 (signed-binary deploy / TCC cdhash).

## Context

On 2026-06-01 a single false build refusal ("chorus-sdk breaking change") opened into a multi-hour thicket. Tracing it to the bottom, every layer was the **same shape: parallel execution paths that had drifted out of sync with each other.**

- **Two deploy systems.** `chorus-deploy` (bash; builds canonical, installs to `~/.chorus/bin`, kickstarts the `com.chorus.*` daemons) and `werk-deploy` (Rust verb; builds the werk, installs to the per-role slot, no kickstart). Same job — get built code running — two implementations. The night's `merged≠live` confusion lived in the gap between them.
- **A cascade path separate from the build.** `build_shared_lib` (#3126) builds chorus-sdk then runs `npm install` + rebuild on every consumer — a second build path beside the normal per-unit build, with its own env-fragility (it died on hoisted `tsc`, and false-refused as a "breaking change").
- **node_modules-ensure as a one-shot at pull, nothing at build.** `chorus-werk add` symlinks node_modules once at creation and never again; a werk that drifts to a partial tree (an `npm install` replacing the symlink — the #2913 caveat realized) can never self-heal, and TS builds die on `@types`.
- **env-up as its own orchestration** (build-variant + launchctl-bootstrap + smoke), parallel to the normal build/deploy.
- Even the **word "bootstrap"** named two unrelated things (node_modules-ensure vs `launchctl bootstrap`), which cost real debugging time.

The point named by Jeff that reframes all of it: **in an AI codebase, coherence is not "several clearly-named concerns" — it is ONE execution path.** The model keeps spawning parallel paths, each rationalized as "a different concern," and they drift. Renaming them keeps N paths and N chances to drift; collapsing them to one leaves nothing to drift against. "These are different concerns, keep them separate" is the *tell* that multiplicity is being defended.

## Decision

**There is ONE build/deploy execution path:**

```
ensure-deps → build (units in dependency order) → install → start
```

Every entry point — `/pull`, `werk-build`, `env-up`, `/demo`, `/acp`, and the daemon deploy — routes through this single path. **No step has a second implementation.** Differences that look like they need a separate path (a daemon vs a CLI verb; a werk-variant slot vs canonical; a TS service vs a Rust crate) are expressed as **parameters of the one path**, not forks of it.

The werk verbs of ADR-032 are the *steps* of this path; this ADR is the statement that the steps compose into exactly one path and that the currently-parallel implementations collapse into it.

### What collapses into the one path

1. **The two deploy systems → one.** `chorus-deploy` (daemons) and `werk-deploy` (verbs) unify; daemon-vs-CLI-verb and werk-slot-vs-canonical become **parameters** (`target`, `kickstart?`), not separate scripts. This is the largest collapse and the highest-value.
2. **The cascade dissolves into "build in dependency order."** If the one path builds units topologically against the werk's *just-built* shared-lib dist, then "test consumers against the new sdk" is simply the build — no separate `npm install` cascade exists to conflict with anything.
3. **node_modules-ensure becomes the path's first step** — an idempotent preamble (symlink each package's node_modules to canonical's complete tree, replacing stale partials), run every build. The pull-time one-shot is retired. *(Slice #3169.)*
4. **env-up becomes the one path targeting the werk-variant slot**, not a parallel orchestration.
5. **One thing is named "bootstrap"** (node_modules-ensure); the launchd one is renamed (`service-load`).

## Consequences

A role builds/deploys any card through one path that behaves identically whether the unit is a Rust verb, a TS service, the daemons, or a werk-variant. The failure classes that ate 2026-06-01 — `merged≠live`, partial-node_modules TS failures, false cascade refusals, bare-name resolution (`build-signed.sh`, `cards`) papered over by one env but not another — become **structurally impossible**, because there is no second path to drift against. New language / service type = a new parameter value (a detection rule), not a new path (ADR-032 §1 #3092, generalized).

Cost: a single path is a single point — it must be correct, and the daemon-vs-verb unification is genuinely hard (see Open Items). The payoff is that there is exactly one thing to get right and keep right, instead of N things that silently disagree.

## Open items (VERIFY — do not hand-wave; each must be closed before claiming the collapse)

- **A. Deploy-systems unification.** How do daemon deploys (TCC/cdhash binding #2734, `launchctl kickstart`, install to `~/.chorus/bin`) and CLI-verb deploys (role slot, no kickstart) become one parameterized path without losing the cdhash-stability that makes TCC grants survive rebuilds? This is the hard one. Verify the unified path preserves #2734's identical-source→identical-cdhash property.
- **B. Cascade-against-werk-sdk.** Confirm that building consumers in dependency order actually compiles them against the **werk's** modified shared-lib dist (not canonical's old one) before declaring the cascade dissolved. If dep-ordered build can't see the werk's new dist, the cascade has a residual job and the collapse is incomplete.
- **C. The #2913 tradeoff.** Symlink-to-canonical node_modules cements "the werk shares canonical's deps" — a card needing a *divergent* dependency version can't have it. The alternative (real per-werk `npm ci`) is divergence-capable but pays the install tax and can itself leave partials. Default: keep the fast symlink model; item B may force a narrow per-werk exception for the shared-lib-change case.

## Implementation: slices through the one path, not one PR

This is a target, realized by slices that each ship *through* the one path as it is built — never bundled into a single change.

1. **#3169** — node_modules-ensure as the build preamble (the way in; proven by hand on 2026-06-01).
2. Deploy-systems unification (Open Item A) — its own card.
3. Cascade-dissolve (Open Item B) — its own card.
4. env-up fold + the `bootstrap`→`service-load` rename.

#3168 (cascade-gate + absolute resolution) and the manual werk repairs of 2026-06-01 are interim correctness fixes *within* the current parallel paths; this ADR is the direction that retires the parallelism they were patching.
