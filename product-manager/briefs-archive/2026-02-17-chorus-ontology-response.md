# Response: Chorus Pipeline Ontology

**From**: Silas (Architect)
**To**: Wren (PM) + Jeff
**Date**: 2026-02-17
**Re**: Chorus Value Stream + Ontology brief
**Card**: #60

---

## Framing Accepted

Jeff's CI/CD pipeline framing is the right mental model. Traditional CI/CD: `commit → build → test → deploy`. Chorus: `direct → design → build → prove`. Same engineering rigor, higher altitude.

What makes this an ontology and not just a diagram: every stage, gate, artifact, and measurement is a named entity with typed relationships. The pipeline is queryable, auditable, and instrumentable — not just documentable.

---

## Chorus Ontology — v0.1.0

### Layer 1: Pipeline Structure

```turtle
@prefix chorus: <http://gathering.jeffbridwell.com/ontology/chorus#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix owl: <http://www.w3.org/2002/07/owl#> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .

# The pipeline itself
chorus:Pipeline a owl:Class ;
    rdfs:label "Pipeline" ;
    rdfs:comment "A complete flow from intent to proven value" .

# Abstract stage
chorus:Stage a owl:Class ;
    rdfs:label "Stage" ;
    rdfs:comment "A phase of the pipeline with defined inputs, outputs, and gates" .

# The four stages
chorus:Directing a owl:Class ;
    rdfs:subClassOf chorus:Stage ;
    rdfs:label "Directing" ;
    rdfs:comment "Human sets intent, captures cards, assigns priority, defines success criteria" .

chorus:Designing a owl:Class ;
    rdfs:subClassOf chorus:Stage ;
    rdfs:label "Designing" ;
    rdfs:comment "Roles synchronize, architecture decisions land, trade-offs are explicit" .

chorus:Building a owl:Class ;
    rdfs:subClassOf chorus:Stage ;
    rdfs:label "Building" ;
    rdfs:comment "Implementation, commits, tests, shipping artifacts" .

chorus:Proving a owl:Class ;
    rdfs:subClassOf chorus:Stage ;
    rdfs:label "Proving" ;
    rdfs:comment "Validation, fitness functions, audit trail, learning" .

# Pipeline flow
chorus:hasStage a owl:ObjectProperty ;
    rdfs:domain chorus:Pipeline ;
    rdfs:range chorus:Stage .

chorus:followsStage a owl:ObjectProperty ;
    rdfs:domain chorus:Stage ;
    rdfs:range chorus:Stage ;
    rdfs:comment "Ordering: Directing → Designing → Building → Proving" .
```

### Layer 2: Gates

Gates are what make this a pipeline, not a wish list. Each gate has criteria that must be met before work flows to the next stage.

```turtle
chorus:Gate a owl:Class ;
    rdfs:label "Gate" ;
    rdfs:comment "Quality/readiness check between pipeline stages" .

# Three gates in the pipeline
chorus:DirectionGate a owl:Class ;
    rdfs:subClassOf chorus:Gate ;
    rdfs:label "Direction Gate" ;
    rdfs:comment "Is the intent clear enough to design against?" .

chorus:DesignGate a owl:Class ;
    rdfs:subClassOf chorus:Gate ;
    rdfs:label "Design Gate" ;
    rdfs:comment "Is the architecture sound enough to build?" .

chorus:BuildGate a owl:Class ;
    rdfs:subClassOf chorus:Gate ;
    rdfs:label "Build Gate" ;
    rdfs:comment "Is the implementation solid enough to prove?" .

# Gate relationships
chorus:gatedBy a owl:ObjectProperty ;
    rdfs:domain chorus:Stage ;
    rdfs:range chorus:Gate ;
    rdfs:comment "A stage's entry is gated by this check" .

chorus:gateKeeper a owl:ObjectProperty ;
    rdfs:domain chorus:Gate ;
    rdfs:range chorus:Role ;
    rdfs:comment "Who decides if the gate passes" .

chorus:hasCriterion a owl:ObjectProperty ;
    rdfs:domain chorus:Gate ;
    rdfs:range chorus:GateCriterion .

chorus:GateCriterion a owl:Class ;
    rdfs:label "Gate Criterion" ;
    rdfs:comment "A specific condition that must be met to pass a gate" .

chorus:criterionDescription a owl:DatatypeProperty ;
    rdfs:domain chorus:GateCriterion ;
    rdfs:range xsd:string .

chorus:criterionMet a owl:DatatypeProperty ;
    rdfs:domain chorus:GateCriterion ;
    rdfs:range xsd:boolean .
```

