# Research: Ontology-Driven System Modernization (IBM / OMG)

**Date**: 2026-02-20
**Author**: Silas (Architect)
**Type**: Research — save for later reference
**Trigger**: Jeff remembers an IBM book (~2010-2020) about using ontologies to ingest existing systems and build new ones ("like the Borg")

---

## Best Match: OMG Architecture-Driven Modernization (ADM) + Knowledge Discovery Metamodel (KDM)

The OMG's ADM initiative, with IBM heavily involved through Rational tooling, defines a standard pattern:

1. **Reverse engineer** an existing system into the Knowledge Discovery Metamodel (KDM) — an ontology for software assets
2. KDM **absorbs everything**: code, data, UI, platform dependencies, relationships
3. The model becomes a **queryable representation** more useful than the source system
4. **Forward engineer** the replacement from the model

KDM became ISO/IEC 19506. It's language-agnostic, platform-agnostic — the ontology doesn't care what the source looks like. That's the "Borg" quality.

### Key sources
- [Architecture-Driven Modernization (Wikipedia)](https://en.wikipedia.org/wiki/Architecture-driven_modernization)
- [KDM ISO/IEC 19506 — standard for legacy modernization](https://www.sciencedirect.com/science/article/abs/pii/S0920548911000183)
- [KDM History (KDM Analytics)](https://kdmanalytics.com/resources/standards/kdm/knowledge-discovery-metamodel-history/)
- [ADM for Software Reverse Engineering (IGI Global)](https://www.igi-global.com/chapter/architecture-driven-modernization-software-reverse/78218)

---

## Other Candidates

### IBM Redpaper REDP-5081: "Foundational Ontologies for Smarter Industries"
- Reference ontologies for Industry 4.0 — observation data, sensor networks, assets, measurements
- Pattern: Monitor → Assess → Predict → Optimize using ontology-modeled data
- More industrial/IoT than software migration
- [IBM Redbooks abstract](https://www.redbooks.ibm.com/Redbooks.nsf/RedbookAbstracts/redp5081.html)

### IBM SOMA (Service-Oriented Modeling and Architecture)
- IBM's SOA methodology (~2004-2012)
- Includes "existing asset analysis" — systematically inventorying existing systems
- Goal-service modeling, domain decomposition, ontology-like catalog of what exists
- [SOMA method (IEEE)](https://ieeexplore.ieee.org/document/5386496/)

### IBM Rational System Architect
- Uses RDF triplestore + SPARQL for enterprise architecture dashboards
- Semantic inference to "connect the dots" across the enterprise
- OSLC (Open Services for Lifecycle Collaboration) standards for linking artifacts

---

## Connection to Our Work

The ADM/KDM pattern maps directly to what we're building:

| ADM/KDM Concept | Our Equivalent |
|-----------------|----------------|
| Existing system to absorb | Team interactions (Slack, sessions, briefs) |
| Knowledge Discovery Metamodel | Chorus ontology (chorus.ttl) |
| Reverse engineering | Conversation harvesting (/chorus, bridge indexing) |
| Queryable model | Structured logs, briefs, board state, chorus.log |
| Forward engineering | Using the model to improve coordination |
| Workflow gates | chorus-audit.sh, sensitive-paths hook |

### Two planes (Jeff's framing, 2026-02-20)

| Plane | Analogy | Structured today? |
|-------|---------|-------------------|
| **Action plane** (Claude sessions) | Like having KDM for source code | Yes — git, chorus.log, briefs, board |
| **Interaction plane** (Slack) | Like unmodeled legacy system | No — ephemeral, not indexed |

Jeff's proposed fix: build a harvester for the interaction plane. Incremental Slack indexer → structured store → custom Claude command (`/context` or `/chorus`) → role reconciles state on session start. Same reverse-engineer → model → act loop.

### Patent connection
Jeff's Staples patent (US9552400B2) — RDF/OWL + SPARQL + workflow gates — is the enterprise-scale version of this same pattern. Chorus is the personal/team-scale version.

---

## Open Questions (for future discussion)
- Does the KDM metamodel structure inform how we model conversations in chorus.ttl?
- Should the conversation harvester output RDF triples (queryable via SPARQL) or structured markdown (human-readable)?
- How does this connect to the Chorus product vision (team coordination as a product)?

---

*Saved as research. Jeff will discuss further when ready.*
