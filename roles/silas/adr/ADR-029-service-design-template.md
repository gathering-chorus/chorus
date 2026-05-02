# ADR-029: Service Design Document — Mandatory Template

**Status:** Accepted
**Date:** 2026-05-02
**Author:** Silas

## Context

Service-design documents in this repo (cards, commits, security, alerts, gates, ci-pipeline, nudge, etc.) drift in shape and content. Every author invents their own structure, which produces:

- Provider-shape docs that bury or omit consumers (the people who actually call the service)
- Validation rules described in prose with no enforced contract
- Breaking changes that ship without naming who they break
- Inconsistent vocabulary across docs ("substrate" / "shape" / "modes" instead of plain technical terms)

Result: docs that read as architecture autobiographies rather than engineering contracts.

## Decision

Every service-design document MUST follow the template below. Sections marked **MUST** are required. Sections marked **MAY** are optional but stay in the same order when present.

The document file is hand-authored HTML at `designing/docs/<service>-service-design.html` (no `.md` companion — see Wren's 2026-05-02 retirement of the parallel `.md` for the cards doc; the `.html` is the canonical source).

### Mandatory sections, in order

1. **Title + metadata** — service name, author, date, parent card, status.
2. **Promise** (MUST) — one paragraph. What the service delivers when complete, in terms a caller cares about. Not implementation. Not provider state.
3. **Consumers** (MUST) — table or list of every entity that calls this service. For each consumer:
   - Name (script / skill / role / external system)
   - What they call (verb + entity)
   - What they expect back (success and failure shapes)
   - Where their call originates in code (file:line if internal)
4. **Contract per consumer** (MUST) — for each consumer in §3, the explicit contract:
   - Input shape (typed, declarative — point at the schema artifact)
   - Output shape on success (typed)
   - Output shape on each failure mode (typed errors, structured fields, no stringly errors)
   - Side effects (what changes in the world when the call succeeds)
5. **Validation** (MUST) — single paragraph naming where contract validation fires. Must answer: which boundary validates against the schema before any side effect; what happens on validation failure (must be: typed error, non-zero exit / non-2xx response, audit event); what does NOT exist (no log-and-continue, no default-permissive, no silent-fail).
6. **Failure modes** (MUST) — every refusal class the service emits. Maps 1:1 to the typed errors named in §4. No floating failure modes that aren't in §4.
7. **As-Is** (MUST) — current state of the service. Diagram + 1-paragraph description. Honest about gaps.
8. **To-Be** (MUST) — target state. Diagram + 1-paragraph description. Names the gap closure.
9. **Migration plan** (MUST when there's a behavior change) — for each consumer in §3 affected by the change:
   - What breaks
   - When it breaks (date or trigger)
   - How that consumer is migrated (in this change, not as a follow-on)
   - Retirement criterion for any deprecated path (calendar date or telemetry-driven)
   - Per ADR-028 Addendum 2: if the change introduces a parallel path, name the retirement signal that closes the parallel — without it, the change is not end-to-end-whole.
10. **Files in scope** (MUST) — exact paths the design changes or governs. No prose; just the list.
11. **References** (MUST) — cards, ADRs, peer service designs, prior decisions cited. No inline jargon coining new terms; if a concept needs a name, point at the existing ADR/DEC that defined it.
12. **Open questions** (MAY) — explicitly deferred decisions. Each names the trigger that would close it.
13. **Out of scope** (MAY) — what the design deliberately does not address.

### Mandatory diagrams

Every service-design MUST include:

1. **Consumer interaction diagram** — Mermaid `flowchart` or equivalent. Every consumer from §3 is a node. Every call from §4 is a labeled edge. Direction = call direction (consumer → service). This diagram is the consumer surface at a glance.

2. **As-Is and To-Be diagrams** — separate Mermaid `flowchart`s, one for §7 and one for §8. Same node vocabulary as the consumer interaction diagram (don't introduce new boxes that don't appear elsewhere).

3. **Sequence diagram for the critical path** — Mermaid `sequenceDiagram` for the most-called consumer flow. Includes the validation step, the side effect, and at least one failure path returning a typed error.

Diagrams MUST render in browser (mermaid loader script in `<head>` per the team's HTML convention — see ci-pipeline-service-design.html for the loader pattern).

### Mandatory writing rules

- **Plain technical terms.** No "substrate," "shape," "modes," "Class A/B," "invariant-vs-norm," "load-bearing primitive" unless cited from an existing ADR/DEC where the term is defined. Default to industry-standard names (API, contract, validation, deprecation, sunset, schema, error type) for industry-standard concepts.
- **Consumers before providers.** §3-§6 (consumers + contracts + validation + failure modes) come before §7-§10 (provider state + migration + files). The doc reads as a contract first; the implementation is how we honor it.
- **No follow-on cards as deferral.** If a section names a gap, the gap must be closed in this design's scope OR named in §13 (Out of scope) with explicit rationale. "Filed as #NNNN, will address later" is not acceptable for in-scope work — it's the pattern this ADR retires.
- **Cite, don't paraphrase.** When referencing principles, ADRs, or DECs, cite by ID and inline the relevant clause in the same sentence. Don't reframe the cited concept in new vocabulary.

## Consequences

### Enables

- Reviewable docs: a reader can scan §3-§6 and know what the service is for and how it fails.
- Mergeable docs: every service-design has the same shape, so reviewers know where to look for each type of question.
- Reduced jargon: the writing rules force authors to use known terms or cite where new ones are defined.
- End-to-end-whole completion: §9 names every consumer affected by a breaking change in the same scope.

### Costs

- More structure per doc (currently each author chooses).
- HTML-canonical means authors can't draft in markdown without a manual export step (mitigated: simple HTML template provided).
- Existing service-design docs that don't conform need a retrofit pass — see Compliance below.

## Compliance

Existing service-design docs are graded against this template:

- **Conforming or close**: cards-service-design.html (post-2026-05-02 four-lens walk), security-service-design.html (modulo §3 consumer list pending).
- **Needs retrofit**: commits-service-design.html, ci-pipeline-service-design.html, gate-set-service-design.md (still .md), nudge-service-design.md, pulse-service-design.md, borg-service-design.md, context-service-design.md, roles-service-design.md, services-service-design.md.

Retrofit is per-doc opt-in over time, not a single-PR migration. New service-design docs MUST conform from creation.

## References

- ADR-025 — ontology vs instances graph separation
- ADR-028 — substrate-class domain contract (Addenda 1 + 2)
- DEC-022 — operations responsibilities
- Wren cards-service-design.html (2026-05-02) — first conforming example
- Kade commits-service-design.html (v3) — reference for rich custom HTML structure (palette + flow rows)
- Jeff direction 2026-05-02: "service designs literally take the consumers out of scope"; "use honest technical terms rather than making shit up"
