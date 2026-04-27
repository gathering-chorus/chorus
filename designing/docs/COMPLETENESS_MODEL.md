# System Completeness Model

Every slice of work delivers against these dimensions. Not all apply to every slice, but the ones that do must be measurable. No dimension should regress when a slice ships.

This is technology-agnostic and language-agnostic. It describes what a complete system looks like, not how to build one.

## The Twelve Dimensions

### 1. Graph
**Question:** Do we know what connects to what?

The system maintains a map of its own structure — files, services, data stores, interfaces, and the dependencies between them. A change to any node should reveal its connections before work begins.

**Fitness test:** Coverage — % of real dependencies captured vs actual. Backtest against git history: predicted blast radius vs files actually changed.

### 2. Semantics
**Question:** Do we have a shared model for what things mean?

Every concept in the system has one definition. Sender, receiver, and canonical all agree. No ambiguity between what a human says and what the system understands.

**Fitness test:** Zero ambiguous terms. Every entity, relationship, and property in the ontology has a single meaning. Synonyms are mapped, not tolerated.

### 3. Data
**Question:** Is the data present, linked, and grouped correctly?

Records exist where they should. Relationships between records are explicit, not implied. Grouping (album → tracks, collection → items) is complete and consistent.

**Fitness test:** Completeness — expected record count vs actual. Zero orphans (records with no parent). Zero duplicates (same entity, different identities).

### 4. Search
**Question:** Can you find anything by meaning, not just keyword?

Both keyword and semantic search return relevant results. The system searches across all memory layers (structured data, logs, text, embeddings) without the user knowing which store holds what.

**Fitness test:** Recall — % of relevant results returned for a known query set. Precision — % of returned results that are actually relevant. Both keyword and semantic paths tested independently.

### 5. Latency
**Question:** Is the system fast enough to feel immediate?

Speed is a non-functional that kills adoption when wrong. Users should never wait for the system to catch up with their thinking.

**Fitness test:** Sub-second for data queries and search. Sub-5 seconds for interactive skills and UI responses. Sub-2 seconds for page loads. Measured continuously, not sampled.

### 6. Coherence
**Question:** Do the parts agree with each other?

Data in one store matches data in another. A track count in the filesystem matches the count in the database matches the count in search. State declared by one component is consistent with state observed by another.

**Fitness test:** Zero orphans (data without parents). Zero broken links (references to things that don't exist). Zero drift (stores that disagree on the same fact). Cross-store reconciliation runs automatically.

### 7. Observability
**Question:** Can the system see itself?

Every state change emits a signal. Every service reports health. Every error is captured, classified, and routed. The system's internal state is always knowable without manual inspection.

**Fitness test:** Every state transition produces a structured event. Health checks cover all services. Error classification is automatic. No silent failures — if something breaks, something else notices.

### 8. Security
**Question:** Is data where it belongs and nowhere else?

Concentric trust boundaries are enforced. Personal data stays local. Shared data stays within the team boundary. Public data is explicitly marked. No accidental leakage across boundaries.

**Fitness test:** Concentric trust boundaries verified — data at each ring cannot escape to an outer ring without explicit action. Credentials never appear in logs, commits, or shared files.

### 9. Tests
**Question:** Does the system prove itself?

Acceptance criteria are verified automatically, not manually. The test suite covers the behaviors that matter, not just the code that's easy to test. Tests run on every change.

**Fitness test:** AC coverage — % of acceptance criteria with automated verification. Test suite passes on every commit. No self-accepted work for code changes.

### 10. Harvest
**Question:** Is external data flowing in correctly?

Data pipelines bring external content into the system on schedule. Manifests govern what's harvested, how often, and what completeness looks like. Staleness is measured and alerted.

**Fitness test:** Pipeline freshness — every domain's harvest is within its staleness threshold. Manifest completeness — expected items vs harvested items. Zero gaps in pipeline coverage.

### 11. UX
**Question:** Can a human use it without friction?

The interface is consistent, fast, and uses a shared vocabulary. Components follow the style guide. Pages load quickly. The system adapts to the user's energy and context, not the other way around.

**Fitness test:** Style guide compliance — zero inline styles, all components use standard classes. Page load under 2 seconds. Accessibility basics met. Vocabulary matches what the user says (see STYLE_GUIDE.md).

### 12. Documentation
**Question:** Does the system explain itself?

Every decision, pattern, and interface is documented. Documentation stays current with the code. The system can narrate its own structure (see Graph dimension). Stale docs are detected and flagged.

**Fitness test:** Doc-drift — zero stale documents (detected automatically). Every public API has documentation. Every decision has rationale recorded. The codebase graph can explain any node.

## How to Use This

### For a slice (Borg Phase 2)
Before building, identify which dimensions this slice touches. After shipping, score each touched dimension: did it go up, stay flat, or regress? Regression is a defect.

### For a release
Score all twelve dimensions. The lowest score is the system's true completeness. Invest next in the weakest dimension, not the most interesting one.

### For fitness functions
Each dimension's fitness test can be automated. When automated, it runs continuously. The system monitors its own completeness and alerts when a dimension degrades.

## Lineage

- **Informix CASE tool (1991)** — generate UIs from models
- **Athena (Staples)** — OWL/RDF interface control documents, sender/canonical/receiver
- **Spot (Staples)** — execution tracking on 50-100 concurrent integration projects
- **Poppendieck** — lean integration, eliminate waste in handoffs, build quality into process
- **US9552400B2** — patent: defining and mapping application interface semantics

The lesson across all of these: semantic rigor works, but process weight kills adoption. This model is the rigor. The fitness functions are the automation. The human carries zero weight.
