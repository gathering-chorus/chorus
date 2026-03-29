# Chorus Gate Registry

Last updated: 2026-02-18
Version: 0.1.0

The single source of truth for what's enforced, what's auditable, and what's missing.

**209 rules documented across the system. This registry tracks their enforcement status.**

---

## Enforcement Tiers

| Tier | Mechanism | When | Can bypass? | Example |
|------|-----------|------|-------------|---------|
| **Gate** | Blocks execution | Before action | No | PreToolUse hook, pre-commit, CI |
| **Checklist** | Verifies at boundary | Session start/close | Surfaced, not blocked | Did you pull? Did you post standup? |
| **Fitness** | Measures over time | After the fact | N/A (reporting) | % sessions with standup, state file staleness |
| **Doc-only** | Written in CLAUDE.md | Never | Yes (honor system) | "Come with a point of view" |

---

## Gate Inventory

### G1: Infrastructure Commands (ACTIVE — Hook)
- **Mechanism**: PreToolUse hook at `engineer/.claude/hooks/infra-guardrails.sh`
- **Scope**: Kade only
- **Rules enforced**: 10 (docker exec, docker logs, kill, docker stop/rm/restart/kill, docker compose down, docker run, terraform apply/destroy)
- **Status**: Active since 2026-02-18
- **Audit**: Hook blocks automatically. No bypass possible.

### G2: Pre-Commit Quality (ACTIVE — Husky)
- **Mechanism**: `npm run precommit` via Husky git hook
- **Scope**: Anyone committing to jeff-bridwell-personal-site
- **Rules enforced**: 7 (Trivy scan, ESLint, unit tests, integration tests, security tests, performance tests, coverage thresholds)
- **Status**: Active since 2026-02-14
- **Audit**: Blocks commit on failure. Can bypass with `--no-verify` (flagged as prohibited).

### G3: CI Pipeline (ACTIVE — GitHub Actions)
- **Mechanism**: GitHub Actions workflow on push/PR to main
- **Scope**: All pushes
- **Rules enforced**: 5 (Jest, Playwright E2E, ESLint, terraform validate, tsc build)
- **Status**: Active since 2026-02-13
- **Audit**: Blocks merge on failure.

### G4: Container Health (ACTIVE — Docker)
- **Mechanism**: HEALTHCHECK in Dockerfile + Terraform health checks
- **Scope**: Running containers
- **Rules enforced**: 4 (Express health, Fuseki health, non-root user, restart policy)
- **Status**: Partial — 5/16+ containers have health checks (personal-site Express + Fuseki, wordpress-blog MySQL + WordPress + MailHog). All WordPress containers have `restart: unless-stopped` + health checks (2026-02-18). ADR-011 plans full coverage for remaining containers.
- **Audit**: Docker reports unhealthy containers. `docker ps` shows health status.

### G5: Code Quality (ACTIVE — tsconfig + ESLint)
- **Mechanism**: TypeScript strict mode, ESLint rules, coverage thresholds
- **Scope**: All code
- **Rules enforced**: 12 (strict mode, coverage thresholds per module, complexity, length, nesting, security patterns, rate limits, headers)
- **Status**: Active
- **Audit**: Caught at pre-commit (G2) and CI (G3).

### G6: Dead Code (ACTIVE — knip + jscpd)
- **Mechanism**: knip, jscpd, tsconfig noUnusedLocals/Parameters
- **Scope**: All code
- **Rules enforced**: 4 (unused files/exports, duplication < 5%, unused locals, unused params)
- **Status**: Active since 2026-02-14
- **Audit**: Manual run. Not yet in pre-commit or CI.

---

## Checklist Inventory (Session Boundary)

