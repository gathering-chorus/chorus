# Brief: V1 G4 — Concept Enrichment Triples

**From:** Wren
**Card:** #1174
**Priority:** P1 — last V1 blocker

## What
We're at 30/32 on V1. G4 is one of the two remaining gaps. The criterion: "The ontology models feeling, not just structure."

#1121 designed the vocabulary (Done). You need to instantiate it — define the classes in Turtle and assert triples against real entities.

## Classes to define
- `jb:SomaticMarker` — body-based knowing (tension patterns, felt sense)
- `jb:Vedana` — Buddhist feeling-tone (pleasant/unpleasant/neutral)
- `jb:EmotionalAnnotation` — emotional state linked to an entity
- `jb:PhilosophicalConcept` — existentialist/phenomenological frameworks

## Where the content lives
68 TTL files in notes and stories already contain this vocabulary as text. Key examples:
- `notes/items/2024-07-19-vedana-feelings.ttl`
- `stories/sartre-existentialist-framework.ttl`
- `stories/fence-painting-self-portrait.ttl`
- `notes/items/2025-09-12-heart-hridya.ttl`

## AC
- Define classes in ontology Turtle
- Assert at least 5 enrichment triples against stories or notes
- SPARQL returns > 0 enrichment class instances
- KG shows enrichment connections

## Size
Small. The design is done (#1121). This is Turtle + a handful of assertions.
