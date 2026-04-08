# Building Value Stream

**Version**: 0.1.0
**Date**: 2026-02-15
**Status**: Draft — maps how work flows through the team

---

## The Two Streams

**Gathering** is the value stream of meaning — how Jeff's world enters, settles, connects, and transforms.
**Building** is the value stream of making — how the team turns intent into working, operated software.

They're coupled: Gathering describes what Jeff experiences. Building describes what the team does to make that experience possible. The building stream serves the gathering stream.

```
Gathering:  Spark → Capture → Triage → Settle → Revisit → Transform → Connect → Reflect → (Spark)
Building:   Direct → Frame → Shape → Spec → Build → Verify → Ship → Operate → Evolve → (Direct)
```

Both are cycles, not pipes. Evolve feeds back to Direct. Reflect generates new Sparks.

---

## The Building Stages

### 1. Direct

**Who**: Jeff
**What**: Intent, direction, observation, feedback. The raw signal that something should exist, change, or stop.
**Forms**:
- "I want X" (feature direction)
- "This doesn't work" (bug/feedback)
- "What if..." (exploration)
- "Foundation before features" (principle)
- "The bridge is for coordination, not decisions" (constraint)

**Output**: A stated intent — captured by whichever role is in session.
**Artifact**: Captured in decisions.md, ADR, brief, or Slack (then persisted by receiving role).

### 2. Frame

**Who**: Wren (PM)
**What**: Product framing. What is this? Why does it matter? What's the scope? What's the priority? What's the user experience?
**Interaction mode**: Collaboration (with Jeff) → X-as-a-Service (brief to Silas)
**Input**: Jeff's direction
**Output**: Product brief with scope, priority, acceptance criteria, and sequencing.
**Artifact**: Brief in recipient's `briefs/` directory.

**Example from today**: Jeff said "roles should hear each other." Wren framed it as "bridge must route role-to-role messages" with the constraint (loop safety) and the product requirement (all 4 participants, ~30s response).

### 3. Shape

**Who**: Silas (Architect)
**What**: Architectural design. How does this fit the system? What are the risks? What changes? What's the technical approach? Does it align with the ontology?
**Interaction mode**: Enabling (reduces cognitive load for downstream)
**Input**: Product brief from Wren
**Output**: Design brief — technical approach, trade-offs assessed, cross-project implications mapped.
**Artifact**: Design brief or ADR in `architect/` + brief to recipient.

**Example from today**: Wren's role-to-role brief → marker-based filtering design. Assessed three approaches, recommended simplest, documented loop safety.

### 4. Spec

**Who**: Silas → Kade
**What**: Build-ready specification. Everything the engineer needs: what to change, where, what patterns to follow, what tests to write, what NOT to build.
**Interaction mode**: X-as-a-Service (clear API — the brief IS the contract)
**Input**: Approved design
**Output**: Implementation spec in `engineer/briefs/`.
**Artifact**: Build spec with code-level guidance.

**Example from today**: Glimmer implementation spec — ontology changes, pod structure, handler routes, triage integration, browse view, test coverage expectations.

### 5. Build

**Who**: Kade (Engineer)
**What**: Implementation. Code, tests, integration. Follow the spec, use existing patterns, flag blockers.
**Interaction mode**: Stream-aligned (executing the flow)
**Input**: Build spec from Silas
**Output**: Working code with tests.
**Artifact**: Commits, test results, `current-work.md` updates.

**Example from today**: Kade shipped style guide (gathering.css) — all 19 views migrated, 1681 tests green.

### 6. Verify

**Who**: Kade (automated) + Jeff (live)
**What**: Does it work? Automated tests validate contracts. Jeff validates experience.
**Interaction mode**: Collaboration (Jeff testing live) + Platform (CI/test infrastructure)
**Input**: Built code
**Output**: Test results + Jeff's feedback.
**Artifact**: Test counts, Jeff's observations (captured by active role).

**Two layers**:
- **Automated**: Unit tests, E2E tests, SHACL validation, lint — Kade runs these before shipping.
- **Live**: Jeff uses the feature. His feedback is the real acceptance test. "I pinged you in #silas and you were deaf" — that's a failed live verification.

### 7. Ship

**Who**: Kade
**What**: Deploy, commit, push, announce. The work is available.
**Interaction mode**: Stream-aligned → signal to all
**Input**: Verified code
**Output**: Deployed feature, commit hash, announcement.
**Artifact**: Git commit, Slack signal to #all-gathering, activity.md entry.

**Example from today**: Kade pushed f8a3f2d — style guide + Swagger + link previews. 1681 unit + 119 E2E green.

### 8. Operate

**Who**: All (Platform services)
**What**: The shipped thing runs in production. Monitoring, observability, coordination, incident response.
**Interaction mode**: Platform (self-service infrastructure)
**Input**: Shipped feature
**Output**: Health signals, metrics, alerts.
**Artifact**: Grafana dashboards, Prometheus metrics, Loki logs, bridge responses.

**Components**: Prometheus, Grafana, Loki, Promtail, AlertManager, blackbox probes, the bridge itself (once built).

### 9. Evolve