### C1: Session Start (ALL ROLES)
- **Mechanism**: `chorus-audit.sh start` (run by SessionStart hook)
- **Status**: BUILDING (2026-02-18)
- **Checks**:
  - [ ] `git pull --rebase` executed
  - [ ] CLAUDE.md + team-architecture.md read
  - [ ] `briefs/` inbox checked
  - [ ] Slack channels read (#all-gathering + own channel)
  - [ ] activity.md scanned
  - [ ] board.sh list run
  - [ ] Previous session closed properly (standup posted, activity updated, changes pushed)

### C2: Session Close (ALL ROLES)
- **Mechanism**: `chorus-audit.sh close` (run by role on close-out)
- **Status**: BUILDING (2026-02-18)
- **Checks**:
  - [ ] activity.md updated with session actions
  - [ ] #standup post made (with cost)
  - [ ] cost-log.md updated
  - [ ] All changes committed and pushed
  - [ ] State files current (current-work.md / system-architecture.md / backlog.md)

### C3: Brief Protocol (ALL ROLES)
- **Mechanism**: Fitness function (periodic check)
- **Status**: PLANNED
- **Checks**:
  - [ ] Briefs in recipient's directory (not sender's)
  - [ ] Brief has required headers (From, To, Date, Priority)
  - [ ] Signal posted to Slack after brief written
  - [ ] Consumption logged in activity.md

### C4: Board Hygiene (ALL ROLES)
- **Mechanism**: Fitness function (periodic check)
- **Status**: PLANNED
- **Checks**:
  - [ ] No work without a card
  - [ ] In-progress items have owners
  - [ ] Blocked items have reasons
  - [ ] Completed work marked done

---

## Fitness Functions

### F1: Session Compliance Score
- **What**: % of sessions that followed start + close checklist
- **How**: Parse git log, Slack history, activity.md timestamps
- **Target**: > 90%
- **Status**: BUILDING

### F2: State File Freshness
- **What**: Time since last update of each role's state files
- **How**: `git log -1 --format=%ci <file>`
- **Target**: Updated same day as session activity
- **Status**: PLANNED

### F3: Standup Coverage
- **What**: % of sessions that posted to #standup
- **How**: Parse Slack #standup history, compare to session dates
- **Target**: 100%
- **Status**: BUILDING

### F4: Brief Response Cadence
- **What**: Average time between brief sent and response
- **How**: Compare brief file timestamps with response timestamps
- **Target**: P1 same session, P2 within 2 sessions
- **Status**: PLANNED

### F5: Cost Tracking Completeness
- **What**: % of sessions with cost logged in cost-log.md
- **How**: Compare session dates to cost-log entries
- **Target**: 100%
- **Status**: PLANNED

---

## Enforcement Gap (Priority Order)

Rules that cause repeated friction, ranked by impact:

| Rule | Current tier | Target tier | Impact |
|------|-------------|-------------|--------|
| Session close-out (standup, activity, push) | Doc-only | Checklist (C2) | **HIGH** — missed close-outs lose context |
| Session start (pull, read state, check briefs) | Doc-only | Checklist (C1) | **HIGH** — stale context causes wrong decisions |
| No work without a card | Doc-only | Fitness (F1) | **MEDIUM** — untracked work creates confusion |
| State file incremental updates | Doc-only | Fitness (F2) | **MEDIUM** — batched updates lose information |
| Commit message format | Doc-only | Gate (G2 extension) | **LOW** — cosmetic but useful for audit |
| Brief routing to correct directory | Doc-only | Checklist (C3) | **LOW** — rarely violated |

---

## Chorus Connection

This registry is the first concrete Chorus artifact applied to our own operations.

- **Each gate = a Chorus Gate** (from the 5-layer model)
- **Each fitness function = a Chorus FitnessFunction** (auditable health metric)
- **Trust score = aggregate of all fitness functions** (team operational health)
- **Patent lineage**: Approval gates (US9552400B2 claim 5) before execution

The infra hook (G1) was the proof-of-concept. Session boundary checklists (C1, C2) are the next enforcement layer. Fitness functions (F1-F5) provide the long-term audit trail.

---

-- Silas
