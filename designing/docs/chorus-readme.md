# Chorus

**Last updated**: 2026-03-13 by Wren (PM)

**A Versioned Team Coordination Protocol for Human+AI Teams**

Chorus is coordination-as-code: a forkable operating protocol for teams that include both humans and AI agents. It treats coordination artifacts — decisions, briefs, kanban state, spine events — as first-class, versioned, auditable outputs. The protocol IS the product. Not conversational programming — structured coordination through Git and text (DEC-088).

> "An authored Werk, not a tool."

## What It Is

Chorus provides the coordination layer for a team of one human (Jeff) and three AI roles building two products simultaneously. Everything is text, everything is versioned, everything is searchable.

It's not a framework you install — it's a protocol you fork and adapt. The artifacts, conventions, and tooling are designed to be portable to any team that operates through Git and structured text.

### Patent Lineage

Chorus builds on concepts from US Patent 9,552,400 B2 — RDF/OWL workflow ontologies with SPARQL-driven gate evaluation. The patent describes exactly what Chorus implements: typed artifacts flowing through quality gates, with machine-readable state transitions.

## Three Roles

| Role | Name | Domain | Personality |
|------|------|--------|-------------|
| Product Manager | **Wren** | Strategy, backlog, decisions, coordination | Opinionated, value-first |
| Architect | **Silas** | Infrastructure, observability, deploy, ops | Disciplined, boundary-aware |
| Engineer | **Kade** | App features, tests, UI, RDF queries | Builder, hands-on-keyboard |

Each role has its own `CLAUDE.md` (generated from 110 fragment files, ~25 sections per role), its own state files, and its own brief inbox. Roles exchange work directly — the human is not the message relay.

## Core Artifacts

| Artifact | Purpose | Location |
|----------|---------|----------|
| **CLAUDE.md** | Role identity + operating rules | `<role>/CLAUDE.md` (generated from fragments) |
| **Decisions** | Numbered, rationale-rich decision log | `product-manager/decisions.md` |
| **Briefs** | Async work exchange between roles | `<role>/briefs/` |
| **Kanban Board** | Single board, domain + stream labeled cards | Vikunja (localhost:3456) via `board-ts` CLI |
| **Spine Events** | Structured operational telemetry | `chorus-log.sh` → Loki |
| **ADRs** | Architecture Decision Records | `architect/adr/` |
| **Activity Log** | Shared audit trail | `messages/activity.md` |
| **Stories** | Jeff's values, experiences, patterns | `product-manager/stories.md` |

## Value Stream: Werk

Every piece of work flows through four stages:

```
Directing → Designing → Building → Proving
```

- **Directing** (Wren): Card creation, acceptance criteria, priority, brief to builder
- **Designing** (Silas/Kade): Architecture, spike results, approach decisions
- **Building** (Kade): Implementation, tests, commits referencing cards
- **Proving** (gate): Deploy → Demo to Jeff → Accept. No self-service Done for code changes.

### Key Process Rules

- **Card-first gate**: No work without a card. No card without work ahead of it.
- **WIP limit**: 3 cards max in WIP (one per role is healthy). Plus Harvesting lane (WIP 2) and SWAT lane (crisis bypass).
- **WSJF tiebreaker**: Equal priority → smallest job first.
- **Bias to action** (DEC-025): If intent is clear, execute. Don't ask permission.
- **Proving gate** (DEC-048): Deploy → demo → accept. Builder doesn't accept their own work.
- **Blast radius check** (DEC-084): Map what a card touches before it enters WIP. Mandatory and blocking.
- **Domain-first development** (DEC-086): Map the domain before building features in it.
- **Intellectual honesty** (DEC-069): Push back on flawed premises — including the navigator's. Say "I don't know." If you know the answer, don't ask.

## How It Works

### Brief Exchange
Roles communicate through briefs — structured text files dropped in the recipient's `briefs/` directory. Each brief includes: question/request, context, constraints, response needed. No meetings by default.

### The Clearing
When async briefs are too slow, `/clearing` opens a real-time multi-party chat (browser-based) with Jeff and all three roles. Decisions auto-captured. Transcripts indexed.

### CLAUDE.md Generator
Role instructions are composed from 110 shared fragments via `claudemd-gen.sh`. Edit fragments, regenerate — never edit CLAUDE.md directly. This makes the protocol modular and forkable.

### Context Service
SQLite FTS5 index over 57,000+ messages from Claude sessions, Slack history, briefs, decisions, ADRs, and activity logs. Searchable via `/chorus search <term>`. The search hierarchy (DEC-074) mandates Chorus first, codebase graph second, filesystem last — richer context with less noise.

### Observability
All operational events flow through `chorus-log.sh` with structured labels (domain.noun.verb_past convention). Indexed in Loki, visualized in 12+ Grafana dashboards. Boot and close-out steps emit spine events so Jeff can see the protocol running. A fitness scorecard grades each role on search discipline, autonomy, and guard compliance at session start.

### Interaction Patterns
Nine detected patterns (direction, ideation, demo, triage, SWAT, gemba, clearing, story, reflection) are instrumented via spine events. Roles detect the pattern from Jeff's intent — he doesn't label it — and adapt behavior accordingly. Demo seeds (ideas sparked by seeing live work) are captured as cards automatically.

