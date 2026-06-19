# ADR-045: A Domain is an `owl:Class` — the unit owl-api generates from

**Status:** **Accepted** — 2026-06-18 (Silas, SA/OWL-DBA). **Ratified off the working surface, not the doc:** the punned `chorus:Properties` domain-class (#3489) drives owl-api's `definesVocabulary` fan-out (#3494) to compose `/borg/properties`, serving Property + PropertyKey CRUD end-to-end — verified live (Property=2, PropertyKey=3 on the generated surface). Proposed driver: the properties-domain gap surfaced live in the model viewer; Jeff: *"a domain is an owl class in our world"* and *"that is how owl-api generates the domain."*
**Builds on:** ADR-040 (IRI formation), ADR-041 (repo tree: ValueStream → Products → Domains), ADR-044 (PropertyKey governance).
**Worked example / first instance:** the `properties` domain (#3489) → generate `/borg/properties` (after Wren's #3488 `repoTarget`).

## Context

The config-as-data work modeled the *machinery* — `chorus:Property` (#3433) and `chorus:PropertyKey` (ADR-044) as classes — but never modeled **`properties` as a domain**. The result: `owl-api /domains/properties` is empty, because there is no domain for the generator to project from. The model viewer made this visible — `Property`/`PropertyKey` showed up as free-floating classes with no domain around them.

The fix isn't viewer-side. It's that **owl-api generates a domain API from a domain-as-a-class** (the #3350/#3466 Domain generator): it reads a domain's classes, datatype/object properties, SHACL constraints, and auth annotations, and projects reads + writes + security + validation + OpenAPI + MCP + page. A domain that isn't modeled as a class is invisible to that contract. So this ADR documents the convention the generator already embodies, and corrects the as-is model that violates it.

## Decision

### 1. A Domain is an `owl:Class`
A domain (`properties`, `tests`, `cards`, …) is modeled as an `owl:Class` — the unit owl-api generates from. Its data are **individuals of it** ("the domain OWL is the spec"). A domain is NOT a free-floating individual of `chorus:Domain` with no class identity.

### 2. Punning reconciles "class" with "registry/tree entry"
A domain IRI plays **two roles under one IRI** (OWL 2 DL punning):
- an **`owl:Class`** — so its content classes/individuals are typed by it and the generator can project it;
- an **individual of `chorus:Domain`** — so it sits in ADR-041's tree (`ValueStream → Product → Domain → Service`) and carries `ownedBy`, `atStep`, status, etc.

`chorus:Domain` remains the class-of-domains (the metaclass); each domain is punned. This makes "domain = class" and "domain in the registry/tree" both true at once — no contradiction, no metamodel hack beyond standard punning.

### 3. Member-vocabulary binds via `chorus:contains` (aligns ADR-041)
A domain's vocabulary binds to it with **`chorus:contains`** — the content-membership edge ADR-041 already defines (`:properties chorus:contains chorus:Property, chorus:PropertyKey, …`). The generator enumerates a domain's surface by `?domain chorus:contains ?class`. (Rejected: `rdfs:subClassOf` — a `Property` is not a *kind of* domain; namespace-only membership — implicit and unqueryable. `contains` is explicit, queryable, and reuses the existing edge — no competing implementation.)

### 4. As-is correction
Today domains are treated as plain instances (`Product hasDomain → Domain` individual, no class identity). This ADR sets the convention and the migration: each domain becomes a punned `owl:Class` + `chorus:Domain` individual, with `chorus:contains` to its vocabulary. v1 stays markable as deprecated (the v1/v2 filter / #3483 cleanup).

### 5. The generated surface (why this is the lever, not paperwork)
From a properly-modeled domain-class + its vocabulary + SHACL constraints + auth annotations, owl-api projects the **full domain surface**, consistency-by-construction:
- **reads + writes** (CRUD) — #3454 write endpoints;
- **security** — model-driven auth (`chorus:requiresAuth` on the write shapes, **fail-closed** — ADR/#3414, the fail-open default Silas caught);
- **write-validation** — the PropertyKey registry's constraints (register-before-use, `sh:in`/`sh:class`/`sh:xone`, ADR-044) **become the generated write-gate**; #3436's validate-on-write is *projected from the model*, not a separate hand-build;
- **OpenAPI + MCP tools + page** — generated.

The richer the domain OWL (vocabulary, constraints, auth), the more the generator emits for free. Governance authored in the model *becomes* generated behavior.

## Consequences

- **Reframes #3489**: the PropertyKey shapes are correct, but they are *vocabulary `chorus:contains`-ed by the `properties` domain-class*. Step 1 is modeling `properties` as a domain-class (this ADR); the shapes are its content.
- **Gates `/borg/properties`**: model `properties` to this convention → after Wren's #3488 (`repoTarget` + regen-safety) → owl-api generates `/borg/properties` (reads/writes/security/validation/MCP/page), landed in the repo.
- **Applies to every domain**: this is the convention the #3466 fan-out (the 30+ domains) generates from — properties is just the first worked instance.
- **#3436 folds into generation**: write-validation is projected from the registry, not separately built (the DAL/generator split is the impl detail; the *source* is one model).

## Open for review
- The `chorus:contains` binding (§3) vs an explicit `chorus:inDomain` — I recommend `contains` (reuses ADR-041); flag if the generator needs a dedicated edge.
- The punning convention (§2) — confirm it's acceptable in our OWL 2 DL profile (it is; reasoners we use tolerate punning).
