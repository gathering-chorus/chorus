# CI Pipeline — Service Design

**Kade, 2026-04-25. Draft. Source: `.github/workflows/quality.yml` (CI workflow), `platform/hooks/pre-commit` (Layer 1), `/gate-*` skills (Layer 2), ADR-026 (CI architecture + lock-file policy), `proving/domains/alerts/ci-main-red.yml` (red-main alert), today's session (#2481, #2487, #2494, plus the cracks #2491–#2500 surfaced).**

## Promise

Every change that lands on `main` builds and passes the team's quality bar from a fresh runner — independent of what happens to be cached on any role's laptop. When CI is healthy, Jeff can ask "is `main` shippable?" and the answer is yes by construction: branch protection blocked anything red, and if anything slips, an alert routes to Silas inside 10 minutes. When CI drifts, locks fall out of sync with package.json, ratchet baselines accumulate silent debt, in-flight cross-role work breaks other roles' PRs, the required-checks list diverges from the workflow, and Jeff becomes the test suite — finding regressions by touching the product and re-deriving the layer model mid-session.

## Overview

CI is layer 3 of the three-layer quality system defined in ADR-026 §a. Each layer answers a different question with a different threat model — redundancy is intentional, not duplication. Pre-commit (Layer 1) is honest-mistake feedback, fast and skippable. Role gates (Layer 2) are card-acceptance — recorded on the card, owned per concern. CI (this) is **authoritative on `main`** — branch-protection-enforced, not skippable. The threat CI closes specifically: `--no-verify` plus the local-vs-CI environment divergence that happens when locks aren't honored or when one role's machine has node_modules another role's doesn't.

The pipeline is a single GitHub Actions workflow at `.github/workflows/quality.yml` running on push-to-main and pull-request-to-main. Job count today is 13 required-status-checks; mcp-roundtrip deferred to #2495 pending ESM/CJS reconciliation. Each job names exactly one logical check so failures point at one concern. Lock-file policy (ADR-026 §c) is load-bearing: per-package `package-lock.json` committed, `npm ci` (not `npm install`), Dependabot for centralized updates. Red main fires `ci-main-red.yml` to Silas with daily cooldown.

