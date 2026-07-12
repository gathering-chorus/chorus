# Chorus — Product Decomposition

**Owner**: Wren | **Date**: 2026-02-23 | **Card**: C#57
**Purpose**: Organize the full body of Chorus work into functional areas that can be decomposed into implementation, refactor, or rewrite chunks.

---

## What Chorus Is

The operating system for a human+AI team. Three artifacts tell the story:
- **System topology** (`/chorus/system`) — the interactive map of what exists
- **Spine architecture** (`product-manager/chorus-spine.html`) — how the three thirds work
- **Card lifecycle** (`designing/docs/werk-value-stream-design.html`; werk-process retired #3421) — how a piece of work flows

---

## Functional Decomposition

### 1. THE SPINE — Value Stream Flow

How work moves from idea to done. The central nervous system.

| Component | State | Action | Card |
|-----------|-------|--------|------|
| **Value stream stages** (Capturing→Directing→Designing→Building→Proving) | Shipped — visible on /value page | — | #233 done |
| **Card as value carrier** | Shipped — card-centric /value page with W/C/L times | — | #233 done |
| **Card quality by stage** | Not started — no definition of what "ready" means at each vertebra | **Implement** | C#57 |
| **Workflow engine** (workflow.sh, manifests) | Shipped — 41 workflows executed | Refactor: manifest schema versioning, step typing | — |
| **Piece flow** (card→Now auto-creates manifest) | Shipped (C#52) | — | — |
| **Brief routing** (auto-handoff between roles) | Shipped — briefs directory per role | Refactor: brief acknowledgment tracking | — |
| **Spine visualization** | Shipped — in /tmp, persisted to product-manager/ | **Implement**: host on Chorus system page | C#55 |

### 2. THE GATES — Quality Enforcement

What blocks or allows transitions between stages. Silas's audit: 209 rules, only 18 machine-enforced.

| Component | State | Action | Card |
|-----------|-------|--------|------|
| **Card-first gate** (no work without a card) | Shipped (C#44) — session audits enforce | — | — |
| **Manifest-first gate** (no build without manifest) | Building — auto-trigger on Now, not on add | Finish implementation | #222 |
| **Session init gate** (must read context before writing) | Shipped — PreToolUse hook | — | — |
| **Write scrubber** (no credentials in shared files) | Shipped — PreToolUse hook | — | — |
| **Infra guardrails** (no dangerous commands) | Shipped — but only on Kade | **Refactor**: deploy to all roles | C#56 |
| **Pre-push test suite** | Shipped — but harmful (2300 tests, 1-2min, corrupts sessions.db) | **Rewrite**: slim to lint + critical tests only | C#56 |
| **Pre-commit hooks** (app repo) | Shipped — Trivy, lint, tests, TTL | — | — |
| **Pre-commit hooks** (team repo) | Shipped locally — not portable (.git/hooks/) | **Refactor**: core.hooksPath or shared dir | C#56 |
| **Gate parity across roles** | Broken — hooks differ per role, no single manifest | **Implement**: single hook manifest | C#56 |
| **191 doc-only rules** (CLAUDE.md text) | Active — honor system | **Implement**: identify ~70 enforceable, prioritize | C#56 |

### 3. THE INSTRUMENTS — Observability

How Jeff and the team see what's happening. The sensory organs.

| Component | State | Action | Card |
|-----------|-------|--------|------|
| **Value Stream page** (/value) | Shipped — card-centric, 3 tabs, time badges | — | #233 done |
| **Cost dashboard** (Grafana) | Shipped (C#51) — per-role burn rate, 15 panels | — | — |
| **Chorus Activity dashboard** (Grafana) | Shipped — events, board events, git commits | — | — |
| **Home Cloud dashboard** (Grafana) | Shipped — container health, disk, network | — | — |
| **Sitemap scraper** (visual gemba walk) | Shipped — Playwright screenshots, issue detection | — | — |
| **Defect escape hatch** (log polling → auto-tickets) | Shipped — Loki queries surface errors | — | — |
| **macOS native alerts** | Shipped (#202) — Alertmanager → osascript | — | — |
| **Build health lava lamp** | Not started — deploy duration, success/fail, error rate | **Implement** | #138 |
| **Cognitive load instrumentation** | Not started — measure load on each participant | **Implement** | C#48 |
| **Spine health dashboard** | Not started — which gates are passing/failing across roles | **Implement** | C#56 |

### 4. THE INTERACTION LAYER — Communication

How humans and agents talk to each other. Wren's vertical (DEC-030).

| Component | State | Action | Card |
|-----------|-------|--------|------|
| **The Clearing** (multi-party chat) | Shipped (C#16) — browser-based, Haiku, decision capture | — | — |
| **Clearing voice quality** | Poor — Haiku roles sound generic, no /chorus context | **Rewrite**: 5 fixes for voice tuning | C#37 |
| **Clearing mobile access** | Not started — iPhone via LAN | **Implement** | C#36 |
| **Session templates** (demo, review, SWAT, standup) | Designed — DEC-032 demo protocol | **Implement**: template selection in Clearing | — |
| **Brief system** (async handoffs) | Shipped — filesystem-based, auto-routed by workflow engine | Refactor: structured format, acknowledgment | — |
| **/look** (sensory bridge) | Shipped — screen/chrome/terminal capture | — | — |
| **/werk** (workflow visibility) | Shipped — list, status, create, advance, visualize | — | — |
| **/chorus** (shared memory search) | Shipped — 14,700+ messages indexed | — | — |
| **Autonomous role activation** (mid-session attention) | Not started — "the nudge" pattern | **Implement** (needs architectural change) | C#15 |

### 5. THE MEMORY — Shared Context

How knowledge persists across sessions. The hippocampus.

| Component | State | Action | Card |
|-----------|-------|--------|------|
| **Chorus index** (SQLite FTS5) | Shipped — 14,700+ messages, full-text search | — | — |
| **Ambient session indexing** (fswatch daemon) | Shipped (#140) — near-real-time, survives reboot | — | — |
| **State files** (backlog.md, projects.md, decisions.md, stories.md) | Active — manual maintenance | Refactor: structured format for machine reading | — |
| **Activity.md** (audit trail) | Active — append-only log | Refactor: structured, queryable | #61 |
| **CLAUDE.md files** (role instructions) | Active — generated from fragments | — | — |
| **claudemd-gen.sh** (CLAUDE.md generator) | Shipped — auto-bump Werk version on changes | — | — |
| **Ontology** (chorus.ttl) | Designed (v0.1.0) — not written to TTL | **Implement**: formalize in Fuseki | C#11? |
| **Self domain** (stories, values, life context) | Collecting — stories.md, self-stories.md | — | — |

### 6. THE PLATFORM — Infrastructure

What everything runs on. Silas's vertical.

| Component | State | Action | Card |
|-----------|-------|--------|------|
| **Docker Compose deployment** | Shipped (ADR-015) — app, Fuseki, WebVOWL | — | — |
| **dist/ bind-mount** | Shipped today — compile+restart workflow | — | — |
| **Deploy health gate** | Broken — 30s timeout too short for Fuseki cold starts | **Rewrite**: extend to 60s+ | Brief sent to Silas |
| **Resource contention** (3 agents, 1 Fuseki, 1 SQLite, 1 Docker) | Structural problem — no isolation | **Implement**: needs ADR | C#56 |
| **Shared observability** (Prometheus, Grafana, Loki, Promtail) | Shipped — 8 containers, resource-limited | — | — |
| **Backup** (pods, Fuseki) | Shipped — cron, verify | — | — |

---

## Priority Sequencing

### Now (unblock everything else)
1. **C#57 — Card quality by stage**: Define what "ready" means at each vertebra. This is the spec for everything below.
2. **C#56 — Spine rewrite (quick wins)**: Slim pre-push hook, deploy infra-guardrails to all roles, gate parity manifest.

### Next (strengthen the spine)
3. **#222 — Manifest-first gate**: Complete the auto-trigger, enforce the handshake between directing and building.
4. **C#56 — Spine rewrite (gate parity)**: Single hook manifest, audit enforceable rules, deploy consistently.
5. **C#37 — Clearing voice quality**: The interaction layer is how Jeff experiences Chorus — voice quality is UX.

### Later (extend reach)
6. **C#56 — Resource contention ADR**: How three agents share one environment safely.
7. **C#48 — Cognitive load instrumentation**: Measure what we manage.
8. **C#15 — Autonomous role activation**: The nudge pattern — roles notice things without Jeff typing.
9. **#138 — Build health lava lamp**: Visual deploy health.
10. **C#36 — Clearing mobile**: Jeff directs from anywhere.

---

## The Three Verbs

Every chunk of work falls into one of three categories:

| Verb | Meaning | Examples |
|------|---------|---------|
| **Implement** | Doesn't exist yet, build it new | Card quality spec, gate parity manifest, cognitive load instrumentation |
| **Refactor** | Works but fragile/inconsistent, improve it | Brief acknowledgment, pre-commit portability, state file structure |
| **Rewrite** | Fundamentally wrong approach, start over | Pre-push test suite (too heavy), deploy health gate (too short), Clearing voice (too generic) |

---

---

## Product Principles

Hard-won lessons from building Chorus and from Jeff's 20+ years in integration architecture. These aren't aspirational — they're patterns we've lived.

### Flow-level visibility, not hop-level

Most integration tooling is organized around binding A→B. It cannot tell you if A has downstream relationships beyond B. You can see every connection and still not see the flow. This is the core anti-pattern of point-to-point integration: each hop looks healthy, but emergent behavior — silent data loss, schema mismatches, bottleneck cascading — only surfaces end-to-end.

Chorus builds for flow-level visibility. The pipe diagram is the unit of design, not the step. Scope cards define the end state. Dashboards show whether the whole flow is healthy. Spine events measure flow completion, not hop success.

**Lineage**: Lean manufacturing → *Lean Integration* (John Schmidt, Informatica/Addison-Wesley 2010) → Jeff's patent US9552400B2 (RDF/OWL workflow gates for multi-hop flow comprehension via SPARQL traversal) → Chorus (team-scale orchestration with the same principle). (DEC-061)

**Test**: Can you answer "what breaks downstream if this changes?" If not, you're still hop-level.

### Upfront design proportional to rework cost

Three execution modes, three planning weights (DEC-061):
- **Planning** (cards): rework is cheap. Kill it, rewrite it, move it.
- **Iteration** (demos): rework is moderate. Ship fast, refine live.
- **Harvesting** (pipelines): rework is 10-100x more expensive. A wrong field choice costs hours of reprocessing.

Each mode gets the right amount of upfront thinking. Don't over-plan cards. Don't under-plan pipelines. The harvest design doc exists because we killed a 22-hour extraction that should have been questioned in 10 minutes of design. (DEC-062)

**Test**: How much does it cost to redo this? Match your design investment to that number.

---

*This document is the carrier for C#57. It will accumulate briefs, Clearing transcripts, workflow references, and decisions as we work through the decomposition.*
