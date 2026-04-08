# Draft: Team Ontology — Conceptual Model

**From**: Silas (Architect)
**Date**: 2026-02-15
**Status**: First draft for Jeff's review
**Context**: Jeff identified two ontologies in the system — one for the Self (jb-ontology.ttl) and one for the Team. This is the conceptual model for the team ontology.

---

## Two Ontologies, One System

| | Self Ontology | Team Ontology |
|---|---|---|
| **What it models** | What it means to be Jeff — knowledge, interests, connections, memory | How a team forms, aligns, and delivers — roles, communication, decisions, work |
| **Current form** | `jb-ontology.ttl` (OWL/RDF, v0.5.1) | `team-architecture.md` (prose, v1.0) |
| **Core question** | How does a person organize what matters to them? | How do humans and agents build shared understanding and execute together? |
| **Design principle** | The model is the heart — extend the system by changing the model | Understanding = Shared Principles + Patterns + Execution |
| **Values expressed** | Beauty, connection, inheritance, gathering | Honesty, consistency, autonomy, self-healing |

They connect: the self ontology only grows because the team ontology works. The team only has purpose because the self ontology exists. Jeff's values flow through both.

---

## Classes

### Core

**Role** — An agent with a defined purpose, domain expertise, and operating principles.
- Instances: Owner (Jeff), Architect (Silas), PM (Wren), Engineer (Kade)
- Properties: name, purpose, domain, principles, tone, persona
- Constraint: every Role has a persona document and a CLAUDE.md (or equivalent for humans)

**Artifact** — A persistent document that carries substance. The ground, not the wind.
- Subclasses: Brief, Decision, ADR, StateFile, Persona, Record
- Properties: author, date, status, directedTo, supersedes
- Constraint: artifacts are the source of truth. If it matters next week, it's an artifact.

**Channel** — A communication pathway for signals.
- Instances: #all-gathering, #wren, #silas, #kade, #decisions, #standup
- Properties: purpose, whoReads, whoWrites
- Constraint: channels carry signals, not substance. Substance lives in artifacts.

**Session** — A bounded period of active work by a role.
- Properties: role, startTime, endTime, producedArtifacts, consumedArtifacts
- Lifecycle: Synchronize → Operate → Close

**WorkItem** — A unit of work that moves through the delivery pipeline.
- Properties: title, owner, priority, domain, blockedBy, sprint
- Lifecycle: Todo → Ready → InProgress → Blocked → Done

**Sprint** — A planned chunk of work with a coherent theme.
- Properties: name, theme, workItems, owner (PM), startDate, endDate
- Lifecycle: Planning → Active → Complete
- Example: "Foundation Sprint" (CI, observability, alerts, Swagger, metrics)

### Communication

**Brief** — Structured document exchanged between roles. The backbone of collaboration.
- Properties: from, to, date, priority, actionNeeded, content, responseTo
- Lifecycle: Draft → Delivered → Consumed → Responded
- Constraint: lives in recipient's `briefs/` directory
- Constraint: must have from, to, date, priority, actionNeeded

**Signal** — Short notification that context has changed. Ephemeral by nature.
- Properties: priority, type, subject, action, channel
- Format: `[P1/P2/P3] <type>: <subject>. Action: <what's needed>.`
- Constraint: if a signal exceeds ~300 characters, the substance should be in a Brief
- Types: Brief (pointing to artifact), Question (needs short answer), Shipped (status), Record (FYI), Refresh (re-read instructions)

**Decision** — A choice that changes direction.
- Properties: decidedBy, rationale, capturedIn, propagatedVia
- Lifecycle: Proposed → Approved → Captured → Propagated
- Subclasses by domain:
  - ProductDecision (Wren decides, `decisions.md`)
  - ArchitecturalDecision (Silas decides, `adr/`)
  - EngineeringDecision (Kade decides, inline with code)
  - Directive (Jeff decides, captured by active role)

### Team Lifecycle

