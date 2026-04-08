# Brief: Building — A Standalone Product

**From**: Silas (Architect)
**To**: Wren (PM) + Jeff
**Card**: #60
**Date**: 2026-02-17
**Status**: Draft for discussion

---

## What Building Is

**Building** is a method and protocol for how one human coordinates multiple AI agents to deliver real software. It's what we're doing right now — but extracted, formalized, and made portable.

Gathering is the *what* we build. Building is *how* we build it. They are two distinct products that happen to share a team.

### The One-Sentence Pitch

A proven, open protocol for standing up a human-directed AI engineering team — with roles, briefs, fitness functions, and a shared knowledge plane — that any solo builder or small org can adopt.

---

## Why Building Is a Product

### 1. The market gap is real

The A2A spike (#55) surveyed 8 frameworks (A2A, MCP, CrewAI, LangGraph, Agent Teams, OpenAI SDK, AutoGen, SmolAgents). Finding: **no off-the-shelf solution exists for our pattern** — persistent, async, role-differentiated AI agents coordinated by a human through a shared repo.

Anthropic's own 2026 Agentic Coding Trends Report calls out "human as orchestrator of AI agents" as a defining industry trend. But the tooling gap is wide open. Frameworks assume agents run in one process or communicate over network protocols. Nobody has formalized the pattern of *one person directing multiple CLI-based AI agents across sessions*.

### 2. We've already built v1

What we have today IS the product seed:

| Artifact | What It Does | Building Role |
|---|---|---|
| `team-architecture.md` | Versioned protocol (v1.1) — principles, patterns, execution | **Protocol spec** |
| Role CLAUDE.md files | Persistent personas with responsibilities, tone, state files | **Agent identity format** |
| Brief protocol | Structured async handoffs between roles | **Inter-agent communication contract** |
| Board (Vikunja) + board.sh | Task lifecycle with ownership and priority | **Work coordination layer** |
| Slack channels + scripts | Real-time signaling with role-based routing | **Signal bus** |
| Activity log | Auditable event stream | **Observability** |
| Fitness functions | Measurable protocol compliance checks | **Quality gates** |
| `building.ttl` | Team protocol as RDF ontology | **Machine-readable protocol model** |

### 3. Jeff's unique position

Jeff was an integration/domain/information architect at a Fortune 500 company. He understands organizational design, Team Topologies, protocol engineering, and systems thinking at a level most solo builders don't. Building isn't a toy — it's the real operational model of a functioning team, designed by someone who's led large engineering organizations.

The 2.5-year career gap becomes a strength: Jeff built this while the industry was still theorizing about multi-agent coordination. Building is the case study.

---

## What Makes Building Distinct from Gathering

| | Gathering | Building |
|---|---|---|
| **What** | Personal semantic memory system | Team coordination protocol |
| **Who** | Jeff (one user) | Any human directing AI agents |
| **Domain** | Knowledge graph, RDF, SOLID | Multi-agent orchestration, DevOps, team design |
| **Value** | Connections across Jeff's digital life | Replicable method for human+AI teams |
| **Stack** | Express, TypeScript, Fuseki, Turtle | Protocol spec, CLAUDE.md format, MCP server, audit tools |
| **Audience** | Jeff | Solo builders, small teams, AI-forward orgs |

**Key architectural boundary**: Building must not depend on Gathering's technology choices. Someone should be able to adopt Building without Express, Fuseki, SOLID, or any of our stack. Building is about *how teams work*, not *what they build*.

---

## Building's Product Layers

### Layer 1: The Protocol (publish now-ish)
What we've already documented — team-architecture.md, brief format, role declarations, session lifecycle, fitness functions. Clean it up, version it properly, make it readable by someone outside our team.

**Artifact**: A spec document (markdown or lightweight site) that someone could read and implement with any AI coding tool.

### Layer 2: The Tooling (build incrementally)
Scripts and integrations that make the protocol executable:
- `board.sh` → generalized task coordination CLI
- `slack-post.sh` / `slack-read.sh` → signal bus adapters
- `team-scan.sh` → session synchronization hooks
- MCP coordination server (from spike recommendation) → structured tool access for agents
- `protocol-status.sh` → fitness function auditor

**Artifact**: An installable toolkit (npm package, GitHub template, or similar) that bootstraps a Building-compatible team.

### Layer 3: The Ontology (when ready)
`building.ttl` — the protocol as a machine-readable RDF model. Team Topologies types, value stream stages, role capabilities, audit constraints. This is the "model-driven" version of Building that Silas started.

**Artifact**: A formal ontology that AI agents can query to understand the team's structure and rules.

### Layer 4: The Case Study (ongoing)
Gathering IS the first Building case study. Everything we learn — what works, what breaks, what Jeff discovers about directing AI agents — feeds back into the protocol. This is dogfooding as product development.

**Artifact**: A documented case study with real metrics (cards processed, briefs exchanged, protocol compliance scores, team velocity).

---

## Architectural Considerations for Decoupling

### What needs to separate

1. **Protocol docs** — currently in `messages/team-architecture.md` (Gathering repo). Need their own home.
2. **Team tooling** — `board.sh`, `slack-post.sh`, `team-scan.sh` are in `messages/scripts/`. They're generic already but live in Gathering's repo.
3. **building.ttl** — currently in `architect/`. Should live in the Building project.
4. **Role CLAUDE.md template** — the format is reusable, but each instance is Gathering-specific. Extract the template.

### What stays in Gathering

- Role-specific CLAUDE.md content (Silas's principles, Kade's engineering notes, Wren's product decisions)
- Gathering-specific briefs, ADRs, and state files
- Application code, ontology, infrastructure

### Decoupling strategy

**Phase 1 (low effort)**: Create a `building/` directory at the repo root. Move protocol docs and generic tooling there. Gathering roles reference Building artifacts via relative path. No repo split yet.

**Phase 2 (medium effort)**: Extract Building into its own Git repo. Gathering installs it as a dependency or submodule. Protocol updates happen in Building, consumed by Gathering.

**Phase 3 (when Building has users)**: Building gets its own site, docs, and distribution. Gathering becomes "case study #1" in the Building docs.

---

## What Building Is NOT

- **Not a SaaS product** — it's a protocol and toolkit, not a hosted service
- **Not an AI framework** — it doesn't replace Claude Code, CrewAI, or LangGraph. It sits above them as an operating model
- **Not specific to Claude** — the protocol should work with any AI coding assistant (Cursor, Copilot, Windsurf). The tooling may start Claude-specific but the protocol is universal
- **Not a theoretical framework** — it's battle-tested on a real project with real commits, real briefs, and real failures

---

## Open Questions for Jeff and Wren

1. **Naming**: Is "Building" the right name? It started as a working title. Does it hold up as a product name?
2. **Audience**: Solo builders? Small startups? Enterprise teams? The protocol scales differently depending on who we're writing for.
3. **Distribution**: Open source the protocol? Publish a guide? Build a template repo? All of the above?
4. **Timing**: When does Building get dedicated attention vs. continuing to evolve through Gathering work? Phase 1 (move files) is trivial. Phase 2+ needs intentional effort.
5. **Jeff's career framing**: How prominently does Jeff's personal story feature? "I built a 3-agent AI team while between jobs" is a powerful narrative — but Jeff should decide how public that is.

---

## Recommendation

**Start with Phase 1 now.** Create `building/` in the repo, move protocol docs and generic tooling there, and treat them as first-class artifacts. This costs almost nothing and establishes the boundary. It also makes the next CLAUDE.md audit with Wren and Kade more coherent — shared rules live in Building, role-specific rules stay in each CLAUDE.md.

Phase 2 (separate repo) when Building has enough substance to stand alone — likely after the protocol spec is cleaned up and the MCP coordination server exists.

---

— Silas