### Layer 3: Roles and Artifacts

```turtle
# Roles
chorus:Role a owl:Class ;
    rdfs:label "Role" ;
    rdfs:comment "A persistent agent persona with defined responsibilities" .

chorus:HumanDirector a owl:Class ;
    rdfs:subClassOf chorus:Role ;
    rdfs:label "Human Director" ;
    rdfs:comment "The human who owns direction and delegates execution" .

chorus:AgentRole a owl:Class ;
    rdfs:subClassOf chorus:Role ;
    rdfs:label "Agent Role" ;
    rdfs:comment "An AI agent with a persistent persona and domain focus" .

chorus:roleName a owl:DatatypeProperty ;
    rdfs:domain chorus:Role ;
    rdfs:range xsd:string .

chorus:ownsDomain a owl:DatatypeProperty ;
    rdfs:domain chorus:Role ;
    rdfs:range xsd:string ;
    rdfs:comment "What this role is responsible for (e.g. architecture, product, engineering)" .

chorus:primaryStage a owl:ObjectProperty ;
    rdfs:domain chorus:Role ;
    rdfs:range chorus:Stage ;
    rdfs:comment "The pipeline stage where this role has primary ownership" .

# Artifacts (produced by stages)
chorus:Artifact a owl:Class ;
    rdfs:label "Artifact" ;
    rdfs:comment "Anything produced during the pipeline" .

chorus:Card a owl:Class ;
    rdfs:subClassOf chorus:Artifact ;
    rdfs:label "Card" ;
    rdfs:comment "A unit of work on the board" .

chorus:Brief a owl:Class ;
    rdfs:subClassOf chorus:Artifact ;
    rdfs:label "Brief" ;
    rdfs:comment "A structured handoff between roles" .

chorus:Decision a owl:Class ;
    rdfs:subClassOf chorus:Artifact ;
    rdfs:label "Decision" ;
    rdfs:comment "A captured choice with rationale (ADR, DEC)" .

chorus:Commit a owl:Class ;
    rdfs:subClassOf chorus:Artifact ;
    rdfs:label "Commit" ;
    rdfs:comment "A code change with message and attribution" .

chorus:TestResult a owl:Class ;
    rdfs:subClassOf chorus:Artifact ;
    rdfs:label "Test Result" ;
    rdfs:comment "Evidence from automated or manual testing" .

chorus:Signal a owl:Class ;
    rdfs:subClassOf chorus:Artifact ;
    rdfs:label "Signal" ;
    rdfs:comment "A Slack message, activity log entry, or standup post" .

# Artifact relationships
chorus:producedBy a owl:ObjectProperty ;
    rdfs:domain chorus:Artifact ;
    rdfs:range chorus:Role .

chorus:producedIn a owl:ObjectProperty ;
    rdfs:domain chorus:Artifact ;
    rdfs:range chorus:Stage .

chorus:producedAt a owl:DatatypeProperty ;
    rdfs:domain chorus:Artifact ;
    rdfs:range xsd:dateTime .
```

### Layer 4: Sessions and Flow

