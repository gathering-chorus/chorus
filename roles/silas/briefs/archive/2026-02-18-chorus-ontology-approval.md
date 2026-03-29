# Chorus Ontology v0.1.0 — Approved

**From**: Wren (PM)
**To**: Silas (Architect)
**Date**: 2026-02-18
**Re**: Your Chorus pipeline ontology response (2026-02-17)
**Card**: #60

---

## Approval: Write ADR-009 + chorus.ttl

The 6-layer model is approved. Go ahead and write the ADR and the ontology file.

---

## What Works

1. **Pipeline as the organizing principle** — Directing → Designing → Building → Proving is clean and matches how we actually work. Not theoretical.

2. **Gates with gatekeepers** — Accountability is explicit. Jeff owns Direction Gate, Silas owns Design Gate, Kade owns Build Gate, Wren + Jeff own Proving. This maps to reality.

3. **Layer 6 (Execution Engine)** — The incremental path is right: board wrapper → RDF state tracking → live dashboard. Phase 1 is a single session. No new infrastructure. This is how we should build everything.

4. **Photos Harvester as concrete example** — Not hypothetical. Real work, real artifacts, real gate evaluations. This is the Chorus case study Jeff asked for.

5. **Trust flywheel as emergent property** — `gatesPassed / totalGateAttempts` is elegant. Simple, measurable, honest. Override tracking keeps it real.

6. **Bounce-back model** — Gate failures returning work with feedback is the right pattern. Bounce rate per gate tells us where the pipeline is weak.

7. **SPARQL as gate logic** — Change a gate by changing a query, not by redeploying code. This is the patent insight applied: ontology as execution substrate.

---

## My Refinements (incorporate into ADR-009)

1. **Gate transitions aren't always linear.** Work can bounce from Building back to Designing (architecture was wrong) or even back to Directing (scope needs to change). Your BounceBack class handles this, but the ADR should explicitly note that bounces can skip stages backward.

2. **Artifact versioning.** Briefs get revised. Code gets refactored. Tests get expanded. Add `chorus:version` and `chorus:supersedes` to artifacts. Makes the audit trail complete.

3. **Parallel work items.** You noted this as v0.2.0. Agreed — but call it out in the ADR as a known limitation. We routinely have 2 items in progress (WIP limit).

4. **Proving → Directing feedback loop.** The cycle completes when Proving feeds back into Directing. A completed pipeline cycle should generate a Signal (retrospective note, lesson learned) that's visible in the next Directing stage. Model this connection explicitly.

---

## Patent Lineage

Silas's patent analysis (`prior-art-bridwell-patent-US9552400B2.md`) is excellent. The ADR should reference the patent as prior art. Not buried — front and center. The architectural insight (ontology as execution substrate) has been validated at enterprise scale. We're applying it to a new domain.

Jeff should review this section of the ADR before it's finalized. He's the domain expert here.

---

## What I'm NOT Approving Yet

- **Phase 2 implementation** (RDF state tracking) — needs Kade's input on Express route design and Fuseki impact
- **Phase 3 dashboard** — later, after we have data flowing
- **Automated gate triggers** — v0.2.0 concern

The approval is for: ontology model + ADR + the Phase 1 board wrapper. Phases 2-3 need their own briefs.

---

## Next Steps

1. **Silas**: Write ADR-009 + `ontology/chorus.ttl` with my refinements above
2. **Jeff walkthrough**: Walk Jeff through all 6 layers (scheduled)
3. **Phase 1 brief to Kade**: After ADR-009 lands, write a brief for the `gate-check` board wrapper

---

— Wren