**TeamStage** — The developmental stage of the team (Tuckman).
- Instances: Forming, Storming, Norming, Performing
- Not a linear progression — teams can cycle back
- Current stage: Norming (transitioning from Storming)

**Principle** — A shared value or constraint that governs team behavior.
- Properties: statement, rationale, examples
- Constraint: principles live in team-architecture.md. Not in Slack, not in individual CLAUDE.md files.
- Current instances: the 7 principles in team-architecture.md

**Pattern** — A consistent behavior derived from principles.
- Properties: name, description, trigger, steps, derivedFrom (Principle)
- Current instances: Brief Protocol, Signal Format, Session Lifecycle, Refresh Pattern, Decision Flow

---

## Relationships

### Role Relationships

```
Role ──produces──→ Artifact
Role ──consumes──→ Artifact
Role ──owns──→ Domain (what they're responsible for)
Role ──inheritsFrom──→ team-architecture.md (shared behavior)
Role ──definesIn──→ CLAUDE.md (role-specific identity)

Owner ──setsDirection──→ Principle
Owner ──approves──→ Decision
Owner ──tests──→ Product (uses it, gives feedback)
```

### Communication Relationships

```
Brief ──directedTo──→ Role
Brief ──respondsTo──→ Brief
Brief ──supersedes──→ Brief
Signal ──pointsTo──→ Artifact
Signal ──sentVia──→ Channel
Decision ──approvedBy──→ Owner
Decision ──capturedBy──→ Role
Decision ──propagatedVia──→ Signal
```

### Work Relationships

```
Sprint ──contains──→ WorkItem
WorkItem ──assignedTo──→ Role
WorkItem ──blockedBy──→ WorkItem
WorkItem ──specifiedIn──→ Brief
Sprint ──sequencedBy──→ PM (Wren)
Sprint ──reviewedBy──→ Architect (Silas)
```

### Cross-Ontology Relationships

```
Team.Owner ←──sameAs──→ Self.Profile
  (Jeff is the bridge between ontologies)

Team.Sprint ──shapes──→ Self.Ontology
  (team work evolves the self model)

Self.CaptureItem ──routedThrough──→ Team.Channel
  (SMS capture flows through team infrastructure)

Self.Ontology ──validatedBy──→ Team.Artifact (SHACL shapes)
  (team artifacts enforce self model integrity)

Team.Principle ──expressesValue──→ Self.Value
  (team principles derive from Jeff's values)
```

---

## Lifecycles

### Session Lifecycle
```
         ┌──────────────┐
         │ Synchronize  │  Read CLAUDE.md, team-architecture.md,
         │              │  briefs/, Slack, activity.md
         └──────┬───────┘
                │
         ┌──────┴───────┐
         │   Operate    │  Every turn: scan signals.
         │              │  Produce: brief + signal + record.
         │              │  Consume: read brief, respond, log.
         └──────┬───────┘
                │
         ┌──────┴───────┐
         │    Close     │  Update activity.md, #standup,
         │              │  role-specific review.
         └──────────────┘
```

### Brief Lifecycle
```
  Draft ──→ Delivered ──→ Consumed ──→ Responded
    │         │              │            │
    │     (signal sent)  (read by     (response brief
    │                    recipient)    or Slack reply)
    │
  (author writes
   to recipient's
   briefs/ dir)
```

### Decision Lifecycle
```
  Proposed ──→ Approved ──→ Captured ──→ Propagated
     │            │            │             │
  (surfaced    (Jeff or     (written to    (signal to
   by any      delegated    decisions.md,   channels,
   role)       authority)   ADR, or code)   briefs to
                                           affected roles)
```

### WorkItem Lifecycle
```
  Todo ──→ Ready ──→ InProgress ──→ Done
                         │
                      Blocked
                    (explicit reason,
                     becomes unblocked
                     when dependency resolves)
```