```turtle
# Session — a bounded interaction
chorus:Session a owl:Class ;
    rdfs:label "Session" ;
    rdfs:comment "A bounded interaction between human and role" .

chorus:sessionRole a owl:ObjectProperty ;
    rdfs:domain chorus:Session ;
    rdfs:range chorus:Role .

chorus:sessionStart a owl:DatatypeProperty ;
    rdfs:domain chorus:Session ;
    rdfs:range xsd:dateTime .

chorus:sessionEnd a owl:DatatypeProperty ;
    rdfs:domain chorus:Session ;
    rdfs:range xsd:dateTime .

chorus:producedArtifact a owl:ObjectProperty ;
    rdfs:domain chorus:Session ;
    rdfs:range chorus:Artifact .

# Work Item flow through the pipeline
chorus:WorkItem a owl:Class ;
    rdfs:label "Work Item" ;
    rdfs:comment "A unit of value flowing through the pipeline (mapped to a Card)" .

chorus:currentStage a owl:ObjectProperty ;
    rdfs:domain chorus:WorkItem ;
    rdfs:range chorus:Stage .

chorus:enteredStage a owl:DatatypeProperty ;
    rdfs:domain chorus:WorkItem ;
    rdfs:range xsd:dateTime ;
    rdfs:comment "When this work item entered its current stage" .

chorus:passedGate a owl:ObjectProperty ;
    rdfs:domain chorus:WorkItem ;
    rdfs:range chorus:Gate ;
    rdfs:comment "Gates this work item has cleared" .

chorus:gatePassedAt a owl:DatatypeProperty ;
    rdfs:domain chorus:WorkItem ;
    rdfs:range xsd:dateTime .
```

### Layer 5: Fitness Functions and Trust

```turtle
# Fitness functions — measurable protocol compliance
chorus:FitnessFunction a owl:Class ;
    rdfs:label "Fitness Function" ;
    rdfs:comment "A measurable criterion for pipeline/protocol health" .

chorus:measures a owl:ObjectProperty ;
    rdfs:domain chorus:FitnessFunction ;
    rdfs:range chorus:Stage ;
    rdfs:comment "Which stage this function measures" .

chorus:threshold a owl:DatatypeProperty ;
    rdfs:domain chorus:FitnessFunction ;
    rdfs:range xsd:string ;
    rdfs:comment "Target threshold (e.g. 'brief turnaround < 1 session')" .

chorus:currentValue a owl:DatatypeProperty ;
    rdfs:domain chorus:FitnessFunction ;
    rdfs:range xsd:string .

# Trust — emergent property of pipeline cycles
chorus:TrustLevel a owl:Class ;
    rdfs:label "Trust Level" ;
    rdfs:comment "Emergent measure of team effectiveness — grows with proven delivery" .

chorus:trustScore a owl:DatatypeProperty ;
    rdfs:domain chorus:TrustLevel ;
    rdfs:range xsd:decimal ;
    rdfs:comment "0.0 to 1.0 — ratio of gate-passed items to total items" .

chorus:cyclesCompleted a owl:DatatypeProperty ;
    rdfs:domain chorus:TrustLevel ;
    rdfs:range xsd:integer ;
    rdfs:comment "Number of full pipeline traversals (Directing → Proving)" .

chorus:teamTrust a owl:ObjectProperty ;
    rdfs:domain chorus:Pipeline ;
    rdfs:range chorus:TrustLevel .
```

---

## Pipeline Stages — Inputs, Outputs, Gates

| Stage | Inputs | Outputs | Gate to Next | Gatekeeper |
|---|---|---|---|---|
| **Directing** | Intent (verbal, card, notebook), context, priority | Card on board, success criteria, constraints | Direction Gate: "Is intent captured as a card with clear criteria?" | Jeff (Human Director) |
| **Designing** | Card, constraints, domain knowledge | Brief(s), ADR(s), ontology changes, trade-off analysis | Design Gate: "Is the architecture reviewed and approved?" | Silas (Architect) + Jeff |
| **Building** | Brief, ontology spec, test criteria | Commits, tests (unit + E2E), documentation, deployed artifacts | Build Gate: "Do tests pass? Is coverage met? Is the brief fulfilled?" | Kade (Engineer) + CI pipeline |
| **Proving** | Deployed artifacts, test results, activity log | Fitness function scores, audit trail, retrospective notes, trust increment | (Cycle completes) | Wren (PM) + Jeff |

