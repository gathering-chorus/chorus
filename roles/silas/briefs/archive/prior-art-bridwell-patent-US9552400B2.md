# Prior Art: Bridwell Patent — US9552400B2

**Patent**: US9552400B2 (WO2013181588A3)
**Title**: Defining and Mapping Application Interface Semantics
**Inventors**: Pavitra Krishnan, **William Jeffrey Bridwell**, Chandra Shekar Bommasamudra
**Assignee**: Staples Inc.
**Filed**: May 31, 2013
**Issued**: January 24, 2017
**Expires**: April 30, 2034

---

## What It Does

Enterprise-scale system for defining, mapping, and managing application interface semantics across an Enterprise Service Bus (ESB). The key innovation: using **RDF/OWL as the integration data store** and **SPARQL as the query/validation layer**, with a workflow engine that enforces approval gates.

### Core Architecture

```
┌─────────────┐     ┌──────────────────────┐     ┌─────────────┐
│  Consumer    │     │   Enterprise Service │     │  Provider   │
│  Services    │────▶│   Bus (Canonical     │────▶│  Services   │
│  (ICDs)      │     │   Model)             │     │  (ICDs)     │
└─────────────┘     └──────────────────────┘     └─────────────┘
       │                      │                         │
       └──────────┬───────────┘                         │
                  ▼                                     │
       ┌──────────────────────┐                         │
       │  Integration Data    │◀────────────────────────┘
       │  Store (RDF/OWL)     │
       │  + SPARQL queries    │
       └──────────────────────┘
                  │
       ┌──────────┴───────────┐
       │  Workflow Module     │
       │  (Draft → Review →   │
       │   Approve → Deploy)  │
       └──────────────────────┘
```

### Key Technical Elements

1. **RDF/OWL Knowledge Graph**: Interface Control Documents (ICDs) stored as RDF triples. Subject-predicate-object expressions enable relationship linking across 1,000-1,000,000+ applications.

2. **SPARQL Query Layer**: Selecting, updating, and traversing dependency chains across integration topologies. Semantic-level impact analysis — trace how a change to one field affects all dependent mappings.

3. **Workflow with Approval Gates**:
   - Draft → Publication Review → Stakeholder Review → Approval → Deployment
   - Designated reviewers and approvers at each gate
   - Status tracking in the integration data store (RDF)

4. **Canonical Model Auto-Enhancement**: When a new ICD defines elements absent from the ESB canonical model, the system automatically expands the model. No manual schema migration.

5. **Semantic Mapping with Transformation Rules**:
   - Noun element (the field) + Verb element (the transformation)
   - Three-layer pattern: Consumer → ESB → Provider
   - Decoupling: services map to the canonical model, not to each other

6. **Impact Analysis via Graph Traversal**: Change a semantic element → trace all dependent mappings, ICDs, connected applications, downstream transformations. Prevents integration breakage.

7. **Versioning + Swimlanes**: Parallel development branches, conflict detection via XML comparison, merge resolution workflow.

8. **Visualization Module**: Graphical mapping chains (consumer → ESB → provider), mapped vs unmapped fields, impact zones, adjustable detail levels.

---

## Relevance to Chorus

This patent is Jeff's prior art for the Chorus execution engine. Same architectural pattern, different domain.

| Patent (Staples ESB) | Chorus (Team Pipeline) |
|---|---|
| RDF/OWL integration data store | Fuseki + Chorus ontology |
| SPARQL for querying semantic relationships | SPARQL ASK queries as gate logic |
| ICD workflow (Draft → Review → Approve → Deploy) | Pipeline (Directing → Designing → Building → Proving) |
| Approval gates with designated reviewers | Gates with gatekeepers (Jeff, Silas, Kade, Wren) |
| Semantic-level impact analysis via graph | Cross-graph joins (ADR-008) + fitness functions |
| Canonical model (ESB hub) | Chorus ontology (pipeline hub) |
| Auto-enhancement of canonical model | Ontology evolution as domains grow |
| Visualization module | Planned dashboard (Layer 6, Phase 3) |
| Versioning + swimlanes | Protocol versioning (team-architecture.md) |
| Transformation rules (noun + verb) | Gate criteria (criterion + pass/fail) |

### Key Architectural Insight (Shared)

The ontology is not documentation — it's the execution substrate. The data model and the process engine share the same semantic infrastructure. RDF/OWL defines the schema. SPARQL validates and queries. The workflow enforces gates. Everything is queryable, auditable, and evolvable without redeployment.

### What Chorus Adds Beyond the Patent