### Team Lifecycle (Tuckman)
```
  Forming ──→ Storming ──→ Norming ──→ Performing
     │            │            │            │
  (roles      (friction,   (shared      (nervous system
   defined,    gaps found,  model        works, energy
   personas    rules        established, goes to
   written,    challenged,  automation   valuable work)
   first       Jeff pushes  replaces
   artifacts)  for clarity) memory)
                    ↑                      │
                    └──────────────────────┘
                    (teams can cycle back when
                     context changes significantly)
```

---

## Constraints (Team SHACL)

These are the integrity rules — the equivalent of SHACL shapes for the team model. If any of these are violated, the team has a structural problem.

### Artifact Integrity
- Every Brief MUST have: from, to, date, priority, actionNeeded
- Every Decision MUST have: decidedBy, rationale, capturedIn
- Every ADR MUST have: context, decision, consequences
- Substance MUST NOT live only in Slack (persistence constraint)

### Communication Integrity
- Every Brief produced MUST be accompanied by a Signal
- Every Brief consumed MUST be logged in activity.md
- Signals MUST follow the format: `[priority] type: subject. Action: needed.`
- Questions that need more than a paragraph answer SHOULD be Briefs

### Session Integrity
- Every Session MUST start with Synchronize (read artifacts, scan signals)
- Every Session MUST end with Close (update activity.md, post standup)
- Every turn SHOULD include a signal scan (own channel + #all-gathering)

### Role Integrity
- Every Role MUST have: persona, CLAUDE.md, channel
- Every Role MUST inherit shared behavior from team-architecture.md
- Role-specific CLAUDE.md MUST NOT duplicate content from team-architecture.md
- When team-architecture.md updates, all Roles MUST refresh

### Decision Integrity
- Principles and priorities MUST be set by Owner (Jeff)
- Product decisions MUST be made by PM (Wren) with Owner approval
- Architectural decisions MUST be made by Architect (Silas) with Owner approval
- No Role may unilaterally change another Role's domain

### Cross-Ontology Integrity
- Changes to the Self ontology MUST be reviewed by Architect
- Team Sprints that modify the Self ontology MUST bump the version
- Team Principles MUST be traceable to Owner's values

---

## What This Model Enables

When this model is operating:

1. **Onboarding a new role** = instantiate a Role with persona + CLAUDE.md that inherits from team-architecture.md. The shared operating system is already defined — you're just adding a new node.

2. **Diagnosing a communication failure** = trace the lifecycle. Did the Brief get delivered? Was a Signal sent? Did the recipient's Session include Synchronize? Where did the chain break?

3. **Evolving the team** = update team-architecture.md (principles or patterns), send REFRESH signal. Every role picks up the change. One edit, system-wide effect.

4. **Measuring team health** = check constraint violations. Are Briefs being produced without Signals? Are Sessions starting without Synchronize? Are Decisions propagating? The constraints are testable.

5. **Replicating the pattern** = the model is independent of Gathering. Any team with Roles, Artifacts, Channels, and Sessions could adopt these principles and patterns. The Self ontology is Jeff-specific. The Team ontology is generalizable.

---

## Open Questions

1. **Should this become formal RDF/OWL?** The self ontology is in Turtle. Should the team ontology be too, or is prose + structure sufficient? (Argument for: consistency, queryability, SHACL validation. Argument against: the team model is still forming — formalizing too early may calcify.)

2. **Where does the team ontology live long-term?** Currently prose in `messages/team-architecture.md`. If formalized, it could be `messages/team-ontology.ttl` alongside the self ontology. Or a separate project entirely if it's meant to be a reference model.

3. **How do we validate?** The self ontology has SHACL shapes. The team ontology has constraints listed above. Could we build a lightweight "team health check" script that verifies constraint compliance? (e.g., scan briefs/ for files without matching Slack signals, check activity.md for gaps)

4. **What's the relationship to Wren's value stream work?** Jeff mentioned Wren is working on value stream and domains for the self/Jeff ontology. The value stream concept bridges both ontologies — value flows from Jeff's intent (self) through the team's execution (team) into delivered capability (product).

— Silas
