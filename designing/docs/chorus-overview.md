# Chorus — Team Coordination Product

**What it is:** An authored body of work — the operating system for a human+AI team, expressed as versioned, legible, auditable protocol. The protocol IS the product. The gathering-team repo is the Chorus Werk — not a repo that contains it.

**Named:** DEC-019 (2026-02-17). The name reflects coordinated voices, not synchronized — and pride in craftsmanship.

**Positioning (DEC-034):** Chorus is a Werk, not a tool. Three defining qualities: **versioned** (like code), **legible** (like prose), **auditable** (like infrastructure). Every artifact must pass all three. "Werk" from Heidegger — the work that discloses truth through its making.

**Patent lineage:** US9552400B2 (Bridwell, Staples 2012-2017). Same architectural pattern (RDF/OWL + SPARQL + approval gates) validated at enterprise scale.

---

## Product Vision (2026-02-20)

Chorus is the entire operating system — the nervous system that connects direction to delivery and back. The value stream is the spine. Every capability we build — hooks, scripts, dashboards, briefs, templates, gates — is a nerve that either carries a signal along that spine or enforces a rule at a junction.

Every touchpoint in the system either:
- **Senses** something (Slack message, commit, deploy, error)
- **Routes** it (to the right role, the right card, the right dashboard)
- **Constrains** it (can't deploy without tests, can't skip a brief, can't exec into containers)
- **Proves** it (screenshot, dashboard, audit trail, retrospective)

Session templates (demo, board walk, review, SWAT) aren't meeting formats — they're **interaction protocols that shape how signal flows through the spine.** Quality gates, pre-commit hooks, CI/CD, monitoring, alerting aren't a separate "ops" concern — they're nerves in the Proving stage that close the loop back to Directing.

**Design principle:** Lightweight constraints built into the flow, not process docs that rely on willpower. The rules live in the wiring. If a role can skip it, it's aspirational. If a hook enforces it, it's real.

### Nervous System Layers

| Layer | What | Examples |
|-------|------|----------|
| **Brain** | Where rules are defined | CLAUDE.md files, team-architecture.md, decisions.md, ADRs |
| **Spine** | Central routing — fires automatically | SessionStart hooks, UserPromptSubmit hooks, post-commit hook |
| **Peripheral nerves** | Block bad actions at the edge | sensitive-paths-hook, write-scrubber, infra-guardrails |
| **Shared memory** | Persistent context across sessions | ~/.chorus/index.db, state files, activity.md |
| **Sensory organs** | Observability — what happened? | chorus.log → Loki → Grafana (8 dashboards) |
| **Muscles** | Operational scripts — do things | app-state.sh, system-state.sh, cards, slack-post.sh |

### Value Stream Maturity (2026-02-20 snapshot)

| Stage | Enforced | Active | Aspirational | Missing |
|-------|----------|--------|-------------|---------|
| Directing | 4 | 2 | 2 | 3 |
| Designing | 3 | 2 | 3 | 2 |
| Building | 5 | 3 | 0 | 3 |
| Quality/Ops | 6 | 6 | 3 | 2 |
| Proving | 1 | 3 | 3 | 3 |
| Cross-cutting | 5 | 2 | 0 | 0 |

Building is strongest. Proving is weakest. Quality/Ops corrected 2026-02-20: CI/CD (GitHub Actions + E2E + SHACL + coverage gates), backup (backup-pods.sh + verify + cron) are enforced; alertmanager is active (needs Slack webhook URL). The gap between brain (aspirational) and spine (enforced) is where drift compounds into failure demand.

---

## Value Stream

```
Directing → Designing → Building → Proving → (loop)
```

Each stage has gates. Work items flow through the pipeline. Trust accrues as items pass gates successfully.

---

## Core Documents

| Document | Location | Owner | Description |
|----------|----------|-------|-------------|
| Team Architecture | [messages/team-architecture.md](../messages/team-architecture.md) | Silas | Operating model v1.1 — principles, communication, session lifecycle |
| Chorus Ontology v0.1.0 | [architect/briefs/2026-02-17-chorus-ontology-response.md](../architect/briefs/2026-02-17-chorus-ontology-response.md) | Silas | 6-layer pipeline model (pending chorus.ttl write-out) |
| Gate Registry | [architect/chorus/gate-registry.md](../architect/chorus/gate-registry.md) | Silas | 6 gates, 4 checklists, 5 fitness functions — what's enforced |
| Decisions Log | [product-manager/decisions.md](decisions.md) | Wren | DEC-001 through DEC-034 |
| Value Stream & Domains | [product-manager/value-stream-and-domains.md](value-stream-and-domains.md) | Wren | 8-stage value cycle, 3-layer domain map |

## Architecture & ADRs

| ADR | Location | Description |
|-----|----------|-------------|
| ADR-009 (pending) | — | Chorus pipeline ontology formalization |
| ADR-010 | [architect/adr/ADR-010-generalized-harvest-pipeline.md](../architect/adr/ADR-010-generalized-harvest-pipeline.md) | Harvest pipeline + quality gates (Chorus Build Gate applied) |
| ADR-011 | [architect/adr/ADR-011-production-like-deployment-pattern.md](../architect/adr/ADR-011-production-like-deployment-pattern.md) | Atomic deploys, health gates, rollback |

## Operational Artifacts

| Artifact | Location | Description |
|----------|----------|-------------|
| Chorus Audit Runner | [messages/scripts/chorus-audit.sh](../../scripts/chorus-audit.sh) | Session start/close checks, gate compliance, disk health |
| Board Client | [messages/board-client/](../messages/board-client/) | TypeScript board client — both boards, typed API, card-first audit (DEC-033) |
| Infra Guardrails Hook | [engineer/.claude/hooks/infra-guardrails.sh](../engineer/.claude/hooks/infra-guardrails.sh) | Platform-enforced command blocking (G1) |
| Structured Logs | [messages/logs/chorus.log](../messages/logs/chorus.log) | JSON audit trail → Promtail → Loki |

## Briefs & Analysis

| Brief | Location | Description |
|-------|----------|-------------|
| Chorus Value Stream | [architect/briefs/2026-02-17-chorus-value-stream-ontology.md](../architect/briefs/2026-02-17-chorus-value-stream-ontology.md) | Wren's value stream proposal |
| Chorus Ontology Approval | [architect/briefs/2026-02-18-chorus-ontology-approval.md](../architect/briefs/2026-02-18-chorus-ontology-approval.md) | Wren's approval + 4 refinements |
| Patent Prior Art | [architect/briefs/prior-art-bridwell-patent-US9552400B2.md](../architect/briefs/prior-art-bridwell-patent-US9552400B2.md) | 39 claims, enterprise validation |
| Building Product Brief | [product-manager/briefs/2026-02-17-building-product-brief.md](../product-manager/briefs/2026-02-17-building-product-brief.md) | Silas's original product framing |
| A2A Spike | [architect/briefs/spike-a2a-agent-communication.md](../architect/briefs/spike-a2a-agent-communication.md) | 8 frameworks evaluated, our protocol is sound |

## Team Roles

| Role | Location | Person | Focus |
|------|----------|--------|-------|
| Product Manager | [product-manager/](.) | Wren | What + why + when |
| Architect | [architect/](../architect/) | Silas | How + constraints + operations |
| Engineer | [engineer/](../engineer/) | Kade | Build + test + ship |
| Director | — | Jeff | Vision + direction + decisions |

## Board

Run `cards --chorus list` or visit http://localhost:3456 (Project: Chorus).

## Logs & Observability

- **Structured logs:** `messages/logs/chorus.log` (JSON, queryable by role/component/level)
- **Loki:** Scraped by Promtail, queryable in Grafana at http://localhost:3100
- **Query example:** `{appName="chorus-audit"} |= "role" | json`
- **Dashboard:** (pending — Chorus board card #8)

---

*Last updated: 2026-02-20*
