# Commits Service Design

**Kade, 2026-04-29. Card #2586.**

## Problem

The commits domain — every git mechanic between "I have a change" and "main reflects it" — emerged as a stack of tactical fixes rather than a designed surface. Today (2026-04-29) surfaced four classes of incident in a single session:

1. **Cross-branch contamination ×3** — wren and kade share `/chorus`'s working tree; one role's checkout is the other role's working state. Wren's #2575 fix landed on kade's #2558 branch; kade's #2558 narrow-refinement started on wren's branch. Substrate: shared `.git/HEAD`. (#2582 ships per-role worktrees as the structural fix.)
2. **Required-checks asymmetry** (#2498 side-finding) — branch-protection rules and Repository Rulesets list different sets of required jobs. Merges that satisfy one bypass the other. DEC-2525 amendment names both systems as required surfaces.
3. **git-queue branch-blindness** (#2580) — git-queue.sh commits whatever branch the working tree is on; if the role's session left a peer's branch checked out, the commit lands on the wrong branch with no warning. P1 defense-in-depth for the worktree convention.
4. **Worktree grain** unresolved — silas adopted per-card worktree (chorus-2526), kade adopted per-role (chorus-kade); wren is mid-decision (#2585 spike). No design says which is the convention vs the exception.

Each was handled tactically. None has a unified service-design surface, so future commits-related cards re-litigate the same questions.

The pattern matches nudge-service-design / independence-service-design / gate-set-service-design: a parent design lands first, tactical cards align under it. Kade owns commits per the 2026-04-26 cross-cutting allocation (Code · Tests · Commits · Pipelines · Heralds).

## As-Is (verified 2026-04-29)

```mermaid
flowchart TB
  subgraph WT["Working trees"]
    SHR["/chorus<br/>(shared — kade + wren)"]
    K["/chorus-kade<br/>(kade only — adopted today)"]
    S["/chorus-2526<br/>(silas — per-card)"]
  end

  subgraph CMT["Commit path"]
    Q["git-queue.sh commit<br/>(serialized via flock)"]
    PC["pre-commit hook<br/>platform/hooks/pre-commit<br/>4 checks: lint-ratchet, principle-edit, oversize, cargo --lib --bins"]
    AB["claudemd-gen auto-bump<br/>(_build counter on protocol-relevant changes)"]
  end

  subgraph PSH["Push path"]
    PQ["git-queue.sh push<br/>(rebase-on-conflict, retries under lock)"]
  end

  subgraph CI["CI / branch protection"]
    GH["GitHub Actions<br/>.github/workflows/quality.yml"]
    BP["Branch protection rules<br/>(legacy)"]
    RR["Repository Rulesets<br/>(new — DEC-2525 amendment)"]
  end

  WT --> Q
  Q --> PC
  PC --> AB
  AB --> PQ
  PQ --> GH
  GH --> BP
  GH --> RR
```

What's actually in place today:

- **git-queue.sh** — `commit` and `push` subcommands; `flock` on `/tmp/chorus-git-queue.lock` serializes across roles. Reads `DEPLOY_ROLE` env. Stages listed paths plus any prior-staged mods (#feedback_git_queue_stages_more_than_listed).
- **pre-commit hook** (`platform/hooks/pre-commit`) — runs lint-ratchet, principle-direct-edit, catalog-oversize, and `cargo test --lib --bins` per affected Rust crate (DEC-2525 wave 1). Skippable via `--no-verify` (CI is authoritative).
- **claudemd-gen auto-bump** — when fragments under `designing/claudemd/` change, `_build` counter increments and CLAUDE.md regenerates.
- **Branch naming** — `<role>/<card-id>` per kade's AC7 of #2556. Lifecycle is informal (branches accrete; #2440 had ~134 commits before merge).
- **Required checks** — listed in branch-protection AND Repository Rulesets; DEC-2525 amendment makes the lockstep explicit but enforcement is manual when adding a new check.
- **Hooks layer** (Layer 1 of ADR-026) — pre-commit only. No post-commit, no pre-push (a pre-push would catch git-queue branch-blindness if the user-side push slipped past the queue).

## To-Be

```mermaid
flowchart TB
  subgraph WT["Per-role worktrees (#2582, #2583, #2585)"]
    K["/chorus-kade — kade<br/>HEAD = kade/<card>"]
    W["/chorus-wren — wren<br/>HEAD = wren/<card>"]
    S["/chorus-silas — silas<br/>HEAD = silas/<card>"]
  end

  subgraph CMT["Commit path (unchanged surface, hardened)"]
    Q["git-queue.sh commit<br/>+ branch-check (#2580)"]
    PC["pre-commit hook<br/>(GIT_* env scrub fixed in #2560)"]
    AB["claudemd-gen auto-bump"]
  end

  subgraph PSH["Push path"]
    PQ["git-queue.sh push<br/>+ pre-push hook (deferred)"]
    R["rebase-merge to main<br/>(no merge-commits per #2556)"]
  end

  subgraph CI["CI required-checks"]
    GH["GitHub Actions"]
    LS["Lockstep enforcer (deferred)<br/>branch-protection ↔ rulesets parity check"]
  end

  K --> Q
  W --> Q
  S --> Q
  Q --> PC
  PC --> AB
  AB --> PQ
  PQ --> R
  R --> GH
  GH --> LS
```

The five To-Be moves:

1. **Per-role worktrees as default** — convention extends to all three roles. Symlink at `~/.claude/projects/<encoded-cwd>/` preserves Claude Code memory continuity (Wren's #2585 spike). Per-card worktrees become a documented exception for crisis/swat work.
2. **git-queue branch-check** (#2580) — verifies `HEAD` matches `<role>/*` before staging; refuses to commit on a peer's branch.
3. **GIT_* env scrub** in code paths that spawn `git` (#2560 wave 2 fixed `file_has_git_history`; sweep needed across other call sites).
4. **Role-suffixed runtime sockets** — `/tmp/chorus-hooks-<role>.sock` once per-role daemons land (Silas's #2559 SIGTERM cleanup is the precondition; multi-role launch is the trigger). Today's single-instance assumption is fine while only one role runs the daemon, but per-role worktrees + per-role daemons removes that assumption.
5. **Required-checks lockstep enforcer** (deferred card) — script that diffs branch-protection's required-checks against Repository Rulesets', fails CI if they drift.

## Single Contract

> **A commit lands when, and only when, it (a) was authored on the role's own worktree and branch, (b) passed the local pre-commit hook layer, (c) was pushed via git-queue, (d) merged to main via rebase-merge after CI's required-checks (mirrored in branch-protection AND rulesets) ran green, and (e) the merged SHA is the SHA against which required-checks ran green.**

Failure modes the contract names:
- (a) excludes cross-branch contamination — caught by per-role worktrees + git-queue branch-check.
- (b) excludes broken-tree commits — caught by pre-commit; CI is the authoritative re-check.
- (c) excludes raw-`git push` races — git-queue.sh push is the only sanctioned path.
- (d) excludes merge-commit cruft and one-system-only required-checks — rebase-merge per #2556, lockstep per DEC-2525 amendment.
- (e) excludes stale-CI-on-force-push and config-drift between checks-ran-against and checks-required-on-merge — addressed via GitHub branch-protection setting "Require branches to be up to date before merging" plus the lockstep enforcer (deferred card). Per Silas's #44 review: day-1 without (e) we hit a case where (a-d) all read green and a force-pushed SHA lands on main without re-running checks.

## Worktree grain — resolved

**Per-role is the convention.** Per-card is the exception.

Rationale:
- **Per-role** retires the substrate condition (shared `.git/HEAD`) for the failure class. One worktree per role, branch checkouts within the worktree are role-scoped, peer's checkout never affects yours.
- **Per-card** would multiply onboarding cost (Claude Code project-keying setup per worktree, per Wren's #2585 spike) and create transient state to garbage-collect.
- **Crisis/swat exception**: when a role is mid-card and a swat lands on the same role, a temporary chorus-<role>-<swatcard> worktree is acceptable. Document, retire after merge.

Onboarding cost (per role, one-time):
1. `git worktree add ../chorus-<role> <role>/main-checkpoint`
2. **Memory continuity choice** (per Wren's #2585 + #2583 reasoning): either (a) symlink `~/.claude/projects/<encoded-cwd-of-worktree>` → `~/.claude/projects/<encoded-cwd-of-canonical>` to preserve accumulated session memory, OR (b) split-memory: accept that the new worktree's project-key starts empty and rebuilds in-place. (a) preserves continuity at the cost of one piece of indirection; (b) honors no-symlinks discipline at the cost of memory-rebuild time (days, not weeks). Both are valid; pick by your role's preference. Wren chose (b) for chorus-wren onboarding. Kade chose (a) implicitly during chorus-kade adoption today.
3. Update role startup CLAUDE.md fragment to launch from the worktree
4. Smoke-test: read a memory file, verify it round-trips

## Cross-branch contamination class — canonical evidence

Three incidents, 2026-04-29 ~11:00–11:30:
1. Wren's #2575 fix committed via git-queue from /chorus while kade's #2558 branch was checked out → fix landed on kade's branch.
2. Kade started #2558 narrow-refinement work in /chorus while wren's branch was the active checkout → edits applied against wren's tree.
3. Kade's manifest.json regeneration in /chorus while a third checkout was active → contaminated mid-rebase.

Substrate cause: `/chorus/.git/HEAD` is one variable shared across all sessions in `/chorus/`. Trigger: any role's `git checkout` mutates it. No detection until after the fact.

The class disappears under per-role worktrees because each worktree has its own `HEAD`. git-queue branch-check (#2580) is defense-in-depth.

## Branch protocol

- **Naming**: `<role>/<card-id>` (e.g., `kade/2560-clippy-ops-cascade`). Optional kebab suffix for human readability.
- **Lifecycle**: branch lives from `cards move WIP` to merge. After merge, branch is deleted via GitHub PR auto-delete.
- **Retirement**: stale branches (>14 days no commits, no open PR) are cleaned up by the role who created them at session close. Inventory check via `git branch -r --merged main`.
- **Long-lived branches** (134-commit #2440 case) are an antipattern; if accretion happens, file a card to wave-merge.

## Push/merge protocol

- **Push**: only `git-queue.sh push`. Raw `git push` is contraindicated (loses lock serialization, no rebase-on-conflict).
- **Conflict resolution**: rebase, not merge. `git update-index --add` for hook-blocked staging during rebase resolution; commit lands as the rebased version of the original.
- **Merge style**: rebase-merge (#2556 lesson — merge-commits in main caused the 134-commit untangling). GitHub PR setting "Allow rebase merging" only.
- **Required checks**: branch-protection AND Repository Rulesets must list the same checks (DEC-2525 amendment). Until the lockstep enforcer ships, manual diff at every promotion.
- **Merge-on-CI-green discipline**: don't merge red. Don't merge yellow with the intent to "fix forward" — fix on the PR branch.

## Hook layer (relationship to ADR-026)

Layer 1 of ADR-026 ("will this commit obviously break something?") = pre-commit. Today this layer covers:

- Lint-ratchet (eslint baseline non-regression)
- Principle-direct-edit (catalog content protection)
- Catalog-oversize (file-size budget)
- Rust hermetic test slice (`cargo test --lib --bins` per affected crate)
- Doc-coherence ratchet (when CLAUDE.md fragments change)

High-priority follow-on:
- **pre-push hook** — load-bearing for the cross-branch-commit failure class, not optional. **#2580 (Done) closes the queue-path half** by enforcing `branch-prefix = role` inside `git-queue.sh commit` and `git-queue.sh push`. A pre-push hook is the **unfinished defense-in-depth half** — it would catch raw `git push` invocations that bypass git-queue entirely. Together they cover both queue-path and bypass-path; without pre-push, anyone running `git push` directly still slips through. Driver: cross-branch failure class. Per Silas's #2580 review.

Deferred / not yet shipped:
- **post-commit hook** — would emit `commits.commit.created` spine events for analytics. Cost: noise unless filtered.
- **commit-msg hook** — would enforce `<role>: <verb> #<card>` format. Cost: existing message lint isn't blocking and hasn't drifted enough to warrant.

The principle: **Layer 1 (hooks) makes the obviously-broken case fast-fail; Layer 3 (CI) is authoritative on main.** Don't let Layer 1 grow into a CI mirror — its job is the obviously-broken slice, not the full battery.

## Governance — DEC-2525 required-checks

DEC-2525 amendment names two surfaces (branch protection + Repository Rulesets) as joint authorities. Promotion criteria for adding a check:
1. Check is hermetic (no external state, no network, no live-role coupling — axis-4 per #2526).
2. Check has been run in CI as `continue-on-error: true` for ≥1 week with no flake.
3. Check is added to **both** branch-protection and Repository Rulesets in the same PR.
4. Lockstep verified manually post-merge until the enforcer ships.

Demotion criteria: if a check flakes ≥3× in 30 days for non-code reasons (env, infra), demote to non-required and file a hardening card.

## Spine events

Canonical namespace: `commits.*` (rename existing `git-queue.commit.*` emissions to `commits.*` in the implementation pass — one canonical surface before #2528 sensor consumer registry reads it). Past-tense event shape per existing convention (`nudge.emitted`, `card.pulled`, `gate.completed`):

```
commits.queue.lock_acquired      <role> branch=<branch>
commits.commit.created           <role> branch=<branch> sha=<sha>
commits.commit.pushed            <role> branch=<branch> sha=<sha>
commits.branch.mismatch_detected <role> expected=<role>/* actual=<branch>   (#2580)
commits.merge.rebased            <role> pr=<num> base_sha=<sha>
commits.required_checks.drift_detected   delta=<list>   (lockstep enforcer)
commits.force_push.detected      <role> branch=<branch> old_sha=<sha> new_sha=<sha>   (clause-(e) backstop)
```

Renames vs the first draft: `queue.acquired → queue.lock_acquired` (disambiguates what was acquired); `queue.committed → commit.created` (past-tense, non-reflexive, symmetric with `card.pulled`); `queue.pushed → commit.pushed`; `branch.mismatch → branch.mismatch_detected` and `required_checks.drift → required_checks.drift_detected` (past-tense parity for noun-state events). Per Silas's review.

## Connections

- **#2582** (Done — worktree convention fragment) — this design names per-role as canonical.
- **#2583** (WIP — wren onboards) — pending #2585 spike outcome.
- **#2585** (Wren spike) — Claude Code project-keying via symlink. **Per-role worktree IS viable.**
- **#2580** (P1 — git-queue branch-check) — tactical, defense-in-depth.
- **#2556** (Done — rebase-merge lesson, #2440 untangling) — informs push/merge protocol.
- **#2498** (side-finding — required-checks asymmetry) — informs DEC-2525 amendment.
- **#2526** wave-N — pre-commit narrowed to `--lib --bins`; informs hook-layer scope.
- **#2560** wave 2 — GIT_* env scrub fix in `file_has_git_history` (one site).
- **#2588** (P3 — Wave 3 dead-code) — architectural orphan retirement under chorus-hooks.
- **#2589** (P2 — env-scrub helper) — class-fix follow-on to #2560; migrates three known git-spawn sites behind a `git_command()` helper.
- **ADR-026** — three quality layers; this design slots into Layer 1 + Layer 3 boundary.
- **DEC-2525** + amendment — required-checks governance.
- Sibling designs: nudge-service-design (Silas), independence-service-design (Wren), gate-set-service-design (Wren).
- **#2592 (P2 — forward arc)** — workspace-API ("code asks the service, doesn't spawn git"). Per-role worktrees retire the `HEAD`-sharing instance of role-identity-bound-to-filesystem-location coupling, but not the underlying class — code that reaches into git's filesystem state (e.g., `file_has_git_history` spawning `git log` and inheriting GIT_*) carries the same coupling. The forward arc decouples by mediating all git access through a service. This design (#2586) does not block on it; it slots under #2592 when that lands.

## References

- `platform/scripts/git-queue.sh` — commit/push surface
- `platform/hooks/pre-commit` — Layer 1 hook
- `designing/claudemd/manifest.json` — auto-bump trigger
- `.github/workflows/quality.yml` — CI required jobs
- `roles/kade/CLAUDE.md` — Per-Role Worktree Convention fragment (#2582)
- `decisions/DEC-2525.md` + amendment — required-checks governance
- `decisions/ADR-026.md` — three quality layers
- Wren's #2585 spike brief (chat transcript 2026-04-29) — Claude Code project-keying findings