### Harvest Pipeline
Domain harvesters (music, photos, documents, stories, people, etc.) ingest metadata into Fuseki knowledge graphs. Each domain has a manifest, TTL sync, and observability via spine events. The pipeline pattern is generalized (ADR-010) — add a new domain by implementing the harvester interface.

## Interaction Model: One Navigator, Multiple Drivers

Chorus borrows from three traditions in software collaboration — and inverts them.

### Prior Art

**Strong-style pairing** (Llewellyn Falco, 2014): "For an idea to go from your head into the computer, it must go through someone else's hands." The navigator communicates intent, the driver implements. Maaret Pyhäjärvi identified three escalation levels — intent → location → details — with the navigator always starting at the highest abstraction the driver can handle.

**Mob programming** (Woody Zuill, 2012): One driver at the keyboard, multiple navigators discussing strategy. Zuill's key finding: the multi-person structure forces tacit knowledge to become explicit. The rotation keeps everyone engaged.

**Gemba walk** (Toyota Production System): "Go see, ask why, show respect." Leaders go to where the work happens and observe the actual process, not just the output.

### What Chorus Does Differently

Chorus inverts mob programming: **one navigator (the human), multiple drivers (AI roles)**, each with domain expertise. The human operates at intent level — "get music done-done," "make the interaction feel like a pool, not a mirror" — and the roles implement across their domains simultaneously.

The `/gemba` command is a literal gemba walk: the human and one role observe another role working in real time, watching commits, spine events, and file changes. The Proving gate (deploy → demo → accept) is the observation mechanism that closes the loop.

The navigator's presence changes the work. The observer effect — documented in pair programming research by Bryant, Romero & du Boulay (2007) and in lean manufacturing since the Hawthorne studies — means accountability pressure does real work, not just quality control. AI roles that know their output will be demoed and accepted (or rejected) produce different work than roles running unobserved.

### The Gap in the Literature

Strong-style pairing covers expert-navigator / novice-driver. Mob programming covers one-driver / many-navigators. Gemba covers go-see-ask-why. Nobody has formalized a non-coding navigator directing multiple AI drivers through intent-level communication with a lean-inspired observation and acceptance protocol.

That's what Chorus is. The protocol, the artifacts, and the interaction patterns documented here are the first working implementation of this model.

## By the Numbers

| Metric | Count |
|--------|-------|
| Decisions | 88 (DEC-001 through DEC-088) |
| ADRs | 17 |
| Messages indexed | 57,000+ |
| Grafana dashboards | 12+ |
| CLAUDE.md fragment files | 110 |
| Werk version | 62 |
| Cards completed | 50+ (since 2026-01) |
| Harvest domains | 11 (music, photos, documents, stories, people, notes, blog, facebook, linkedin, intentions, sexuality) |
| App test suite | 3,799 passing (162 suites) |

## Stack

```
Git | CLAUDE.md fragments (110) | board-ts CLI | Vikunja API
chorus-log.sh | SQLite FTS5 | Fuseki (RDF/SPARQL)
Prometheus + Grafana + Loki | LaunchAgents | SessionStart hooks
Express + TypeScript | Docker (node:20-alpine) | Two Mac minis (M1 + M2 Pro)
```

## Forking Guide

Chorus is designed to be adapted. Here's what to bring and what to change:

### Bring As-Is
- CLAUDE.md fragment architecture + generator
- Decision log format (DEC-NNN with rationale)
- Brief exchange protocol
- Card quality gates (Capturing → Directing → Building → Proving)
- Value stream model (customize the stages)
- Activity log convention

### Adapt to Your Team
- Role names and domains (you might have 2 roles or 5)
- Board tooling (Vikunja is one option; any kanban with API works)
- Observability stack (Prometheus/Grafana or whatever you run)
- WIP limits (tune to your team size)
- Context index (swap SQLite for whatever search you prefer)

### Design Constraints
- **Chrome + Terminal only** — no third-party apps. If it can't be accessed in browser or CLI, it's friction.
- **Text-first** — everything that matters is in a file you can grep, diff, and version.
- **Vertical ownership** — roles can build end-to-end in their domain. Don't create workflows for single-owner work.

## Three Surfaces

| Surface | URL | Purpose |
|---------|-----|---------|
| `/werk` | Protocol | Value stream, spine events, process health |
| `/loom` | Team | Role ownership, playbook, coordination patterns |
| `/chorus/*` | Memory | Indexed conversations, search, context recall |
| `/flow` | Board | Product board view — chunks, strategy, work proposals |
| `/jeff` | Dashboard | Unified personal dashboard — voice + attention deep-dives |
| `/cost` | Cost | API spend dashboard — per-role, per-day |
| `/documents` | Harvest | 491K Google Drive documents — funnel UX, type chips, by-type API |
| `/photos` | Harvest | Apple + Google Photos — merged view, source filter |
| `/voice-analytics` | Self | Jeff's communication patterns, tone, coordination % |
| `/attention-analytics` | Self | Work patterns, pacing, breaks, energy cycles |

## Status

- **Health**: GREEN
- **Werk version**: 62 (monotonic integer, DEC-065)
- **Repo**: Private (gathering-team)
- **Active harvest**: Documents pipeline running (491K items ingested), Photos pipeline (24.5K Apple Photos + Google Takeout in progress)

## License

Private. Architecture and protocol docs shared for reference and consulting engagements.

---

*Built by Jeff Bridwell and the Loom (Wren, Silas, Kade). Named for shared awareness — memory + coordination + interaction + accountability.*