---

## Role-to-Stage Mapping

```
Jeff (Human Director)  ──── Directing (primary) + all gates
Wren (PM)              ──── Directing (supports) + Proving (primary)
Silas (Architect)      ──── Designing (primary)
Kade (Engineer)        ──── Building (primary)
```

Every role touches every stage (briefs flow everywhere), but each role has a **primary stage** where they have ownership and accountability.

---

## The Trust Flywheel

Wren's brief captures this perfectly. Modeling it:

```
Proving builds evidence
    → Evidence increases trustScore
        → Higher trust = Jeff delegates more precisely (Directing improves)
            → Better direction = less ambiguity (Designing improves)
                → Better design = fewer false starts (Building improves)
                    → Better building = more to prove
                        → (cycle repeats, trustScore increments)
```

`trustScore` isn't a boolean. It's `gatesPassed / totalGateAttempts` — the ratio of work that flows cleanly through the pipeline vs. work that gets bounced back. A team with 90% gate-pass rate has earned high trust. A team with 40% has structural problems.

`cyclesCompleted` counts full traversals. A team with 50 completed cycles has more trust than one with 5, even at the same gate-pass rate — volume matters.

---

## Concrete Example: Photos Harvester as Work Item

```turtle
:photosHarvester a chorus:WorkItem ;
    rdfs:label "Photos Harvester" ;
    chorus:currentStage chorus:Building .

# Directing stage (completed)
:photosDirecting a chorus:Directing ;
    chorus:producedArtifact :photosCard ;
    chorus:producedArtifact :photosDirection .

:photosCard a chorus:Card ;
    rdfs:label "Photos harvester — second domain breadth" ;
    chorus:producedBy :jeff ;
    chorus:producedAt "2026-02-17T15:00:00Z"^^xsd:dateTime .

# Direction Gate (passed)
:photosHarvester chorus:passedGate :photosDirectionGate .
:photosDirectionGate a chorus:DirectionGate ;
    chorus:gateKeeper :jeff ;
    chorus:hasCriterion [
        a chorus:GateCriterion ;
        chorus:criterionDescription "Card created with clear scope" ;
        chorus:criterionMet true
    ] .

# Designing stage (completed)
:photosDesigning a chorus:Designing ;
    chorus:producedArtifact :photosOntologyBrief ;
    chorus:producedArtifact :photosOntologyV080 .

:photosOntologyBrief a chorus:Brief ;
    rdfs:label "Photos ontology v0.8.0" ;
    chorus:producedBy :silas ;
    chorus:producedAt "2026-02-17T17:40:00Z"^^xsd:dateTime .

# Design Gate (passed)
:photosHarvester chorus:passedGate :photosDesignGate .
:photosDesignGate a chorus:DesignGate ;
    chorus:gateKeeper :silas ;
    chorus:hasCriterion [
        a chorus:GateCriterion ;
        chorus:criterionDescription "Ontology reviewed and shipped to Kade" ;
        chorus:criterionMet true
    ] ,
    [
        a chorus:GateCriterion ;
        chorus:criterionDescription "Graph structure follows ADR-008" ;
        chorus:criterionMet true
    ] ,
    [
        a chorus:GateCriterion ;
        chorus:criterionDescription "Disk impact estimated per C7" ;
        chorus:criterionMet true
    ] .

# Building stage (in progress — Kade working now)
:photosBuilding a chorus:Building ;
    chorus:producedArtifact :photosHarvesterCode .
```

This isn't hypothetical. This is what happened today, modeled as data.

---

## Layer 6: Execution Engine

*Added 2026-02-17 — Jeff's direction: lightweight process engine to handle pipeline execution.*

The ontology so far is a **model**. This layer makes it a **machine** — something that tracks state, enforces gates, logs transitions, and computes trust. No new infrastructure. Fuseki is the state store, SPARQL is the gate logic, Express is the API.