| Component | Status | Source | Gap |
|-----------|--------|--------|-----|
| Workflow definition | REAL | `.github/workflows/quality.yml`, 13 required jobs | mcp-roundtrip job deferred (#2495 ESM/CJS) |
| Pre-commit (Layer 1) | REAL | `platform/hooks/pre-commit` runs tsc/jest/cargo/lint per changed package | KNOWN_FAILS pattern uses --no-verify w/ trace; not enforced in tooling (#2497) |
| Role gates (Layer 2) | REAL | `/gate-product` `/gate-code` `/gate-quality` `/gate-arch` `/gate-ops` skills | Demo skill assumes user-facing; infra cards don't fit (#2499) |
| Branch protection | REAL | classic protection on `main`, 13 required checks, admin bypass allowed | Org also has Rulesets enforcing same 13 — duplicate enforcement (#2498 → Silas pulled) |
| Lock-file policy | REAL | per-package package-lock.json committed, `npm ci` in CI, Dependabot config | **Initial 23 lock-update PRs flooded inbox** — expected one-time, weekly thereafter |
| Reactive alert | REAL | `proving/domains/alerts/ci-main-red.yml` polls Actions API every 10 min | No host-down alert if poller dies (separate concern) |
| ESLint ratchet | REAL | `npm run lint:ratchet` enforces per-rule baseline | Baseline drift between branches is silent (#2496) |
| Required-checks ↔ workflow coupling | **MANUAL** | `Settings → Branches → Edit main rule` lists job names by hand | Rename a job → check runs but doesn't gate; **#2500 drift detector filed** |
| Smoke-check (app surface) | OUT OF SCOPE | Lives in `jeff-bridwell-personal-site` repo's own CI | ADR-026 addendum 2026-04-25 |
| MCP round-trip (live api) | DEFERRED | Was attempted in #2487 — surfaced ESM/CJS issue | #2495 carries the real api-boot work |
| Test-isolation in chorus-hooks/inject | PARTIAL | cargo-test jobs green via CHORUS_ROOT env override | Hardcoded `/Users/jeffbridwell` paths in tests (#2491 — Silas-owned) |

## Sub-Domain Interaction Model

CI doesn't act directly — it's invoked by GitHub on commit/PR events and produces status checks consumed by branch protection and the red-main alert. Like Quality, the service is a **measurement + verdict surface**, not an enforcer; branch protection is the enforcer that consumes verdicts.

| Trigger | Produces | Consumed By | Surface |
|---------|----------|-------------|---------|
| Push to `main` | One workflow run per push, 13+ status checks | Branch protection (rejects if not green), `ci-main-red.yml` alert poll | `https://github.com/gathering-chorus/chorus/actions` |
| Pull request targeting `main` | One workflow run per head-commit, 13+ status checks | Branch protection (blocks merge if red), Wren's gate-product (informational) | PR checks tab |
| Workflow run completes red on `main` | `red:<run-url>` line in alert check stdout | `proving/domains/alerts/ci-main-red.yml` action — nudges Silas, daily cooldown | `nudge silas` (osascript + messaging API) |
| Job names change in `quality.yml` | (should produce) drift event vs required-checks list | (should feed) #2500 drift detector | **NOT WIRED — #2500** |
| Lock-file out of date with package.json | Dependabot opens a per-package PR, weekly Monday | Role review queue, then merge → workflow runs against new lock | `gh pr list --label deps` |
| Ratchet baseline shrinkable | (today) silent — no signal | (should feed) #2496 ratchet-drift signal | **NOT WIRED — #2496** |
| Test fails but is upstream-known | --no-verify with #NNNN trace in commit message | (today) commit-message convention only; no machine-checkable allowlist | **NOT WIRED — #2497** |

The core pattern: CI is a **verdict producer**, branch protection is the **enforcer**, and alerts are the **safety net**. Verdicts are independent — each job answers one question. Enforcement is policy applied to the verdict set. The safety net catches the case where verdict + enforcement both pass but main goes red anyway (admin-bypass push, post-merge regression, flake). Consistent with Quality (model + measurements; gates consume) and Roles (source not enforcer).

## Dependencies

Per Wren's 4-layer pattern (#2159): CI sits on principles + practices + policies + decisions.

| Dependency | Sub-domain | Status | Instances | Notes |
|-----------|------------|--------|-----------|-------|
| Principles | loom-principles | POPULATED | 3 CI-relevant | `quality-at-source` (CI catches what local layers missed), `tests-hermetic-by-default-integration-gated-explicitly` (CI runs hermetically; integration deferred), `infrastructure-is-your-codebase-too` (workflow + hooks are code, reviewed like code) |
| Practices | loom-practices | POPULATED | 2 CI-relevant | `production-ready` (CI = "does this build clean?"), `api-first` (workflow YAML is the API to enforcement) |
| Policies | loom-policies | MISSING | — | Sub-domain blocked on #2151. CI will depend on: KNOWN_FAILS allowlist policy (#2497), ratchet drift policy (#2496), red-main escalation policy (today inline in this doc, runbook section), required-checks coupling policy (#2500) |
| Decisions | loom-decisions | SHELL | 0 populated | ADR-026 (CI architecture + lock-file policy) and its 2026-04-25 addendum (smoke-check stays out) are CI-load-bearing but not yet harvested as graph instances; #2485 covers loom-decisions reach |

**Why dependencies matter to CI:** the layer model is a shape derived from principles (quality-at-source says CI is the safety net for what slipped through local), practices (production-ready says CI proves shippability), policies (when a check bites: pre-commit vs gate vs CI), and decisions (why specific checks bite where they do). When any layer is missing, CI's posture drifts. Observed today 2026-04-25: without a KNOWN_FAILS policy, role A's pre-existing test debt blocks role B's commit; B uses `--no-verify` with a trace, but the convention isn't machine-checkable, so future drift is invisible.

## Components

### Workflow definition (`.github/workflows/quality.yml`)
Single workflow, 13+ jobs, runs on push-to-main and PR-to-main. Each job is one concern: lint-ratchet, MCP shape, catalog-oversize, principle-direct-edit, tsc matrix (5 packages today; platform/api in flight as #2495 unblocks), jest matrix (4 packages), cargo-test matrix (chorus-hooks + chorus-inject). Adding a new job is mechanical (recipe in this doc) but coupling to required-checks list is manual until #2500 lands.

### Pre-commit hook (`platform/hooks/pre-commit`)
Layer 1. Rust-implemented via `chorus-hook-shim`. Runs Check 1 (tsc), Check 2 (jest), Check 4.6 (cargo test), Check 4.7 (doc-coherence-ratchet), plus secret-scan, ratchet, MCP shape. Skippable via `--no-verify` — explicitly overridden by Layer 3 as authoritative. KNOWN_FAILS pattern (carded test failures from other roles) currently goes through `--no-verify` with #NNNN reference in commit message; #2497 wires the allowlist.

### Role gates (Layer 2)
Five skills, one per concern: `/gate-product` (Wren — AC, Experience, domain registration), `/gate-code` (Kade — tests green, build clean, warning diff), `/gate-quality` (Kade — hooks, regression, console.log, smoke when local), `/gate-arch` (Silas — namespace, boundaries, structural fit), `/gate-ops` (Silas — health, log flow, rollback). Recorded as card comments + spine events. Skippable for `type:chore` and `type:swat` cards. Demo skill (DEC-048) assumes user-facing — infra cards adapt via #2499.

### Branch protection
Classic protection on `main` in `gathering-chorus` org. 13 required status checks. "Do not allow bypassing" unchecked → admins (Jeff + role tokens via Jeff's account) can push direct to `main`. Org-level Rulesets system also enforces same 13 checks; duplicate enforcement collapses under #2498 (Silas-owned, pulled).

### Lock-file policy + Dependabot
ADR-026 §c. Per-package `package-lock.json` committed, root `package-lock.json` committed, Cargo locks committed. CI runs `npm ci` (not `npm install`) — drift between local and CI is a red flag, surfaced loudly (the failure mode that fired #2481's first run when the root lock was gitignored). `.github/dependabot.yml` opens weekly Monday lock-update PRs across 11 ecosystem-directory pairs. Initial scan opened 23 PRs at once (#2-#24); steady-state should be 2-5 per week.

### Reactive alert (`ci-main-red.yml`)
Polls GitHub Actions API every 10 minutes for the most recent run on `main`. If `status=completed && conclusion=failure`, fires nudge to Silas with daily cooldown (one nudge per red incident, not per poll). No GH_TOKEN → check returns ok and skips silently (separate alert if Silas wants one). Read-only check; never auto-reverts (ADR-026 §d explicitly vetoes that).

### ESLint ratchet (`npm run lint:ratchet`)
Per-rule baseline at `platform/state/eslint-baseline.json`. Pre-commit and CI both run the same check. New rule firings require either a fix or `npm run lint:baseline` to adopt. Baseline updates today are manual; #2496 looks at automating or signaling drift.

### Required-checks contract
The integration point between workflow and merge-protection guarantee. Listed manually in `Settings → Branches → Edit main rule → Status checks`. Brittle: rename a job → check runs but doesn't gate; remove a job without removing from list → all PRs un-mergeable. **#2500 drift detector** closes this loop.

### Red-main runbook (Silas-owned)
When `ci-main-red` fires, the triage tree is: (1) recent merge + unrelated to in-flight → revert the merge, file fix card on original author; (2) forward-fix is quick (≤10 min, root cause obvious) → push fix direct to main, watch CI re-run green; (3) not (1) or (2) → file swat card with failing job name, mark main as known-red in #shared-observability, decide with team whether to revert or freeze merges. Daily cooldown means triage runs once per incident.

## Surfaces

- **GitHub Actions UI** — `https://github.com/gathering-chorus/chorus/actions` — primary read surface, full run history, log streaming.
- **PR Checks tab** — verdict per job for the head commit; what branch protection consults.
- **Branch protection settings page** — `https://github.com/gathering-chorus/chorus/settings/branches` — required-checks list (today's manual mirror of `quality.yml`).
- **Org Rulesets page** — `https://github.com/gathering-chorus/chorus/rules` — duplicate enforcement layer (collapses under #2498).
- **Dependabot PRs** — `gh pr list --label deps` — weekly lock-update flow.
- **`ci-main-red` nudge** — Silas's terminal + messaging API, daily cooldown.
- **Pipelines-domain page** — `http://localhost:3000/gathering-docs/domain-detail.html?id=pipelines-domain` — this design registered as a contract entity.

## Consumers

- **Branch protection** — consumes the verdict-set, blocks merge if any required check is red.
- **`ci-main-red.yml` alert** — consumes the verdict on `main` post-merge, routes red to Silas.
- **Roles (PR authors)** — consume their PR's verdict; fix what's red on their PR before requesting `/gate-code`.
- **Dependabot** — produces input (lock-update PRs); consumes output (CI verdict on those PRs).
- **`/gate-code` skill** — consumes pre-commit + recent CI state to decide gate verdict.
- **`/demo` skill (DEC-048)** — consumes the gate-chain state; today assumes user-facing demo, infra adapt via #2499.

## Gaps

- **#2495** — api-boot ESM/CJS reconciliation. Restores the deferred mcp-roundtrip CI job. Architecture call (CJS-with-fixed-paths vs full ESM migration) on Silas when card pulls.
- **#2496** — Ratchet baseline drift signal. Today silent; either auto-PR on shrinkable baseline or gate-code surface "could shrink by N" warning.
- **#2497** — KNOWN_FAILS allowlist. Pre-commit honors carded test failures with explicit reason; CI does not (CI is authoritative). Closes the `--no-verify` convention drift.
- **#2498** — Trunk-vs-Rulesets cleanup (Silas-owned, pulled). Pick one enforcement layer; document.
- **#2499** — Demo skill adapts for infra cards. Infra demo = prove the threat closes (introduce regression on throwaway branch, show CI catches), not "show CI green" status report.
- **#2500** — Required-checks drift detector. Diff job names in `quality.yml` against exported required-checks list, fail on drift.
- **#2491** — chorus-hooks/inject test isolation (Silas-owned). Removes hardcoded `/Users/jeffbridwell` paths.
- **#2493** — sessions.test.ts pre-existing fail (Wren-filed). Validator change made test stale; small fix.

## Next Steps

1. **Land PR #25** with lean scope — bats hermeticity + ADR-026 addendum. mcp-roundtrip deferred. Closes #2487.
2. **Pull #2500** (drift detector) once #2487 is done — small, high leverage, closes the silent-un-gate failure mode.
3. **Silas's #2498** lands — collapses the duplicate enforcement, gives us per-path rules for future role-state-files-unprotected work.
4. **#2495 + #2491 in parallel** — restores mcp-roundtrip CI job and chorus-hooks test isolation. Both are Silas-architecture-call work.
5. **#2496 + #2497** — quieter improvements; can wait until the structural cards above settle.
6. **Loom-decisions harvest of ADR-026** (#2485 follow-on) — once it lands, this design's "Decisions" dependency row goes from SHELL to POPULATED, and the layer model becomes graph-queryable.

## Not in scope for this design

- The Gathering app's own CI (lives in `jeff-bridwell-personal-site` repo). Smoke-check is its problem, per ADR-026 addendum.
- Production deploy automation — chorus runs natively on Library/Bedroom; CI gates merge, not deploy.
- Coverage thresholds and pyramid shape — that's the Quality service design's territory.
- Test-value policy — also Quality.
- Cost / runtime budget for CI minutes — not yet a concern at current run volume; will become one if mcp-roundtrip + Fuseki service container becomes per-PR standard.

## References

- ADR-026 — CI architecture + lock-file policy (`roles/silas/adr/ADR-026-ci-architecture-and-lock-file-policy.md`), including 2026-04-25 addendum (smoke-check stays out)
- Card #2481 — initial CI ratchet implementation (closed)
- Card #2487 — MCP round-trip CI scaffolding (in flight, PR #25 lean-scope)
- Card #2488 — api tsc moduleResolution (closed inline by #2487 attempt; reverted; superseded by #2495)
- Card #2491 — chorus-hooks/inject CI test isolation (Silas-owned)
- Card #2493 — sessions.test.ts pre-existing fail (Wren-filed)
- Card #2494 — this service design (in flight, gate:arch-pass from Silas)
- Cards #2495 / #2496 / #2497 / #2498 / #2499 / #2500 — gaps tracked above
- `proving/domains/alerts/ci-main-red.yml` — reactive alert
- Pipelines-domain page — `http://localhost:3000/gathering-docs/domain-detail.html?id=pipelines-domain`
- Quality service design (`designing/docs/quality-service-design.md`) — sibling layer (Layer 2 enforcement)
- Roles service design (`designing/docs/roles-service-design.md`) — template followed
