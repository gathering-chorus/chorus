# Dogfooding Prep: Operating Model Reorg

**Date**: 2026-02-19
**Author**: Silas
**Audience**: Jeff (and all roles tomorrow)

---

## What's Live for Dogfooding

### Data Classification Enforcement
- **PreToolUse hook** on Read — wired in all 3 role settings, tested
- **Three `.sensitive-paths` manifests** — architect, engineer, product-manager
- **Policy doc** — `product-manager/data-classification-policy.md` (Wren)
- **Private files hard-blocked**: `.env`, `terraform.tfstate`, `.ssh/`, `stories.md`
- **Internal files prompt**: network-inventory, docker-compose, prometheus configs, infrastructure-constraints, service-manifest, cost-log
- **Public files pass through**: ADRs, ontology, code, briefs, decisions
- All events logged to `chorus.log` → Loki

### Boundary Checking
- **Session-start scan**: `chorus-audit.sh start <role>` checks last 48h for `[boundary]` and `[infra]` commits
- **G5 gate**: full audit verifies hook executable, all 3 manifests present, policy doc exists
- **Boundary contract**: `architect/boundary-contract.md` — Silas's dependency declarations
- **ADR-013**: full architectural spec for the operating model

### Team Coordination
- **Brief protocol**: write to recipient's `briefs/`, signal in Slack
- **Session lifecycle**: Synchronize → Operate → Close (all roles)
- **Communication taxonomy**: Brief/Signal/Question/Record
- **Board scripts**: `board.sh` (Gathering) + `chorus-board.sh` (Chorus)
- **Slack channels**: #all-gathering, #silas, #wren, #kade, #standup

---

## What's NOT Live Yet

| Item | Owner | Status |
|------|-------|--------|
| Bridge context scrubbing (Slack → API) | Kade | Briefed, not built |
| Memory write scrubbing (activity.md, MEMORY.md) | Kade | Briefed, not built |
| Wren's `.boundaries.yml` (full manifest with dependencies) | Wren | Has `.sensitive-paths`, not full format |
| Kade's `.boundaries.yml` | Kade | Has `.sensitive-paths`, not full format |
| `[boundary]` commit tag convention | All | ADR-013 defines it, only Silas has used it |
| `[infra]` commit tag convention | All | ADR-013 defines it, not yet used |

---

## Dogfooding Test Scenarios

### 1. Data Classification (can test immediately)
- Open a Silas session → try to read `architect/network-inventory.md` → should prompt "Internal"
- Open a Kade session → try to read `jeff-bridwell-personal-site/.env` → should hard block "Private"
- Open any session → read an ADR → should pass silently
- Check `chorus.log` for logged events

### 2. Session Start Boundary Check (can test immediately)
- Start any role → `chorus-audit.sh start <role>` runs automatically
- Should show "2 [boundary] commits in last 48h" (from today's work)
- Should show "Sensitive paths hook wired" + "manifest exists"

### 3. Boundary Change Flow (needs a live change)
- Silas modifies a file Kade depends on (e.g., prometheus.yml)
- Commits with `[boundary]` tag
- On Kade's next session start, the boundary check reports the change
- Kade verifies his dashboards still work

### 4. Brief Protocol Round-Trip
- Wren writes a brief to `engineer/briefs/`
- Signals in #all-gathering
- Kade reads the brief on session start
- Kade responds (brief or Slack)

### 5. Three-Role Coordination
- Jeff gives direction in #all-gathering
- Each role picks up the relevant piece
- Roles coordinate via briefs and signals without Jeff relaying

---

## Board State (Clean)

**Gathering board**: 1 Now, 5 Next, 18 Later, 45 Done
**Chorus board**: 1 Now, 1 Next, 3 Later, 11 Done

**Silas open items**:
- #70 Cross-domain search (Later)
- #59 Ontology walkthrough with Jeff (Later)
- #50 Cloud readiness assessment (Later)
- C#5 Elevate building/ (Later)
- C#4 Building protocol ontology (Later)

---

## Prep Checklist for Tomorrow Morning

- [ ] All 3 roles run `chorus-audit.sh start <role>` — verify clean start
- [ ] Jeff picks a scenario from above to walk through
- [ ] Each role verifies their `.sensitive-paths` manifest covers the right files
- [ ] Test one boundary change end-to-end (commit → signal → session-start detection)

---

— Silas
