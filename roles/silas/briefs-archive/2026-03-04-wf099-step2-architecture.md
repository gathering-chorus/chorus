# Brief: WF-099 Step 2 — Conceptual Architecture

**From:** Wren | **To:** Silas | **Workflow:** WF-099 | **Card:** #947

## Context

Step 1 is done. I wrote `SYSTEM_MODEL.md` at `jeff-bridwell-personal-site/data/about/SYSTEM_MODEL.md`. It defines:

- **The cycle:** Ideate → Think → Reflect → Build → Borg (convergence) → back to Ideate
- **Three concentric layers:** Self (inner, local) → Gathering (middle, hybrid) → Chorus (outer, cloud)
- **Borg:** Not just card absorption — the convergence engine across the whole system. Instrumenting normalized into a system, including the Jeff system.

## Your Task

Take the value model and draw the architecture. Not infrastructure — the conceptual model of how the pieces connect.

### Deliverables

1. **Architecture document** in `architect/docs/` — how Gathering, Chorus, and Self connect as one system
2. **Data flow diagram** — how information moves between the four modes (ideate/think/reflect/build) and across the three layers
3. **Borg architecture** — where does convergence detection live? What does it observe? How does it feed back into the cycle?
4. **What exists vs what's missing** — map current services/surfaces to the model, identify structural gaps

### Questions to Answer

1. How do the data flows between layers actually work? What APIs, what protocols?
2. Where does Borg's convergence detection live — service, cron, LaunchAgent, or embedded in existing tools?
3. How does Self's read-only Chorus access work technically (relates to #939)?
4. What's the architecture of the capture channel — one intake point, many sources?
5. How does the cross-domain connection ratio get measured and surfaced?

### Key References

- `jeff-bridwell-personal-site/data/about/SYSTEM_MODEL.md` (Step 1 output — read this first)
- `jeff-bridwell-personal-site/data/about/PRODUCT_VISION.md`
- `jeff-bridwell-personal-site/data/about/OWNER_PERSONA.md` (Borg quote, line ~73)
- DEC-043 (three surfaces), DEC-068 (Self partition)
- Patent US9552400B2 (Jeff's prior art — RDF/OWL + SPARQL + workflow gates)

## After You

When Step 2 is done, brief Kade for Step 3 — prototype one piece of the model as a visible surface in the app.
