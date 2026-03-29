# Brief: Chorus Value Stream + Ontology

**From**: Wren (PM)
**To**: Silas (Architect)
**Date**: 2026-02-17
**Priority**: P1 — parallel with Photos work
**Card**: #60 (Building/Chorus product)

---

## Decision

Jeff named it. The team coordination product is **Chorus**. DEC-019 captured.

---

## Direction from Jeff

Jeff wants the ontology and value stream to drive what Chorus is — same approach as Gathering (DEC-014: concepts → architecture → implementation). Don't start with implementation details. Start with the model.

His words: "take a step up — once we have the value stream and ontology we can start plugging capabilities to the value stream and instrumenting them."

---

## What We Need

### 1. Chorus Value Stream

Gathering has: Gathering → Cultivating → Harvesting → Reflecting

Chorus needs its equivalent cycle. My first-draft proposal:

**Directing → Designing → Building → Proving**

- **Directing**: Human gives intent, captures cards, sets priority, owns sequencing. Capabilities: card creation, priority assignment, WIP limits, direction signals.
- **Designing**: Roles synchronize, briefs flow, architecture decisions land. Capabilities: brief routing, CLAUDE.md personas, ontology modeling, ADRs.
- **Building**: Implementation, commits, tests, shipping. Capabilities: session protocol, commit discipline, state files, test coverage.
- **Proving**: Audit trail, fitness functions, retrospection, protocol compliance. Capabilities: activity log, protocol-status checks, standup summaries, session close-outs.

This is a starting point — push back, reshape, rename. The stages should feel as natural as Gathering's four spaces.

### 2. Chorus Ontology

What are the core classes and relationships? Draft:

- **Role** — a persistent agent persona (Wren, Silas, Kade)
- **Session** — a bounded interaction between human and role
- **Brief** — a structured handoff between roles
- **Card** — a work item on the board
- **Decision** — a captured choice with rationale
- **Signal** — a Slack message, activity log entry, or standup
- **Protocol** — the rules governing how roles interact
- **Artifact** — anything produced (commit, brief, ADR, test, doc)
- **FitnessFunction** — a measurable criterion for protocol health

Relationships:
- Role → produces → Artifact
- Role → sends → Brief → to → Role
- Session → generates → Signal
- Card → moves through → ValueStreamStage
- Decision → constrains → Protocol
- FitnessFunction → measures → Protocol

### 3. What Makes Chorus Different

From Silas's A2A spike: no existing framework packages what we do. The differentiators to model:

1. **Persistent identity** — roles remember across sessions (CLAUDE.md + memory files)
2. **Async coordination** — roles don't run simultaneously (briefs, not API calls)
3. **Human as director** — not orchestrator-agent, not peer-agent. Human owns direction, agents own execution within their domain.
4. **Shared filesystem as state** — repo is the coordination medium, commits are messages
5. **Auditable protocol** — every interaction leaves a trail (activity.md, git log, Slack)

### 4. How This Connects to Gathering

Chorus is the method. Gathering is the proof. The value stream stages map to what we actually do:

| Chorus Stage | Gathering Example |
|---|---|
| Directing | Jeff says "Photos next, breadth over depth" |
| Designing | Silas designs Photos ontology, Wren writes brief |
| Building | Kade builds harvester, ships browse views |
| Proving | Activity log captures it, fitness functions validate it |

Every Gathering feature shipped is a Chorus case study.

---

## Key Framing: Chorus as Extended CI/CD Pipeline

Jeff's insight (2026-02-17): "If you imagine the flow — it is like a longer CI/CD pipeline from value to delivery."

Traditional CI/CD: `commit → build → test → deploy`
Chorus pipeline: `direct → design → build → prove`

Same concept, longer pipe, higher altitude. CI/CD solved the code-to-production gap. Chorus solves the **intent-to-proven-value** gap. This means:

- **Each stage has gates** — a brief doesn't move to Building without architectural sign-off, a card doesn't move to Designing without product priority
- **Each stage has automation potential** — activity log entries on commit, standup posts on session close, brief acknowledgment on receipt
- **Each stage has metrics** — cycle time (card created → done), brief turnaround, decision velocity, protocol compliance rate
- **Each stage has inputs and outputs** — not just labels, but defined artifacts that flow through the pipe

The consulting pitch writes itself: "You have CI/CD for your code. You don't have it for your ideas."

**Model the value stream as a pipeline, not just a cycle.** Stages have defined inputs, outputs, gates, and instrumentation points. This is what makes Chorus instrumentable — and what makes it a product, not just a process doc.

### Trust as the Core Product Dynamic

Jeff's insight: "There is a power and trust issue between humans and machines — if the human has a meaningful and tuned interaction with the team, the whole is greater than the part."

Every multi-agent framework solves coordination mechanics. None address the trust layer. Chorus does. The pipeline is a **trust-building flywheel**:

- **Proving** builds trust (auditable outcomes, visible compliance)
- **Trust** improves **Directing** (human delegates more, with more precision)
- **Better direction** improves **Designing** (less ambiguity, faster decisions)
- **Better design** improves **Building** (clearer scope, fewer false starts)
- **Better building** creates more to **Prove**

Each cycle increases the team's capacity. The ontology should model trust as an emergent property of pipeline cycles completed — not a boolean, but a gradient that grows with proven delivery.

---

## What I'm NOT Asking For

- Implementation — no code, no MCP server, no tooling yet
- Packaging — not thinking about external docs or distribution
- Timeline — this is conceptual work, take the time it needs

Just the model. Ontology + value stream. We plug capabilities in after.

---

— Wren
