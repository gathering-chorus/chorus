# Commits Service Design

**Kade, 2026-04-29 / refreshed 2026-04-30 (post worktree-contamination guard, pre-push hook, plain-language pass, subagent corrections folded). This doc is the canonical source for the five pre-merge enforcement surfaces; the CI/CD service design references this doc for surface detail rather than restating.**

## Promise

Every commit that reaches `main` was authored on the right branch in a non-shared working tree, passed pre-commit checks, was pushed via the serialized commit queue, and merged via rebase against an up-to-date base. Five hooks enforce that contract. An engineer joining the project should be able to read the contract once and understand which hook catches which failure mode.

## Vocabulary

| Internal term | Industry standard | What it actually is |
|---|---|---|
| `git-queue.sh` | Serialized commit + push wrapper | Mutex-locked git wrapper enforcing branch-name conventions |
| `werk` | Deploy wrapper script | Shell script bundling cargo build + claudemd-gen + install-hooks under a SHA gate |
| `werk check` | Status query | Reports whether the local worktree is in sync with `main` |
| Per-role worktree | One git worktree per developer | Sibling clones (`/chorus-kade`, `/chorus-wren`, `/chorus-silas`) so each session has its own `.git/HEAD` |
| Topic worktree | Per-card worktree | Optional sibling clone for a specific card (e.g., `/chorus-2526`); used for crisis/swat work |
| Spine events / `chorus.log` | Structured audit log | JSONL event stream for incident reconstruction |
| Card | Ticket | Vikunja kanban ticket (e.g. #2580) |
| Wren / Kade / Silas | PM / Engineer / Architect | Role names for the three engineers on the team |
| Jeff | Project owner | The human directing the team |
| `chorus-hook-shim` | Hook executable | Rust binary called by Claude Code's PreToolUse for refusals |
| PreToolUse | Anthropic Claude Code hook surface | Pre-execution gate that can deny tool calls before they run |
| `claudemd-gen` | CLAUDE.md regeneration script | Auto-bumps a build counter and regenerates per-role CLAUDE.md from fragments under `designing/claudemd/` |
| Loom | Decision/principle/practice graph subdomain | Where ADRs (`loom-decisions`), principles, patterns, policies live |
| DEC-NNN / ADR-NNN | Decision record | A numbered architecture/team decision document |

## Problem (origin context, 2026-04-29)

Commits emerged as a stack of tactical fixes, not a designed surface. One session surfaced four distinct failure classes:

1. **Cross-branch contamination** — three engineers shared `/chorus`'s working tree; one engineer's `git checkout` was the others' working state. `.git/HEAD` is one variable.
2. **Required-checks asymmetry** — branch-protection and Repository Rulesets listed different required jobs. Merges that satisfied one bypassed the other.
3. **Branch-blindness** — `git-queue.sh` committed whatever branch was checked out. If a peer's branch was active, the commit landed on it.
4. **Worktree grain unresolved** — the architect adopted per-card worktrees, the engineer adopted per-role, the PM was mid-decision. No design said which was canonical.

Each was handled tactically without a unified design surface. This doc is the surface.

## The Five Enforcement Surfaces

This is the canonical list. Each surface catches a distinct attack vector on the path from "engineer edits code" to "main has the change." All five emit JSONL audit events to `platform/logs/chorus.log` for incident reconstruction.

| # | Attack vector caught | Hook | Card |
|---|---|---|---|
| 1 | **Wrong-branch commit** (engineer commits to a peer's branch via shared `HEAD`) | Per-role worktrees + `git-queue.sh` branch-check | #2582 + #2580 |
| 2 | **Raw push** (bypassing the queue) | pre-push hook | #2598 |
| 3 | **Dangerous git from agent** (Claude Bash tool runs raw `rebase`/`reset --hard`/`cherry-pick`) | `chorus-hook-shim` PreToolUse refusal | #2598 |
| 4 | **Cross-worktree contamination** (running git ops on the canonical clone — unconditional; the canonical clone is read-only by invariant, not by who-is-building) | worktree-contamination guard | #2625 |
| 5 | **Bad commit content** (typecheck/lint/test failure, secrets, oversize, dup-strings, cog-complexity over 12) | pre-commit hook | (built up over many cards including #2603, #2627) |

The five enumerate the distinct attack vectors — wrong-branch, raw-push, dangerous-agent-git, cross-worktree, bad-content. They aren't five chosen for symmetry; they're derived from where wrong-shape commits can enter the system.

Surface 5 (pre-commit) is the only one bypassable via `--no-verify`, intended for the carded `KNOWN_FAILS` pattern (carded test failures from other engineers, traced via `#NNNN` in the commit message). The allowlist enforcing that convention is #2497 (planned). Note: `--no-verify` bypasses **all** pre-commit checks, not just the KNOWN_FAILS scope — until #2497 lands, the convention is honor-system.

```mermaid
flowchart TB
  subgraph WT["Per-role worktrees (#2582)"]
    K["/chorus-kade<br/>HEAD = kade/&lt;card&gt;"]
    W["/chorus-wren<br/>HEAD = wren/&lt;card&gt;"]
    S["/chorus-silas<br/>HEAD = silas/&lt;card&gt;"]
  end

  subgraph CMT["Commit + push path (5 enforcement surfaces — see table)"]
    Q["#1 git-queue.sh commit<br/>+ branch-check"]
    PC["#5 pre-commit hook"]
    PP["#2 pre-push hook"]
    SHIM["#3 chorus-hook-shim<br/>PreToolUse refusal"]
    WTG["#4 worktree-contamination guard"]
  end

  subgraph PSH["Merge path"]
    R["rebase-merge to main<br/>(no merge-commits per #2556)"]
  end

  subgraph CI["CI / required checks"]
    GH["GitHub Actions<br/>scheduled-only post-#2600"]
    BP["Branch protection — required_status_checks<br/>currently empty (DEC-2525)"]
    RR["Repository Rulesets — required_status_checks<br/>currently empty (DEC-2525)"]
  end

  K --> Q
  W --> Q
  S --> Q
  Q --> PC
  PC --> PP
  PP --> R
  R --> GH
  GH --> BP
  GH --> RR

  K -.PreToolUse.-> SHIM
  K -.PreToolUse.-> WTG
```

## The Single Contract

> **A commit lands when, and only when:**
>
> **(a)** it was authored on the engineer's own per-role worktree, AND
>
> **(b)** the commit's branch matches the engineer's role-prefix (`<role>/*`), AND
>
> **(c)** the commit passed the pre-commit hook layer (or carries a tracked `--no-verify` exemption), AND
>
> **(d)** it was pushed via `git-queue.sh` (raw `git push` is refused at three surfaces), AND
>
> **(e)** it was merged to `main` via rebase (no merge-commits) against an up-to-date base, AND
>
> **(f)** if any required checks are configured, the merged SHA is the SHA against which they ran green.

Failure modes the contract names:
- **(a)** excludes cross-branch contamination at the substrate (wrong worktree → wrong `HEAD`)
- **(b)** excludes branch-blindness at the queue (right worktree, wrong branch)
- **(c)** excludes broken-tree commits — pre-commit catches obvious-broken; CI's nightly run is authoritative-but-delayed
- **(d)** excludes raw-push races — refused at pre-push hook + shim PreToolUse + infra-guardrails for non-Claude callers
- **(e)** excludes merge-commit cruft (#2556 — merge-commits caused a 134-commit untangling)
- **(f)** is currently vacuously satisfied (required checks are empty post-#2600 cost-stop). Becomes load-bearing when checks return; #2500 detector closes the manual loop

**Documented escape hatches** (each is bounded and audited):
- `--no-verify` on commit — bypasses pre-commit; convention requires `#NNNN` trace; allowlist (#2497) not yet wired
- `// cog-override: <reason>` magic comment — exempts a function from cog-complexity at error 12; emits a `cog.override.used` audit event per applied override
- `# worktree-override` magic comment — exempts a Bash command from the worktree-contamination guard for legitimate cross-worktree work; emits an audit event
- `DEPLOY_ROLE_PREPUSH_OVERRIDE=1` env var — bypasses the pre-push hook for emergency recovery; not for routine use

## Worktree convention — per-role is canonical

Per-role is the convention. Per-card (topic worktrees like `chorus-2526`) is the exception, retired after merge.

Today's actual worktree topology:
- `/chorus` — canonical clone; intended for read-only inspection + `git fetch`. **Today's drift:** sometimes carries an in-flight role branch (e.g. wren/2631 during this session). When a role works there, the cross-worktree contamination guard fires
- `/chorus-kade`, `/chorus-wren`, `/chorus-silas` — per-role worktrees with isolated `HEAD`
- `/chorus-2526` — silas topic worktree (still open for that card's work)

Rationale for per-role as default:
- **Per-role** retires the substrate condition (shared `.git/HEAD`) for the failure class. One worktree per role; branch checkouts within the worktree are role-scoped; a peer's checkout never affects yours
- **Per-card** would multiply onboarding cost (Claude Code project-keying setup per worktree per Wren's #2585 spike) and create transient state to garbage-collect

Onboarding (per role, one-time):
1. `git worktree add ../chorus-<role> <role>/main-checkpoint`
2. **Memory continuity choice** (per Wren's #2585): either (a) symlink `~/.claude/projects/<encoded-cwd-of-worktree>` → canonical to preserve session memory, OR (b) split-memory: accept that the new worktree starts empty. Both are valid; pick by preference. Wren chose (b); Kade chose (a)
3. Update role startup CLAUDE.md fragment to launch from the worktree
4. Smoke-test: read a memory file, verify it round-trips

## Branch protocol

- **Naming**: `<role>/<card-id>` (e.g., `kade/2580-git-queue-branch-check`). Optional kebab suffix for human readability
- **Lifecycle**: branch lives from `cards move WIP` to merge. After merge, branch is deleted via GitHub PR auto-delete
- **Retirement**: stale branches (>14 days no commits, no open PR) cleaned up at session close. Inventory via `git branch -r --merged main`
- **Long-lived branches** are an antipattern — if accretion happens, file a wave-merge card

## Push / merge protocol

- **Push**: only `git-queue.sh push`. Raw `git push` is refused at three surfaces (pre-push hook, shim PreToolUse, infra-guardrails for non-Claude callers)
- **Conflict resolution**: rebase, not merge. Hook-blocked staging during rebase resolution: `git update-index --add`. The rebased commit lands as the original author
- **Merge style**: rebase-merge (#2556 — merge-commits in main caused a 134-commit untangling). GitHub PR setting "Allow rebase merging" only
- **Required-checks lockstep**: branch-protection AND Repository Rulesets must list the same checks (DEC-2525 amendment). Both are deferred today (empty post-#2600 cost-stop). The lockstep enforcer (#2500) is not in force; it becomes load-bearing when required checks return

## Pre-commit layer (Surface 5 detail)

`platform/hooks/pre-commit` runs as part of every `git-queue.sh commit`. Each check fires only when staged files trigger its scope.

- **Typecheck** — `tsc --noEmit` on changed TypeScript packages
- **Tests** — `jest` on changed JS/TS, `cargo test --lib --bins` on changed Rust crates (hermetic only)
- **Secrets scan** — refuses commits writing API keys / .env / credentials
- **Catalog oversize** — refuses oversize binaries to `data/catalog/`
- **Principle direct-edit** — refuses edits to `data/principles/` outside the canonical write path
- **Sonarjs error tier (Check 4.4, #2603 + #2627)** — blocks on `no-duplicate-string` (threshold 5) or `cognitive-complexity` (threshold 12) in staged TypeScript. Magic-comment override `// cog-override: <reason>` exempts a function and emits an audit event
- **MCP tool description shape** — staged MCP tool definitions match the description-shape contract
- **Doc-coherence ratchet** — when `claudemd-gen` source fragments change, doc references stay coherent

The CI/CD service design previously duplicated this checklist verbatim; that doc now references this section as the canonical source.

## Related patterns surfaced 2026-04-30

These aren't part of the commit-flow itself but originate from the same substrate work and share enforcement surfaces. They're noted here for cross-reference; detail lives in the CI/CD service design.

- **Affordance-layer refusal** (#2467, #2629) — when a deprecated input shape needs to disappear, refuse it at every surface that could reach the affordance (writer parser + HTTP API + bats gate + schema column drop). One invariant; the four surfaces are different from this doc's five
- **Retirement gates** (#2467, #2632) — when a surface is retired, the deletion ships with a small bats file containing grep-assertions enforcing structural absence. Forward-only structural assertion encoded as test
- **No-warn-tier convention** — every lint either blocks or doesn't fire (Jeff's call 2026-04-30). Discipline, not currently self-enforced

## Implementation Plan (existing cards mapped to slots)

Done across the prior arc and today's session:
- Per-role worktree convention — #2582 ✓
- Branch-check at queue surface — #2580 ✓ (with #2597 silent-exit fix)
- Pre-push hook + shim PreToolUse refusal — #2598 ✓
- Worktree-contamination guard — #2625 ✓ (with #2626 follow-on RCA)
- Pre-commit error-tier quality rules — #2603 + #2627 ✓
- Wren onboarded to chorus-wren worktree — #2583, #2585 ✓
- `claudemd-gen` auto-bump on fragment change — earlier work ✓

**Phase A — pre-commit maturity:**
1. **#2497 KNOWN_FAILS allowlist** — codifies `--no-verify` + `#NNNN` trace as machine-checkable
2. **#2496 Ratchet baseline drift signal** — surface when a per-rule baseline can shrink instead of letting accretion sit silent

**Phase A.5 — substrate hardening (separate concern from pre-commit hooks):**
3. **#2589 chorus-hooks git-spawn env-scrub helper** — migrate 3 known sites + audit. Hardens git-spawning code paths so they don't inherit GIT_* env from the parent process
4. **#2599 Sweep remaining 24 chorus scripts to source-from-substrate** — eliminate-runtime-dep applied to script bootstrap; commits-flow scripts inherited the old shape

**Phase B — required-checks governance (when checks come back):**
5. **#2500 Required-checks drift detector** — script that diffs branch-protection's required-checks against Repository Rulesets'; fails if they drift. Vacuously satisfied today (both empty), load-bearing when checks are reinstated

**Phase C — observability + namespace cleanup:**
6. **Spine namespace consolidation** (no card yet) — current state is a mix: `commits.branch.mismatch_detected` already on `commits.*`; `commit.landed` is legacy from #2193 (singular); queue/push/ontology under `build.queue.*` / `build.push.*` / `build.ontology.*` / `ontology.version.changed`. Target: unify under `commits.*`. File a card before next session frame
7. **#2588 Wave 3 dead-code retirement under chorus-hooks** — architectural orphan cleanup
8. **#2200 Cross-language contract tests** — TS↔Rust hash-parity is the only enforcement today; broader coverage when wired

**Successor design (NOT a phase of this design):**
- **#2592 workspace-API** — code asks the service, doesn't spawn git directly. If/when this lands, it dissolves the queue layer, the shim PreToolUse refusal, and the worktree convention into a single mediated API. That's a successor design that obsoletes this one, not a continuation of it. Deserves its own doc and a deprecation story for `git-queue.sh`

## Spine events (current + target)

Today's emissions are a mix of namespaces inherited from the build-up of the substrate. Reality (verified 2026-04-30 against `git-queue.sh`):

| Event | Current namespace | Source |
|---|---|---|
| Branch mismatch detected | `commits.branch.mismatch_detected` | `git-queue.sh:64` (#2580) |
| Commit landed | `commit.landed` (singular, legacy) | `git-queue.sh:329` (#2193) |
| Queue lock acquired/released | `build.queue.*` | `git-queue.sh:207, 213, 221, 263, 286, 290, 340` |
| Push started/completed/failed | `build.push.*` | `git-queue.sh:370, 399, 404` |
| Build/ontology version changed | `build.ontology.*` / `ontology.version.changed` | `git-queue.sh:301` |

Target: unify under `commits.*` in past-tense parity with `nudge.emitted` / `card.pulled`. New emissions to add:
```
commits.queue.lock_acquired      <role> branch=<branch>
commits.commit.created           <role> branch=<branch> sha=<sha>
commits.commit.pushed            <role> branch=<branch> sha=<sha>
commits.merge.rebased            <role> pr=<num> base_sha=<sha>
commits.required_checks.drift_detected   delta=<list>   (#2500 lockstep enforcer when wired)
commits.force_push.detected      <role> branch=<branch> old_sha=<sha> new_sha=<sha>   (clause-(f) backstop)
```

The rename + new emissions land together as the namespace consolidation card (Phase C item 6).

## Hook-layer relationship to ADR-026

ADR-026 names three quality layers:
- **Layer 1 (pre-commit)** — "will this commit obviously break something?" Fast-fail; skippable; CI is authoritative on `main`
- **Layer 2 (role gates)** — card-level acceptance, recorded as PR/card comments
- **Layer 3 (CI)** — scheduled re-run on `main`, post-merge witness

This design slots into Layer 1 + the substrate beneath it (worktrees, queue, hooks-on-Bash-tool). Layer 1's job is the obviously-broken slice, not a CI mirror.

## Surfaces

- **`git-queue.sh`** — `commit`, `push` subcommands; serialized via `flock`
- **`werk check`** — local visibility into "is this worktree in sync with main"
- **`platform/hooks/pre-commit`** — Layer 1 enforcement
- **`platform/hooks/pre-push`** — Layer 1 push-side enforcement (refuses raw push)
- **`chorus-hook-shim`** PreToolUse — refuses raw `rebase`/`cherry-pick`/`reset --hard`/dangerous-git on canonical clone
- **Audit log** — `platform/logs/chorus.log` — JSONL events from all enforcement surfaces
- **GitHub Actions** — `https://github.com/gathering-chorus/chorus/actions`
- **Branch protection + Repository Rulesets** — DEC-2525-governed lockstep, both empty today
- **Pipelines-domain page** — `http://localhost:3340/gathering-docs/domain-detail.html?id=pipelines-domain`

## Gaps (open work)

The Implementation Plan above ties each gap to its card. Summary by impact:

1. **No `KNOWN_FAILS` allowlist** (#2497) — `--no-verify` convention is uncoded; mechanism is unbounded today
2. **No ratchet baseline drift signal** (#2496) — baseline accretion only caught via manual audit
3. **GIT_* env scrub incomplete** (#2589) — 3 known sites need helper migration
4. **Required-checks drift detector** (#2500) — deferred today (both lists empty); load-bearing when checks return
5. **Spine namespace not yet unified** under `commits.*` — current state is a mix of `commits.*` + `commit.*` + `build.*` + `ontology.*`. Card filing pending
6. **Workspace-API successor design** (#2592) — when started, obsoletes parts of this design; needs its own doc
7. **Daemon-during-rebase race** (no card yet) — surfaced 2026-04-30 when `git-queue.sh`'s stash-pull-pop dance raced with `claudemd-gen` daemon writing to `manifest.json`. Hit twice in one session. Needs pattern-naming + card
8. **Two-doc-lockstep with no detector** — this doc and the CI/CD service design must co-update when the surface table changes. There's no automated check that catches drift. Same failure class DEC-2525 names for required-checks but applied to designs themselves
9. **Canonical clone drift** — `/chorus` is intended as read-only canonical, but in practice currently carries an in-flight role branch. Cross-worktree contamination guard fires correctly when this happens, but the convention "/chorus is read-only" isn't structurally enforced

## Connections

- **Sibling designs**: CI/CD service design (`ci-pipeline-service-design.md` — references this doc for surface detail; covers Layer 3 + the meta-shape of the three-layer system); nudge service design; gate-set service design
- **Co-domain**: #2636 (Silas's substrate-debt sweep) — same hooks-on-text-substring class surfaces in commits flow; hooks-read-PreToolUse-JSON-not-bash-strings is the contract Silas + Kade locked

## Not in scope

- CI / Layer 3 — covered by CI/CD service design
- Role gates / Layer 2 — covered by sibling designs
- Production deploy — `werk` handles canonical deploys, not commits
- Test pyramid + coverage thresholds — Quality service design

## References

- `platform/scripts/git-queue.sh` — commit/push surface
- `platform/hooks/pre-commit` + `pre-push` — Layer 1 hooks
- `platform/services/chorus-hooks/src/hooks/{worktree_contamination_guard,infra_guardrails}.rs` — PreToolUse refusal
- `designing/claudemd/manifest.json` — auto-bump trigger
- `roles/<role>/CLAUDE.md` — Per-Role Worktree Convention fragment (#2582)
- ADR-026 — three quality layers
- DEC-2525 + amendment — required-checks governance
- Wren's #2585 spike brief — Claude Code project-keying findings
- Cards (Done): #2580, #2582, #2583, #2585, #2598, #2625, #2603, #2627, #2467, #2629, #2632, #2526, #2611
- Cards (Phase A): #2497, #2496
- Cards (Phase A.5): #2589, #2599
- Cards (Phase B): #2500
- Cards (Phase C): #2588, #2200 (+ spine-namespace consolidation card pending)
- Successor design: #2592 (workspace-API)
