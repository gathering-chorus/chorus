# CI Pipeline — Service Design

**Kade, 2026-04-25 / refreshed 2026-04-29 post cost-stop. Source: `.github/workflows/quality.yml`, `platform/hooks/{pre-commit,pre-push}` (Layer 1), `platform/scripts/{git-queue.sh,werk}` (Layer 1 substrate, #2580/#2598), `/gate-*` skills (Layer 2), ADR-026 (CI architecture + lock-file policy), DEC-2525 (required-checks governance + amendment), `proving/domains/alerts/ci-main-red.yml` (red-main alert), today's session (#2580 / #2586 / #2597 / #2598 / #2600).**

## Promise

Every change that lands on `main` carries a quality bar that's been measured by the same gates regardless of which role authored it. Those gates today are: pre-commit hooks running tsc/jest/cargo per affected package; the queue-layer branch-check (#2580) refusing cross-role contamination; the pre-push hook (#2598) refusing raw `git push` and wrong-role-prefix branches; the chorus-hook-shim PreToolUse extension refusing raw `git push`/`rebase`/`cherry-pick`/`reset --hard`; the role-gate chain (#1762 family) recording AC-pass on the card. CI itself (this service) is post-merge: a scheduled GitHub Actions workflow that re-runs the suite on `main` once a day and alerts Silas if it goes red. When CI is healthy, Jeff can ask "is `main` shippable?" and the local layers + once-a-day re-run answer yes by construction. When CI drifts, locks fall out of sync, ratchet baselines accumulate silent debt, the schedule fires red and nobody notices, or the local-layer substrate develops gaps that CI's once-a-day pass can't catch fast enough.

## Overview

CI is one piece of a multi-layer quality system. Per ADR-026 §a + Jeff's 2026-04-29 reframe, redundancy across layers is intentional — different threat models, not duplication. The layers shifted today (cost-stop, #2600). What was a 12-job per-PR matrix is now a once-daily scheduled re-run with `0` required-status-checks; per-PR validation has been pushed left into Layer 1's substrate.

| Layer | Today | Threat closed | Skippable? |
|---|---|---|---|
| 0 — Substrate (werk + git-queue + pre-push + PreToolUse) | #2580 + #2597 + #2598 | Cross-role contamination, raw-git bypasses, deploy-from-non-main, build-from-stale | No (refused at four threat surfaces) |
| 1 — Pre-commit | `platform/hooks/pre-commit`, Rust-implemented via chorus-hook-shim | Honest mistakes pre-commit; lint, oversize, principle-edit, hermetic Rust slice (`cargo test --lib --bins`), doc-coherence | Yes via `--no-verify` (CI was authoritative; now schedule-only catches drift) |
| 2 — Role gates | `/gate-product` `/gate-code` `/gate-quality` `/gate-arch` `/gate-ops` skills | Card-acceptance bar — AC, build, hooks, structural fit, ops health | Skippable for `type:chore`/`type:swat` cards |
| 3 — CI (this) | `.github/workflows/quality.yml` schedule-only after #2600 | Local-vs-CI environment divergence; lock-honored-as-CI-runs-them | Not skippable for the schedule fire; merge no longer gated by CI |

The pipeline today is a single GitHub Actions workflow at `.github/workflows/quality.yml`. Pre-#2600 it ran on push-to-main and pull-request-to-main with 13 required jobs. Post-#2600 (PR #49 merged 2026-04-29 20:33Z) it runs on schedule (~daily 09:17 UTC). Branch-protection + Repository Ruleset (15547153) `required_status_checks` are now **empty** — merge is no longer gated by CI verdict. Local layers + the role-gate chain are the per-PR quality bar; the schedule fire is the once-a-day post-merge witness.

| Component | Status | Source | Gap |
|-----------|--------|--------|-----|
| Workflow definition | REAL — schedule-only | `.github/workflows/quality.yml`, ~daily 09:17 UTC; matrix retained but unfired by push/PR | Schedule fire alerts on red, but post-merge regressions discoverable only at next fire (≤24h delay) |
| Layer-0 substrate (#2580 + #2597 + #2598 + #2599) | REAL — landed today | `git-queue.sh` branch-check, `werk` wrapper, `pre-push` hook, chorus-hook-shim PreToolUse refusal extension | werk currently covers chorus-hook-shim/claudemd-gen/install-hooks; service deploys (chorus-api/clearing/pulse) still via app-state.sh — phase-2 candidate |
| Pre-commit (Layer 1) | REAL | `platform/hooks/pre-commit` runs lint-ratchet + principle-edit + catalog-oversize + cargo --lib --bins + doc-coherence | KNOWN_FAILS pattern uses `--no-verify` w/ trace; not enforced in tooling (#2497) |
| Role gates (Layer 2) | REAL | `/gate-product` `/gate-code` `/gate-quality` `/gate-arch` `/gate-ops` skills | Demo skill assumes user-facing; infra cards adapt via #2499 |
| Branch protection | EMPTIED | classic protection on `main` retained for non-status rules; `required_status_checks` now empty post-#2600 | Without required checks, an accidental main push can land red-on-main and only schedule-fire catches it |
| Repository Ruleset 15547153 | EMPTIED | mirrored required-checks (DEC-2525 amendment) — also emptied post-#2600 | Lockstep contract still intact; just both sides empty for now |
| Lock-file policy | REAL | per-package `package-lock.json` committed, `npm ci` in CI, Dependabot weekly Mon → reduced to monthly + 1 PR/ecosystem post-#2600 | Drift between local + scheduled-CI runs invisible until next schedule fire |
| Reactive alert | REAL | `proving/domains/alerts/ci-main-red.yml` polls Actions API every 10 min; daily cooldown | Polls every 10 min but with schedule firing only ~daily, alert fires at most once/day on red |
| ESLint ratchet | REAL | `npm run lint:ratchet` enforces per-rule baseline; runs in pre-commit + scheduled CI | Baseline drift between branches silent (#2496) |
| Required-checks ↔ workflow coupling | RESOLVED VIA EMPTYING | After #2600, both branch-protection and Ruleset 15547153 carry empty required-checks lists | When required-checks come back (if ever), drift detector #2500 still applicable |
| Smoke-check (app surface) | OUT OF SCOPE | Lives in `jeff-bridwell-personal-site` repo's own CI | ADR-026 addendum 2026-04-25 |
| MCP round-trip (live api) | DONE | #2495 closed; CI job restored | — |
| Test-isolation in chorus-hooks/inject | PARTIAL | cargo-test green via CHORUS_ROOT env override | #2491 still Later — hardcoded `/Users/jeffbridwell` paths in some tests |

## Sub-Domain Interaction Model

CI now has a smaller surface than before. It doesn't act on push or PR (those events used to be the primary triggers); it fires on schedule and produces a verdict that alerts Silas if red. The per-PR threat model has moved to Layer 0 (substrate) + Layer 1 (pre-commit). The trigger / produces / consumed by table reflects post-cost-stop state.

| Trigger | Produces | Consumed By | Surface |
|---------|----------|-------------|---------|
| **Schedule fire (~daily 09:17 UTC)** | One workflow run, full matrix | `ci-main-red.yml` alert poll | `https://github.com/gathering-chorus/chorus/actions` |
| Push to `main` | (no workflow run today) | — | n/a (workflow trigger removed in #2600) |
| Pull request | (no workflow run today) | Branch protection no longer gates on CI verdict | n/a |
| Workflow run completes red on `main` | `red:<run-url>` line in alert check stdout | `proving/domains/alerts/ci-main-red.yml` action — nudges Silas, daily cooldown | `nudge silas` |
| Layer-0 substrate refusal (commit / push / git op) | spine event `commits.branch.mismatch_detected` or hook-deny output | Drift dashboards (Silas), incident reconstruction | chorus.log |
| Lock-file out of date | Dependabot opens monthly PR (post-#2600) | Role review queue, then merge → next schedule re-run validates against new lock | `gh pr list --label deps` |
| Ratchet baseline shrinkable | (today) silent — no signal | (should feed) #2496 ratchet-drift signal | **NOT WIRED — #2496** |
| Test fails but is upstream-known | --no-verify with #NNNN trace in commit message | (today) commit-message convention only; no machine-checkable allowlist | **NOT WIRED — #2497** |

The core pattern: CI is a **periodic verdict producer**, layer-0 substrate is the **per-PR enforcer**, and alerts are the **safety net**. Verdicts at scheduled cadence catch what the substrate missed; substrate refusal catches what slipped at commit-time before pre-commit even runs. The safety net's window grew (≤24h vs ≤immediate before) so the load-bearing requirement is that Layer 0 + Layer 1 catch the obvious-broken case first.

## Dependencies

Per Wren's 4-layer pattern (#2159): CI sits on principles + practices + policies + decisions.

| Dependency | Sub-domain | Status | Instances | Notes |
|-----------|------------|--------|-----------|-------|
| Principles | loom-principles | POPULATED | 5 CI-relevant | `quality-at-source` (CI catches what local missed), `tests-hermetic-by-default-integration-gated-explicitly` (hermetic-by-default; integration gated), `infrastructure-is-your-codebase-too` (workflow + hooks reviewed like code), `parallel-paths-not-fallback-chains` (Layer 0/1/2/3 are parallel paths at different threat surfaces), `every-ceremony-must-yield` (cost-stop applied this — per-PR matrix didn't yield enough) |
| Practices | loom-practices | POPULATED | 2 CI-relevant | `production-ready` (CI = "does this build clean?"), `api-first` (workflow YAML is the API to enforcement) |
| Policies | loom-policies | MISSING | — | Sub-domain blocked on #2151. CI will depend on: KNOWN_FAILS allowlist policy (#2497), ratchet drift policy (#2496), red-main escalation policy (runbook section), required-checks coupling policy (#2500), schedule-cadence policy (post-#2600 — what fire frequency justifies vs schedule-only re-run cost) |
| Decisions | loom-decisions | SHELL | 0 populated | ADR-026 (CI architecture + lock-file policy), its 2026-04-25 addendum (smoke-check stays out), DEC-2525 (required-checks governance + amendment), and #2600 (schedule-only retreat) all CI-load-bearing but not yet harvested as graph instances; #2152 covers harvest |

**Why dependencies matter to CI:** the layer model is a shape derived from principles, practices, policies, and decisions. When principles shift (e.g., "every-ceremony-must-yield" applied as cost-stop justification on 2026-04-29), the layer shape shifts too — that's exactly what #2600 was. Without policies populated, the layer shape can drift faster than the doc that describes it.

## Components

### Layer 0 — Substrate (NEW post 2026-04-29)
Substrate refusal at four threat surfaces. None of them are CI per se; they are the per-PR enforcement that replaced the per-PR matrix.

- **`git-queue.sh` branch-check** (#2580, Done) — refuses commit/push if HEAD doesn't match `<DEPLOY_ROLE>/*`. Spine event `commits.branch.mismatch_detected` with cwd + commits_ahead.
- **`werk` wrapper** (#2598, in flight, PR #56) — `werk check` (read-only drift report), `werk deploy` (refuses unless HEAD == origin/main, composes cargo build + claudemd-gen + install-hooks), `werk deploy --dev` (sandbox path).
- **Pre-push hook** (#2598) — `platform/hooks/pre-push` refuses raw `git push` (no `_GIT_QUEUE_PUSH` marker) or wrong-role-prefix branch; honors `DEPLOY_ROLE_PREPUSH_OVERRIDE=1`.
- **chorus-hook-shim PreToolUse refusal extension** (#2598) — refuses raw `git push`/`rebase`/`cherry-pick`/`reset --hard` from Claude's Bash tool (matches existing `git commit`/`add` block).
- **#2597 git-queue silent-exit fix** (Done) — closed the bypass surface where the queue silently exited and trained roles to use raw `git push`.

### Workflow definition (`.github/workflows/quality.yml`)
Single workflow, matrix retained, **trigger changed to schedule-only post-#2600**. ~daily 09:17 UTC fire. Each job is one concern: lint-ratchet, MCP shape, catalog-oversize, principle-direct-edit, tsc matrix, jest matrix, cargo-test matrix. Adding a new job is mechanical (recipe in this doc) and now decoupled from required-checks (which are empty).

### Pre-commit hook (`platform/hooks/pre-commit`)
Layer 1. Rust-implemented via `chorus-hook-shim`. Runs lint-ratchet, principle-direct-edit, catalog-oversize, `cargo test --lib --bins` per affected Rust crate, doc-coherence ratchet (when CLAUDE.md fragments change). Skippable via `--no-verify` — explicitly overridden by Layer 3 schedule fire as authoritative-but-delayed. KNOWN_FAILS pattern (carded test failures from other roles) currently goes through `--no-verify` with #NNNN reference; #2497 wires the allowlist.

### Role gates (Layer 2)
Five skills, one per concern: `/gate-product` (Wren — AC, Experience, domain), `/gate-code` (Kade — tests green, build clean, warning diff), `/gate-quality` (Kade — hooks, regression, console.log, smoke when local), `/gate-arch` (Silas — namespace, boundaries, structural fit), `/gate-ops` (Silas — health, log flow, rollback). Recorded as card comments + spine events. Skippable for `type:chore`/`type:swat`.

### Branch protection + Repository Ruleset (DEC-2525 amendment)
Both surfaces still exist; both `required_status_checks` lists were emptied post-#2600. The lockstep contract from DEC-2525 amendment ("required-checks list lives in two GitHub protection systems") is intact — both sides match (both empty). When required-checks come back (if ever), DEC-2525 amendment governs the coupling and #2500 drift detector closes the manual loop.

### Lock-file policy + Dependabot
ADR-026 §c. Per-package `package-lock.json` committed, root + Cargo locks committed. Pre-#2600: Dependabot weekly Mon, 11 ecosystem-directory pairs. Post-#2600: monthly + 1 PR/ecosystem (cost discipline applied to dependency PRs since each one used to fire the matrix). Schedule fire validates against current locks once/day.

### Reactive alert (`ci-main-red.yml`)
Polls GitHub Actions API every 10 minutes. With schedule-only firing, the alert effectively wakes once/day on red incidents. Daily cooldown unchanged. Read-only check; never auto-reverts (ADR-026 §d).

### ESLint ratchet (`npm run lint:ratchet`)
Per-rule baseline at `platform/state/eslint-baseline.json`. Pre-commit + scheduled CI both run the same check. New rule firings require fix or `npm run lint:baseline` to adopt. Drift signal #2496 still Later.

### Red-main runbook (Silas-owned)
Triggered by `ci-main-red` schedule fire. Triage: (1) recent merge + unrelated to in-flight → revert merge, file fix card; (2) forward-fix quick (≤10 min) → push fix direct to main, await next schedule fire green; (3) else → file swat card, mark known-red in #shared-observability, decide team-wide. Daily cooldown means triage runs once per incident.

## Surfaces

- **GitHub Actions UI** — `https://github.com/gathering-chorus/chorus/actions` — primary read surface, history, log streaming.
- **PR Checks tab** — empty post-#2600 (no per-PR matrix).
- **Branch protection settings page** — `https://github.com/gathering-chorus/chorus/settings/branches` — non-status rules retained, status checks empty.
- **Repository Rulesets page** — `https://github.com/gathering-chorus/chorus/rules` — Ruleset 15547153 paired with branch-protection (DEC-2525 amendment), status checks empty.
- **Dependabot PRs** — `gh pr list --label deps` — monthly cadence post-#2600.
- **`ci-main-red` nudge** — Silas's terminal + messaging API, daily cooldown.
- **chorus.log spine events** — `commits.branch.mismatch_detected`, `werk.deploy.*`, hook-deny events — post-#2600 the substrate spine is the per-PR signal surface.
- **`werk check`** — local drift visibility for any role; closes the "is the runtime in sync with main" question without round-tripping CI.
- **Pipelines-domain page** — `http://localhost:3000/gathering-docs/domain-detail.html?id=pipelines-domain`.

## Consumers

- **Branch protection** — no longer consumes CI verdict for merge gating (post-#2600); still enforces non-status rules.
- **`ci-main-red.yml` alert** — consumes verdict on `main` post-schedule-fire, routes red to Silas.
- **Roles (PR authors)** — consume their PR's local-layer verdicts (pre-commit + role gates); CI verdict no longer pre-merge.
- **Dependabot** — produces input (lock-update PRs); consumes output (next schedule fire validates).
- **`/gate-code` skill** — consumes pre-commit + werk check state to decide gate verdict.
- **`/demo` skill** — consumes the gate-chain state; #2499 still pending for infra-card adapter.
- **`werk` wrapper** — consumes `git rev-parse origin/main` for SHA gate; emits `werk.deploy.*` spine events.

## Gaps

**Done since 2026-04-25 refresh:**
- ~~#2495 — api-boot ESM/CJS reconciliation~~ — Done
- ~~#2487 — MCP round-trip CI scaffolding~~ — Done
- ~~#2498 — Trunk-vs-Rulesets cleanup~~ — Done
- ~~#2485 — loom-decisions reach~~ — Done
- ~~#2580 — git-queue branch-check~~ — Done (today)
- ~~#2597 — git-queue silent-exit fix~~ — Done (today)
- ~~#2600 — CI cost-stop swat~~ — Done (today)

**In flight:**
- **#2598** — werk wrapper + pre-push + raw-git refusal extension. PR #56 open, gates passed product/code/quality, awaiting Silas's gate:arch + gate:ops.

**Open (Later):**
- **#2496** — Ratchet baseline drift signal. Schedule-only CI still uses ratchet; drift surface still missing.
- **#2497** — KNOWN_FAILS allowlist. `--no-verify` convention still ungated.
- **#2499** — Demo skill adapts for infra cards. Infra demo = prove threat closes, not "show CI green."
- **#2500** — Required-checks drift detector. Less urgent post-#2600 (both sides empty), but applies when required-checks come back.
- **#2491** — chorus-hooks/inject test isolation (Silas-owned). Hardcoded `/Users/jeffbridwell` paths in tests.
- **#2493** — sessions.test.ts pre-existing fail (Wren-filed).
- **#2589** — git-spawn env-scrub helper sweep (Kade, follow-on to #2560).
- **#2599** — chorus-env-setup.sh self-locating sweep (Silas, follow-on to #2571).

**Silas's CI architecture line (Later):**
- **#2527** — Nightly drift lane. Slow signals (mutation/perf/integration/CVE/doc-coherence) file cards on red, never block. **Promoted in importance post-#2600** — without per-PR matrix, the drift lane is the primary backstop for slow signals.
- **#2528** — Sensor consumer registry. Every CI check / alert / drift signal mapped to named consumer. Load-bearing once #2527 lands.
- **#2529** — run-tests contract spec. Load-bearing interface between CI substrate and tests substrate.
- **#2530** — Flake quarantine mechanism. Less urgent post-#2600 (no per-PR matrix to flake on).
- **#2200** — Cross-language contract tests TS↔Rust divergence observability.
- **#2333** — Post-restart smoke for /api/flow sequences[].
- **#2591** — GitHub Actions org-billing alert + auto-raise — guardrail against recurring quota wedge.

## Next Steps

1. **#2598 lands** (in flight) — closes Layer 0 substrate; pre-push hook + raw-git refusal extension activate post-merge + werk-deploy.
2. **#2527 + #2528 + #2529** in that order (Silas) — post-cost-stop architecture; nightly drift lane is now the primary backstop, sensor registry maps signals to consumers, run-tests contract is the load-bearing interface.
3. **#2591** (Silas) — billing alert is independent of #2527/#2528/#2529; can ship in parallel.
4. **#2496 + #2497** — quieter improvements; can wait until structural cards above settle.
5. **Loom-decisions harvest of ADR-026 + DEC-2525 + #2600** (#2152 follow-on) — Decisions row goes from SHELL to POPULATED; layer model becomes graph-queryable.
6. **Required-checks reinstatement** (no card) — when local-layer maturity warrants it (or schedule-only proves insufficient), reinstating required-checks via DEC-2525-amendment-governed lockstep is a one-PR move.

## Not in scope for this design

- The Gathering app's own CI (lives in `jeff-bridwell-personal-site` repo). Smoke-check is its problem, per ADR-026 addendum.
- Production deploy automation — chorus runs natively on Library/Bedroom; werk handles canonical deploys, not CI.
- Coverage thresholds and pyramid shape — Quality service design's territory.
- Test-value policy — also Quality.
- Cost / runtime budget for CI minutes — addressed today by #2600 (schedule-only). Budget discipline becomes principle-level in 4-layer dependency once policies sub-domain stands up.

## References

- ADR-026 — CI architecture + lock-file policy (`roles/silas/adr/ADR-026-ci-architecture-and-lock-file-policy.md`), including 2026-04-25 addendum (smoke-check stays out)
- DEC-2525 — required-checks governance, including 2026-04-29 amendment (lives in two GitHub protection systems: branch-protection + Repository Rulesets)
- Card #2481 — initial CI ratchet implementation (closed)
- Card #2487 — MCP round-trip CI scaffolding (closed)
- Card #2495 — api-boot ESM/CJS (closed)
- Card #2498 — Trunk-vs-Rulesets cleanup (closed; replaced with DEC-2525 amendment)
- Card #2580 — git-queue branch-check (Done 2026-04-29)
- Card #2586 — commits-domain service design (Done 2026-04-29)
- Card #2597 — git-queue silent-exit fix (Done 2026-04-29)
- Card #2598 — werk wrapper + pre-push hook + raw-git refusal extension (in flight, PR #56)
- Card #2600 — CI cost-stop swat (Done 2026-04-29; PR #49)
- Cards #2496 / #2497 / #2499 / #2500 / #2491 / #2493 / #2589 / #2599 — gaps tracked above
- Cards #2200 / #2333 / #2527 / #2528 / #2529 / #2530 / #2591 — Silas's CI architecture line
- `proving/domains/alerts/ci-main-red.yml` — reactive alert
- Pipelines-domain page — `http://localhost:3000/gathering-docs/domain-detail.html?id=pipelines-domain`
- Quality service design (`designing/docs/quality-service-design.md`) — sibling layer (Layer 2 enforcement)
- Roles service design (`designing/docs/roles-service-design.md`) — template followed
- Commits service design (`designing/docs/commits-service-design.md`) — Layer 0 substrate sibling