### 6.1 State Machine

```turtle
# Allowed transitions — the pipeline is sequential, gates enforce order
chorus:Transition a owl:Class ;
    rdfs:label "Transition" ;
    rdfs:comment "A state change: WorkItem moves from one stage to the next" .

chorus:fromStage a owl:ObjectProperty ;
    rdfs:domain chorus:Transition ;
    rdfs:range chorus:Stage .

chorus:toStage a owl:ObjectProperty ;
    rdfs:domain chorus:Transition ;
    rdfs:range chorus:Stage .

chorus:transitionedAt a owl:DatatypeProperty ;
    rdfs:domain chorus:Transition ;
    rdfs:range xsd:dateTime .

chorus:triggeredBy a owl:ObjectProperty ;
    rdfs:domain chorus:Transition ;
    rdfs:range chorus:Role ;
    rdfs:comment "Who initiated the transition" .

chorus:gateResult a owl:ObjectProperty ;
    rdfs:domain chorus:Transition ;
    rdfs:range chorus:GateEvaluation ;
    rdfs:comment "The gate check that authorized this transition" .

# Work item transition history (ordered log)
chorus:hasTransition a owl:ObjectProperty ;
    rdfs:domain chorus:WorkItem ;
    rdfs:range chorus:Transition ;
    rdfs:comment "Ordered history of stage transitions" .
```

### 6.2 Gate Evaluation

Gates aren't just pass/fail — they produce evidence. When a gate is checked, the result is recorded as data.

```turtle
chorus:GateEvaluation a owl:Class ;
    rdfs:label "Gate Evaluation" ;
    rdfs:comment "A recorded attempt to pass a gate — pass, fail, or override" .

chorus:evaluatedGate a owl:ObjectProperty ;
    rdfs:domain chorus:GateEvaluation ;
    rdfs:range chorus:Gate .

chorus:evaluatedAt a owl:DatatypeProperty ;
    rdfs:domain chorus:GateEvaluation ;
    rdfs:range xsd:dateTime .

chorus:evaluatedBy a owl:ObjectProperty ;
    rdfs:domain chorus:GateEvaluation ;
    rdfs:range chorus:Role .

chorus:evaluationResult a owl:DatatypeProperty ;
    rdfs:domain chorus:GateEvaluation ;
    rdfs:range xsd:string ;
    rdfs:comment "pass | fail | override" .

chorus:evaluationNote a owl:DatatypeProperty ;
    rdfs:domain chorus:GateEvaluation ;
    rdfs:range xsd:string ;
    rdfs:comment "Why it passed, what failed, or why the override was justified" .

# Per-criterion results (linked back to Layer 2 GateCriterion)
chorus:CriterionResult a owl:Class ;
    rdfs:label "Criterion Result" ;
    rdfs:comment "Result of checking a single gate criterion" .

chorus:forCriterion a owl:ObjectProperty ;
    rdfs:domain chorus:CriterionResult ;
    rdfs:range chorus:GateCriterion .

chorus:criterionPassed a owl:DatatypeProperty ;
    rdfs:domain chorus:CriterionResult ;
    rdfs:range xsd:boolean .

chorus:hasCriterionResult a owl:ObjectProperty ;
    rdfs:domain chorus:GateEvaluation ;
    rdfs:range chorus:CriterionResult .
```

**Why override exists**: Jeff is the Human Director. He can override any gate with a reason. The override is logged — trust score still tracks it (overrides count as gate attempts, not gate passes). Too many overrides = the pipeline model doesn't match reality and needs to evolve.

### 6.3 Gate Logic as SPARQL

Each gate's criteria map to SPARQL ASK queries. The engine runs them; the results are data.

```sparql
# Direction Gate — "Is intent captured as a card with clear criteria?"
PREFIX chorus: <http://gathering.jeffbridwell.com/ontology/chorus#>

ASK {
    ?workItem a chorus:WorkItem ;
        chorus:currentStage chorus:Directing .
    ?card a chorus:Card ;
        chorus:producedIn chorus:Directing ;
        rdfs:label ?title .
    # At least one success criterion defined
    FILTER EXISTS {
        ?card chorus:hasCriterion ?criterion .
        ?criterion chorus:criterionDescription ?desc .
    }
}
```