1. **Trust as emergent property** — gate pass rates compound into a trust score
2. **Bounce-back model** — gate failures return work with feedback (not just rejection)
3. **Content pipelines** — same engine runs domain lifecycles (Seed → Glimmer → Idea → Project), not just team coordination
4. **Fitness functions** — measurable protocol compliance beyond pass/fail gates

---

## Full Patent Claims (39 Claims)

The patent has three independent claim groups covering the same invention as a **method** (Claims 1-12, 37), **software** (Claims 13-24, 38), and **system** (Claims 25-36, 39). This triple coverage is the strongest form of utility patent protection.

### Independent Claim 1 (Method)

> A computer-implemented method comprising: determining, using one or more computing devices, a first semantic element of a first interface control document, the first semantic element representing a first operation included in a first application interface representing a first service, the first interface control document being a data set including properties of the first service; determining, using the one or more computing devices, a second semantic element of a second interface control document, the second semantic element representing a second operation included in a second application interface representing a second service, the second interface control document being a data set including properties of the second service, the second service including one of an enterprise service bus, and extract-transform-load framework, and a gateway; mapping, using the one or more computing devices, the first semantic element of the first interface control document with the second semantic element of the second interface control document; and integrating, using the one or more computing devices, the first application interface with the second service based on the mapping of the first semantic element of the first interface control document with the second semantic element of the second interface control document.

### Key Dependent Claims (Method Group)

**Claim 4** — Centralized repository: Receiving ICDs, storing them in a centralized repository, connecting semantic elements based on mappings. The canonical information model of the ESB is stored alongside service ICDs.

**Claim 5** — **Approval gates**: Obtaining approval of ICDs and/or mappings before integration executes. *This is the direct ancestor of Chorus gate evaluation.*

**Claim 6** — Visualization: Generating graphical illustration of semantic mappings for display.

**Claim 7-9** — Multi-service chaining: Consumer → ESB → Provider pattern. Third service integration via the canonical model. Visualization of end-to-end mapping chains.

**Claim 10-12** — Validation: Checking compatibility of semantic elements, verifying required elements are mapped, validating incorporated references, error notification to stakeholders.

**Claim 37** — Transformation rules: Mapping uses transformation rules (not just 1:1 field mapping).

### Independent Claim 13 (Software)

Mirror of Claim 1 as a computer program product (non-transitory computer readable medium). Dependent claims 14-24 and 38 mirror claims 2-12 and 37.

### Independent Claim 25 (System)

> A system comprising: one or more processors; a **definition module** executable by the one or more processors to determine semantic elements and mappings; a **data store** configured to store the mappings; and a **workflow module** executable by the one or more processors to integrate application interfaces based on the mappings, the workflow module coupled to the data store to access the mappings.

### Key Dependent Claims (System Group)

**Claim 29** — **Approval workflow**: Workflow module obtains approval before integration. System-level enforcement of gates.

**Claim 30** — **Visualization module**: Separate module for generating graphical mapping displays.

**Claim 31-33** — Multi-service integration with visualization.

**Claim 34-36** — Validation module with error detection and stakeholder notification.

**Claim 39** — Transformation rules at the system level.

### Claim Structure Summary

| Claim Range | Type | Covers |
|---|---|---|
| 1-12, 37 | Method | The process of semantic mapping + integration |
| 13-24, 38 | Software | The program that executes the method |
| 25-36, 39 | System | The architecture (definition module + data store + workflow module) |

### Claims Most Relevant to Chorus

| Claim | What It Protects | Chorus Parallel |
|---|---|---|
| 5, 17, 29 | **Approval gates** before execution | Gate evaluation with gatekeepers |
| 4, 16, 28 | **Centralized semantic repository** | Fuseki triple store + ontology |
| 10-12, 22-24, 34-36 | **Validation + error notification** | SHACL shapes + fitness functions |
| 6, 18, 30 | **Visualization of mappings** | Mind map, planned dashboard |
| 7-9, 19-21, 31-33 | **Multi-service chaining via canonical model** | Cross-graph SPARQL (ADR-008) |
| 37-39 | **Transformation rules** | Gate criteria with pass/fail logic |

---

## For Tomorrow's Walkthrough

This patent validates the Chorus execution layer architecture. Jeff isn't speculating — he's applying a proven, patented approach to a new domain. The walkthrough should:

1. Acknowledge the lineage: Staples patent → Gathering ontology → Chorus execution
2. Use the patent's vocabulary where it fits (semantic elements, canonical model, impact analysis)
3. Show where Chorus extends beyond the patent (trust flywheel, content pipelines, bounce-back)
4. Treat Jeff as domain expert, not stakeholder to convince

---

— Silas