**Who**: All
**What**: Learning. What worked? What broke? What constraint did we discover? What fitness function do we need?
**Interaction mode**: Facilitating (helping the team get better)
**Input**: Operational experience + Jeff's observations
**Output**: Protocol updates, new constraints, fitness functions, ontology evolution.
**Artifact**: team-architecture.md updates, new ADRs, building.ttl evolution, decisions.md entries.

**Example from today**: Jeff tested the bridge gap live → DEC-017 emerged (bridge scope guardrail) → ADR-006 recorded → fitness function ff-boundary-integrity defined in building.ttl. The system learned something and encoded it.

---

## Stage Transitions & Interaction Modes

```
Direct ──[Collaboration]──→ Frame ──[X-as-a-Service]──→ Shape
                                                           │
                                                    [Enabling]
                                                           │
Evolve ←──[Facilitating]── Operate ←──[Platform]── Ship ←── Verify ←── Build ←──[X-as-a-Service]── Spec
  │
  └──[Facilitating]──→ Direct (cycle)
```

| Transition | Mode | Mechanism |
|---|---|---|
| Direct → Frame | Collaboration | Jeff + Wren in session |
| Frame → Shape | X-as-a-Service | Brief (Wren → Silas) |
| Shape → Spec | Internal | Silas produces both |
| Spec → Build | X-as-a-Service | Brief (Silas → Kade) |
| Build → Verify | Stream-aligned | Kade runs tests |
| Verify → Ship | Stream-aligned | Kade deploys |
| Ship → Operate | Platform | Observability auto-ingests |
| Operate → Evolve | Facilitating | Team observes and learns |
| Evolve → Direct | Facilitating | Learnings feed Jeff's next direction |

---

## Fitness Functions for the Building Stream

Each stage has a testable assertion:

| Stage | Fitness Function | How to Test |
|---|---|---|
| Direct | **Intent captured** | Every Jeff directive appears in an artifact within the same session |
| Frame | **Scope clarity** | Brief contains: what, why, scope, priority, acceptance criteria |
| Shape | **Structural soundness** | Design addresses: system impact, cross-project implications, ontology alignment |
| Spec | **Build readiness** | Kade can build from the spec alone without asking clarifying questions |
| Build | **Pattern adherence** | Implementation follows existing codebase patterns (handler, service, pod) |
| Verify | **Coverage** | Unit + E2E tests cover the acceptance criteria. Jeff tests live. |
| Ship | **Clean deploy** | All tests green, commit pushed, announcement posted |
| Operate | **Visibility** | Feature has observability: metrics, logs, or probes |
| Evolve | **Learning encoded** | Discoveries become constraints, fitness functions, or protocol updates |

---

## How the Two Streams Connect

The building stream exists to serve the gathering stream:

| Gathering Stage | Building Supports It By |
|---|---|
| Spark | Giving Jeff frictionless capture channels (SMS, Slack, catalog) |
| Capture | Building the capture infrastructure (CaptureItem, routing) |
| Triage | Building the triage UI and routing logic |
| Settle | Building pod services, ontology, collection management |
| Revisit | Building browse views, glimmer lifecycle, status transitions |
| Transform | Building promotion paths (Glimmer → Idea → Project) |
| Connect | Building cross-collection relationships, SPARQL queries |
| Reflect | Building dashboards, visualizations, the reflection UI layer |

Every building cycle makes a gathering stage work better. The building stream is in service to meaning-making.

---

## Ontology Representation

These stages are modeled in `building.ttl` as `building:ValueStreamStage` instances. The building value stream stages should be added alongside the gathering stages already defined.

```turtle
building:stage-direct a building:BuildingStage ;
    rdfs:label "Direct" ;
    rdfs:comment "Jeff expresses intent, direction, or feedback." .

building:stage-frame a building:BuildingStage ;
    rdfs:label "Frame" ;
    rdfs:comment "Product framing: scope, priority, acceptance criteria." .

building:stage-shape a building:BuildingStage ;
    rdfs:label "Shape" ;
    rdfs:comment "Architectural design: how it fits, what risks, what approach." .

building:stage-spec a building:BuildingStage ;
    rdfs:label "Spec" ;
    rdfs:comment "Build-ready specification for implementation." .

building:stage-build a building:BuildingStage ;
    rdfs:label "Build" ;
    rdfs:comment "Implementation: code, tests, integration." .

building:stage-verify a building:BuildingStage ;
    rdfs:label "Verify" ;
    rdfs:comment "Automated tests + live user testing." .

building:stage-ship a building:BuildingStage ;
    rdfs:label "Ship" ;
    rdfs:comment "Deploy, commit, push, announce." .

building:stage-operate a building:BuildingStage ;
    rdfs:label "Operate" ;
    rdfs:comment "Monitoring, observability, coordination." .

building:stage-evolve a building:BuildingStage ;
    rdfs:label "Evolve" ;
    rdfs:comment "Learning encoded: new constraints, fitness functions, protocol updates." .
```

---

*The gathering stream is a cycle of meaning. The building stream is a cycle of making. Both are perennial — they come back around. What the building stream learns this cycle makes the next cycle better.*

— Silas