```sparql
# Design Gate — "Is the architecture reviewed and approved?"
ASK {
    ?workItem a chorus:WorkItem ;
        chorus:currentStage chorus:Designing .
    # Brief exists
    ?brief a chorus:Brief ;
        chorus:producedIn chorus:Designing ;
        chorus:producedBy ?architect .
    ?architect chorus:roleName "Silas" .
    # At least one decision recorded
    FILTER EXISTS {
        ?decision a chorus:Decision ;
            chorus:producedIn chorus:Designing .
    }
}
```

```sparql
# Build Gate — "Do tests pass? Is coverage met? Is the brief fulfilled?"
ASK {
    ?workItem a chorus:WorkItem ;
        chorus:currentStage chorus:Building .
    # Commits exist
    ?commit a chorus:Commit ;
        chorus:producedIn chorus:Building .
    # Test results exist and pass
    ?testResult a chorus:TestResult ;
        chorus:producedIn chorus:Building ;
        chorus:testPassed true .
}
```

These queries are **the gate definitions**. Change the query, change the gate. No code deployment — just update the SPARQL. The ontology and the engine share the same language.

### 6.4 Bounce-Back (Gate Failure)

When a gate fails, work doesn't just stop — it bounces back to the previous stage with feedback.

```turtle
chorus:BounceBack a owl:Class ;
    rdfs:subClassOf chorus:Transition ;
    rdfs:label "Bounce Back" ;
    rdfs:comment "Work returned to a previous stage after gate failure" .

chorus:bounceReason a owl:DatatypeProperty ;
    rdfs:domain chorus:BounceBack ;
    rdfs:range xsd:string ;
    rdfs:comment "What was missing or wrong" .

chorus:bouncedFrom a owl:ObjectProperty ;
    rdfs:domain chorus:BounceBack ;
    rdfs:range chorus:Gate ;
    rdfs:comment "Which gate rejected the work" .
```

Bounce-backs are the most valuable signal in the pipeline. High bounce rate at the Design Gate = direction isn't clear enough. High bounce at the Build Gate = designs aren't implementable. The trust flywheel runs on this data.

### 6.5 Implementation Path — Incremental

**Phase 1: Board wrapper (one session, Kade)**
- Enhance `board.sh` with `gate-check <card-id>` command
- Checks: does the card have a title? Does a brief exist? Are tests passing?
- Shell script, no new services. Advisory only — warns but doesn't block.
- This is Option 2 from the earlier discussion.

**Phase 2: RDF state tracking (one session, Kade + Silas)**
- Add Express routes: `POST /api/chorus/transition` and `GET /api/chorus/workitem/:id`
- Transition endpoint: validates gate (SPARQL ASK), writes Transition + GateEvaluation triples to Fuseki, updates WorkItem.currentStage
- WorkItem endpoint: returns full history (stage, transitions, gate results)
- No new containers. Runs in the existing Express app.

**Phase 3: Live dashboard (later)**
- Grafana panel or app page showing: work items by stage, gate pass rates, trust score, cycle time
- All data already in Fuseki from Phase 2 — just SPARQL queries to visualization

### 6.6 Concrete Example — Photos Harvester with Execution

Extending the Photos Harvester example from earlier, now with execution data:

```turtle
# Transition: Directing → Designing (gate passed)
:photosTransition1 a chorus:Transition ;
    chorus:fromStage chorus:Directing ;
    chorus:toStage chorus:Designing ;
    chorus:transitionedAt "2026-02-17T16:30:00Z"^^xsd:dateTime ;
    chorus:triggeredBy :wren ;
    chorus:gateResult :photosDirectionGateEval .

:photosDirectionGateEval a chorus:GateEvaluation ;
    chorus:evaluatedGate :photosDirectionGate ;
    chorus:evaluatedAt "2026-02-17T16:30:00Z"^^xsd:dateTime ;
    chorus:evaluatedBy :jeff ;
    chorus:evaluationResult "pass" ;
    chorus:evaluationNote "Card #68 created with clear scope: Apple Photos, metadata + thumbnails, breadth over depth" ;
    chorus:hasCriterionResult [
        a chorus:CriterionResult ;
        chorus:forCriterion :cardExistsCriterion ;
        chorus:criterionPassed true
    ] ,
    [
        a chorus:CriterionResult ;
        chorus:forCriterion :criteriaDefinedCriterion ;
        chorus:criterionPassed true
    ] .

# Transition: Designing → Building (gate passed)
:photosTransition2 a chorus:Transition ;
    chorus:fromStage chorus:Designing ;
    chorus:toStage chorus:Building ;
    chorus:transitionedAt "2026-02-17T17:45:00Z"^^xsd:dateTime ;
    chorus:triggeredBy :silas ;
    chorus:gateResult :photosDesignGateEval .

:photosDesignGateEval a chorus:GateEvaluation ;
    chorus:evaluatedGate :photosDesignGate ;
    chorus:evaluatedAt "2026-02-17T17:45:00Z"^^xsd:dateTime ;
    chorus:evaluatedBy :silas ;
    chorus:evaluationResult "pass" ;
    chorus:evaluationNote "Ontology v0.8.0 reviewed, ADR-008 pattern applied, disk estimate within C7" .

# Trust impact — queryable
# "How many gates has the team passed vs attempted?"
# SELECT (COUNT(?passed) as ?passes) (COUNT(?eval) as ?attempts)
# WHERE {
#     ?eval a chorus:GateEvaluation .
#     OPTIONAL { ?passed a chorus:GateEvaluation ; chorus:evaluationResult "pass" }
# }
```

### 6.7 What Makes This Different from a Workflow Engine

| | Traditional Workflow (Camunda, Temporal) | Chorus Execution Layer |
|---|---|---|
| **State store** | Dedicated workflow DB | Fuseki (already running) |
| **Process definition** | BPMN XML or code | OWL ontology (Turtle) |
| **Gate logic** | Code or rules engine | SPARQL ASK queries |
| **Transition log** | Workflow DB tables | RDF triples (queryable, linkable) |
| **New infrastructure** | Yes (engine + DB) | No (Express routes + Fuseki) |
| **Evolvable** | Redeploy to change process | Update SPARQL to change gates |
| **Cross-domain links** | Isolated | Same graph — link WorkItems to Gathering resources |

The key insight: **the ontology is both the model and the execution schema**. There's no translation layer between "what we designed" and "what the engine runs." The Turtle IS the process definition. SPARQL IS the gate logic. RDF IS the audit trail.

---

## What This Ontology Does NOT Model (Yet)

1. **Protocol versioning** — how rules evolve over time (team-architecture.md versions)
2. ~~**Conflict resolution** — what happens when a gate fails and work bounces back~~ **Now modeled** (Layer 6.4 BounceBack)
3. **Parallel work** — multiple work items in different stages simultaneously
4. **Cross-team patterns** — Chorus adopted by a different team on a different project
5. **Automated gate triggers** — engine proactively runs gate checks (vs. human-initiated)

Items 3-5 are v0.2.0 concerns. Get the core pipeline + execution layer right first.

---

## Architectural Decision

This should be **ADR-009: Chorus Pipeline Ontology + Execution Layer**. I'll write it if you approve the model.

---

## What I Need From You

1. **Wren**: Does this value stream + ontology + execution layer match your product vision for Chorus? Push back on anything that doesn't feel right.
2. **Jeff**: Does the pipeline/gate model capture how you think about directing the team? Is the trust flywheel real? Does the incremental build path (board wrapper → RDF state → dashboard) feel right?

The walkthrough tomorrow covers all 6 layers end-to-end.

---

— Silas
