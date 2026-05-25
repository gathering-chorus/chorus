# ADR-032: Verb Contract v1 — One Definition All Werk Verbs Reference

**Status:** Accepted
**Date:** 2026-05-24
**Author:** Silas
**Cards:** #3045 (werk-pull, the blueprint) · #3056 (werk-commit) · #3057 (werk-accept) · #3061 (werk-build) · #3062 (werk-deploy) · #3046 (demo — orchestrator) · #3064 (acp — orchestrator) · #3063 (werk-pull env-read follow-on)

*Six leaf verbs (pull/commit/build/deploy/verify/accept) + two orchestrators (demo, acp) that compose them by subprocess — all under this one contract.*
**Supersedes:** the inline contract AC lines copy-pasted onto each verb card.

## Context

The werk pipeline is a chain of single-purpose verbs — pull → commit → build → deploy → verify → accept — each a zero-dep Rust binary built to the werk-pull (#3045) blueprint. They share invariants: env vars, a single cross-verb trace, branch/card handling, the blueprint shape, gh-wiring, and the witness log.

The first instinct was to pin each invariant as an identical AC line copied into every verb card. **That copy-paste IS a drift vector.** In one hour on 2026-05-24, three drifts surfaced directly from it:

1. **Shared trace** — werk-pull mints its own `trace_id()` (ns+pid) and never reads `CHORUS_TRACE_ID`, so each verb gets a different trace and the cross-verb thread (#3056 AC#4) silently breaks.
2. **Branch/card** — there was no single rule for the no-werk-refuse guard, and its absence corrupted canonical that morning: `/acp` ran on a card that was carded + moved but never *pulled* (no werk), so it committed canonical's dirty tree as an "acp #unknown" commit and left local `main` ahead of origin, breaking team sync.
3. **Trace-carrier filename** — the live TS (`emit.ts resolveTraceId`) reads `/tmp/demo-trace-<card>.txt` while the verb ACs invented `/tmp/<card>-trace` — two files that would never join.

N identical copies = N places to drift. This is `chorus:principle-no-competing-implementations` violated in our own card ACs.

## Decision

**This ADR is the single definition of the verb contract.** Every werk verb card carries one line — *"adheres to verb contract v1 (ADR-032)"* — instead of copying the invariants. Verb cards keep only their verb-*specific* ACs. The contract:

1. **Zero-dep blueprint** (werk-pull #3045 `lib.rs`): thin main (exit 0 print / exit 1 eprintln); pure unit-tested helpers; typed `run()`/`run_in()` subprocess wrappers; `FlockGuard` + `lock()`; a testable core `verb(card, role, home, werk_base, …)` with ALL inputs explicit; `run_verb()` parses args/env; `R<T> = Result<T, String>`; all-or-nothing `rollback()` under the lock; best-effort `jsonl()` witness that never affects the op. **ZERO chorus-code deps** — std + libc(flock) + CLI subprocesses (git/gh/cargo/codesign/launchctl/build-signed.sh/chorus-bin-install) only; no chorus-log, no chorus-sdk. (werk-pull's `Cargo.toml [dependencies]` is empty — the defining axis, CI-checkable.) **Orchestrating verbs (demo) invoke the sibling verb binaries (werk-build / werk-deploy / werk-verify) as subprocesses** — still zero-dep (a subprocess, not a code dependency) — so an orchestrating verb's allowed-subprocess set is the union of git/gh/cards plus those binaries. A verb's subprocess list must cover what it actually shells out to.

2. **Env contract:** reads `CHORUS_HOME`, `CHORUS_WERK_BASE`, `DEPLOY_ROLE` (from `chorus-env-setup.sh`). build/deploy additionally resolve the install-target by slot: `WERK_<ROLE>_BIN` before demo (the role's session runs it), `CHORUS_BIN` (`~/.chorus/bin`) after acp (canonical). The two deploys are **not redundant**: demo→`WERK_<ROLE>_BIN` is a role-scoped prove (isolated to the role's session, watched live, other roles unaffected); acp→`CHORUS_BIN` is the shared land post-accept. Same one prod env — the difference is binary *scope* (role vs shared), not a second environment.

3. **Shared trace — one thread across the chain.** Resolve by precedence: `CHORUS_TRACE_ID` env → `/tmp/<card>-trace` file → mint-and-persist (write the file so downstream verbs inherit). The **file is the cross-process carrier** (verbs are separate processes; env is belt, file is suspenders). Matches `emit.ts resolveTraceId`. **One filename: `/tmp/<card>-trace`** — general, because the trace spans the whole card run, not just demo. `emit.ts`, `demo_preflight`, and #3063 adopt this one name. Result: one trace threads pull → commit → build → deploy → verify → accept across gh-status + jsonl.

4. **Branch + card handling.** Branch = `<role>/<card>` derived from `DEPLOY_ROLE` + card-id via one shared helper (`branch_name`). card-id is an **intent assertion** → refuse `card-mismatch` (#2868) if the branch-derived id differs. **Every verb REFUSES cleanly if the `<role>/<card>` werk/branch does not exist — and NEVER operates on canonical `main`.** (This guard is the structural fix for the acp-#unknown canonical corruption of 2026-05-24.) Card status: pull moves Next/Later → WIP; commit/build/deploy/verify require WIP and never transition; accept moves WIP → Done (the only Done path; emits `card.accepted`) and closes the branch + werk.

   **Accept is the exit-finalize, not an entry-gate and not a bracket.** The verb sequence is pull → commit → build → deploy → verify → **accept** (last). Accept validates its preconditions (card in WIP, gate + verify evidence present) *and* finalizes (merge `<role>/<card>` → main, WIP → Done emitting `card.accepted`, close branch + werk) as one atomic step at the exit. Entry validation belongs to pull (→ WIP); accept does not re-gate at entry. The monolithic `acp` name (accept-commit-push, which *read* accept-first) is retired by the decomposition — the verb "accept" means only the merge + done + close finalize, sequenced last. Build/deploy/verify run on the werk-branch build **before** the merge, so prod is proven before the source lands on main; if verify fails, deploy rolls back and the card **stays WIP** (no merge, no accept). This resolves the #3057 entry-vs-exit ambiguity: it is exit only. **The werk-branch is rebased to current origin/main *before* build** (clean/ff; rebase conflict → refuse) — so by build-invariance the accept ff-merge yields the exact cdhash that was built, deployed, and verified. The deployed binary *is* the merged-main result; no post-merge rebuild, and werk-stale-vs-main cannot slip through.

5. **gh-wiring (#3056 AC#4):** per-verb commit-status `chorus/<verb>/<card>` carrying the verb's relevant id (cdhash for build/deploy, sha for commit); **carry prior `chorus/*/<card>` statuses forward** onto the new SHA; one shared trace across gh + jsonl. **commit-status, NOT gh DEPLOYMENTS** — we have one environment and it is production, so DEPLOYMENTS' multi-env modeling is wasted and commit-status keeps uniformity + the carry-forward mechanic.

6. **Witness:** best-effort `jsonl()` lines per step to a LOCAL file `ops/logs/werk-<verb>.jsonl`; **NEVER chorus-log** (would break zero-dep); Borg/Loki ingest downstream.

## gh state model

A card's process is visible on GitHub through **two distinct surfaces** — keep them separate:

- **commit-status = PROCESS (per verb).** Each verb owns a status context `chorus/<verb>/<card>` (pull · commit · build · deploy · verify · accept). State: none → pending → success / failure / error. Prior `chorus/*/<card>` statuses carry forward onto the new SHA (§5) so the whole chain is readable on one commit.
- **PR / branch / main = LIFECYCLE.** Branch `<role>/<card>` exists from commit (pushed) onward; the PR is opened **and** merged at accept (the exit-finalize, §4); the branch is deleted at accept. `main` only ever receives merged, verified work.

Three sub-decisions, pinned (the SVG's defaults, confirmed):

1. **Each verb owns its own status** — sets `pending` on start, a terminal (success/failure/error) on finish. No orchestrator pre-seeding: a verb run standalone reports correctly, and a verb that never runs (because an upstream failed) stays `none` — which reads as "didn't get here," not a stuck `pending`. This is the leaf-verb-autonomy of §1.
2. **The PR opens + merges at accept** — not a draft at push. The per-verb commit-statuses already attach to the branch HEAD commit during the work, so the in-flight process is visible on the branch *without* a draft PR — no draft-PR lifecycle to manage, no lingering draft on abandon. The PR is the merge mechanism at the finalize, not a process surface.
3. **Distinguish `failure` from `error`** — `failure` = the verb ran and the work is wrong (tests red, verify cdhash-mismatch) → fix the work; `error` = the verb couldn't run (lock timeout, subprocess crash, infra down) → retry / fix infra. Maps to GitHub's native commit-status states; the distinction is what makes the status actionable (#3050-style alerting needs it).

## Gate placement (ADR-026's three layers in the verb model)

ADR-026's three quality layers keep their roles; the verb model relocates *where* layer 1 runs and *scopes* it to the card.

1. **Pre-commit redistributes into the verbs, scoped to the card's CHANGED surface — never a whole-tree monolithic git-commit hook.** That monolith is exactly what bit #3053 (whole-tree `--fix` firing on incidentally-staged files; multi-product collisions; the 30s-vs-90s timeout). Homes:
   - **build:** `tsc` + `jest` + scoped `sonarjs` (cog/dupe) + scoped `lint-ratchet` — compile/test/quality the card's product, on its diff only.
   - **commit:** secrets-scan + **commit-only-the-card's-files** (the no-sprawl guard — never `git add -A`) + principle-direct-edit guard + werk-version-bump + catalog-oversize.
   - **verify:** running==built + smoke.
   - **Scope rule:** every check runs against the card's changed surface, not the whole tree. This is the #3053 fix.

2. **The 5 role gates split mechanical vs judgment.**
   - **Mechanical cores fold into verbs:** product's AC+experience-present → **pull's entry validation** (#3045 already does this); code + quality (tests green, no `console.*`, scoped lint/sonar) → **build/verify**; arch's namespace/name conventions (ADR-031 name-test) → **build + CI**; ops's health/rollback/disk → **deploy/verify**.
   - **Judgment stays a human role-review** — "is this worth doing" (product), "does this fit the system" (arch) — which no verb can mechanize. The **demo verb (#3046) orchestrates** these review requests (the feedback nudges); it does not automate the judgment. Net: the verbs absorb the mechanical re-checks so the reviews focus on judgment, not re-running tests.

3. **CI (`quality.yml`) is unchanged** — branch-protected, authoritative on `main` ("does main build cleanly from scratch"). Verbs are local fast-feedback (skippable, as pre-commit was); CI is the from-scratch authority that `--no-verify` cannot bypass (ADR-026). Verbs don't replace it.

Resolves #3056's open question (werk-commit inherits the commit-time checks, scoped) and connects to #3046's gate fold.

## Consequences

- The inline contract AC lines on #3056, #3057, #3061, #3062 collapse to one reference line to this ADR. One source of truth; nothing to drift.
- #3063 (werk-pull env-read), `emit.ts`, and `demo_preflight` adopt the single carrier filename `/tmp/<card>-trace`, so cards-CLI emits and the verb chain join into one full-card-run thread.
- The no-werk-refuse guard (§4) structurally closes the acp-on-un-pulled-card corruption class.
- New verbs inherit the contract by reference; changing an invariant is one edit here, not N across cards.
- **Enforcement (the teeth, per ADR-031):** a CI test asserting each werk verb adheres — empty `[dependencies]` (zero-dep), trace-precedence resolution, and the no-werk-refuse guard. A contract is only real if something checks it.
