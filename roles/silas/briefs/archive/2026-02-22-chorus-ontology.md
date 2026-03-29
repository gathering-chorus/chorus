# Brief: Chorus Ontology — Formalize What's Already There

**From:** Wren
**To:** Silas
**Date:** 2026-02-22
**Card:** C#40

## Context

Jeff and I were reviewing your nervous system visualization. It's beautiful — but the connections are drawn without defined relationships. Everything goes "→ spine" without distinguishing which vertebra, how, or why. Jeff asked the right question: "isn't this an ontology consideration?"

Yes. Chorus needs its own ontology — separate from Gathering's (which describes collections, domains, Self). Chorus describes how the team operates.

## What Already Exists (Implicit)

These entities are scattered across scripts, manifests, CLAUDE.md files, and conventions:

| Entity | Where it lives today |
|--------|---------------------|
| Role | CLAUDE.md files, participants.ts |
| Decision | decisions.md, workflow history |
| Workflow | WF-NNN.json manifests |
| Step | Inside workflow manifests |
| Brief | Markdown files in role briefs/ dirs |
| Tool | Skills (/look, /werk, /clearing, /chorus), scripts |
| Vertebra | Value stream doc, spine visualization |
| Artifact | Code, docs, dashboards — untyped |

And implicit relationships:

- Role **operates-at** Vertebra (Wren→Directing/Designing, Kade→Building/Proving, Silas→Designing/Building)
- Tool **feeds** Vertebra (/look,/listen→Capturing, /clearing→Directing, /werk→all)
- Decision **creates** Workflow
- Workflow **sequences** Steps
- Step **assigned-to** Role
- Step **produces** Artifact
- Brief **hands-off-to** Role
- /chorus **remembers** all of the above
- Gathering is **output-of** the spine AND **input-to** Capturing (feedback loop)

## What I Need From You

Formalize this. Not a Gathering-sized effort — small, tight, derived from what already exists. The goal:

1. Named entities with properties
2. Named relationships with cardinality
3. The viz becomes a **view** of the ontology, not a freehand drawing
4. Connections in the viz are derived from relationship definitions, not hand-coded

## Constraint

Jeff also noted: Gathering should appear on the viz as both output (from Building/Proving) and input (back to Capturing). And /werk should connect to the vertebrae it orchestrates, not just float.

## Not Asking For

- RDF/OWL formalization (yet)
- Schema changes to existing systems
- Migration of anything

Just: name it, type it, draw the relationships. We'll know it's right when the viz connections can be generated from the ontology instead of hand-coded.
