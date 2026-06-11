# ADR-040: Namespace + IRI-Formation Convention — One Rule Per Level

**Status:** Proposed — 2026-06-11 (Silas, SA/OWL-DBA). Pending: Jeff (final).
**Card:** #3256 — step 1 of the coherent-model program (convention → DAL → reconcile-OWLs → crawler → OWL→API).
**Consolidates:** ADR-020 (typing), ADR-025 (graph separation), ADR-031 (interface naming) + #1772 (URN inventory). Each retains its own domain; this ADR is the umbrella that states the IRI-formation rule at every level and names what is canonical where they touch.
**Resolves:** the #3242 impedance mismatch (`chorus:Proving` vs `chorus:value-stream-step-proving`).

## Context

OWL authoring is about to multiply: Kade's werk-domain OWL (#3340) is explicitly
generator input for owl-api; the DAL (#3257) must mint conformant IRIs; the
four existing OWL homes drift from each other today. The namespace question was
decided in fragments — ADR-020 typed product-vs-domain, ADR-025 split
schema-vs-content graphs, ADR-031 named the interface surface, #1772
inventoried the URN zoo (30.9M triples, 6 patterns) — but no document states
**how an IRI is formed**, which is why the #3242 slice authored
`chorus:Proving` while the live tree says `chorus:value-stream-step-proving`.

The live reality this ADR codifies (verified 2026-06-11 against
`data/athena/tree.json` — the v2 driver — and the running Fuseki):

| Surface | Live pattern | Example |
|---|---|---|
| v2 value-stream steps | type-prefixed kebab | `chorus:value-stream-step-proving` |
| v2 roles | type-prefixed kebab | `chorus:role-wren` |
| v2 services | type-prefixed kebab | `chorus:service-crawler` |
| v2 products | **bare** kebab | `chorus:loom`, `chorus:chorus` |
| v2 domains | **bare** kebab | `chorus:roles`, `chorus:principles` |
| v1 subdomains | type-**suffixed** | `chorus:build-domain` (49 nodes, live reads) |
| classes | CamelCase singular | `chorus:Product`, `chorus:ValueStreamStep` |
| graph names | urn zoo | `urn:chorus:*` ×10 (incl. `shapes`, absent from #1772's table), `urn:borg:*` ×3, `urn:gathering:*`, `urn:jb:*`, one stray `http://…` |

Three instance grains coexist. Generation amplifies whatever the model is —
so the model gets one rule per level **before** anything generates from it.

## Decision

Four levels. Each states its formation rule. The deepest tooth is stated
first because it makes the rest mechanical:

> **Rule 0 — IRIs are minted, never typed.** Authors hand the DAL
> `(type, name)`; the DAL forms the IRI from this convention. A hand-written
> IRI in authored TTL/JSON is reviewed as a smell. You cannot misname what
> you never name. (This is the inversion-of-control move: the convention
> lives in one mint, not in every author's memory.)
>
> **Interim protocol (this ADR landed, #3257 DAL not yet):** authors form IRIs
> by hand FROM the Level-3 table below and mark the file
> `conformance: ADR-040-manual` in its header comment — the reconcile gate at
> model-3 re-verifies everything minted during the gap. Kade's #3340
> `0.1.0-provisional-pre-3256` marker is the pattern.

### Level 1 — Graph names (WHERE triples live): `urn:`

```
urn:<product>:<collection>[/<qualifier>]     all lowercase kebab
```

- Canonical roots: `urn:chorus:`, `urn:gathering:`, `urn:borg:`. One URN root
  per product (Borg is a product in the settled seven — `urn:borg:` stays,
  resolving #1772's open DECIDE).
- Graph names are **locations**, not identities — they never appear as
  subjects/objects inside triples.
- Schema-vs-content split per ADR-025 (retained intact): `urn:chorus:ontology`
  = classes/properties/axioms/SHACL only; `urn:chorus:instances` = all content.
  The live `urn:chorus:shapes` graph (60 triples) is nonconforming with
  ADR-025's "shapes live in ontology" — flagged for the model-3 reconcile:
  merge into `urn:chorus:ontology` or amend ADR-025; not silently both.
- Migrations stay #1772's (unchanged by this ADR): `urn:jb:*` → `urn:gathering:*`;
  the stray `http://jeffbridwell.com/gathering/icd/current` graph name → `urn:gathering:icd/current`;
  `urn:jb/` pod graphs are SOLID-internal, not ours to rename.

### Level 2 — Entity namespace (WHO an entity is): `https://`

```
https://jeffbridwell.com/chorus#        prefix chorus:   (the Chorus model)
https://jeffbridwell.com/ontology#      prefix jb:       (the personal/Gathering model)
```

- **Identity is https, location is urn.** An entity IRI never starts with
  `urn:`; a graph name never starts with `https://`.
- One namespace per model. New Chorus-model entities — including Kade's werk
  domains (#3340) — go in `chorus:`. No per-subproduct namespaces (no
  `borg:` entity namespace for new work; the existing `urn:borg:` graphs hold
  instances whose entity-IRI migration is model-3 scope).

### Level 3 — Instance naming grain (HOW an instance is spelled)

```
chorus:<kebab-name>              Product and Domain ONLY   (the spine entities)
chorus:<type>-<kebab-name>       every other typed instance
```

- Products and domains are bare because that is the live v2 driver and they
  are the few-dozen governed spine entities whose names are unique by
  curation (`chorus:loom`, `chorus:principles`). Everything else carries its
  type: `chorus:role-wren`, `chorus:value-stream-step-proving`,
  `chorus:service-crawler`, `chorus:principle-be-direct`,
  `chorus:practice-…`, `chorus:gate-…`, `chorus:decision-…`.
- The type prefix is the class name, kebab-cased, singular
  (`ValueStreamStep` → `value-stream-step-`).
- **#3242 resolved explicitly:** `chorus:Proving` was wrong on two axes —
  CamelCase is class grain, and steps are type-prefixed. Canonical:
  `chorus:value-stream-step-proving`. Per-entity-kind table: role →
  `chorus:role-<name>`; value-stream-step → `chorus:value-stream-step-<name>`;
  service → `chorus:service-<name>`; principle/practice/policy/skill/gate/
  decision/document → `chorus:<type>-<name>`; product/domain → `chorus:<name>`.
- v1's type-**suffix** grain (`build-domain`, 49 live subdomains) is **legacy,
  read-compatible, write-frozen**: nothing new is minted with a suffix; the
  v1→v2 restructure (the settled refactor-not-migrate path) renames them at
  its own pace. The DAL refuses suffix-grain on writes from day one.

### Level 4 — Class vs instance vs property

```
chorus:CamelCaseSingular         class        (chorus:ValueStreamStep)
chorus:camelCase                 property     (chorus:ownedBy, chorus:atStep)
chorus:kebab-lower               instance     (per Level 3)
```

- Casing alone distinguishes the three at a glance and is machine-checkable
  by regex. Classes and properties are declared only in `urn:chorus:ontology`;
  instances live only in `urn:chorus:instances` (ADR-025).
- Class vocabulary of record is the Athena-design OWL's eleven
  (Product, Domain, Service, Role, ValueStream, ValueStreamStep, Document,
  Principle, Practice, Infrastructure, Store) — `ValueStreamStep`, not
  `Phase`/`Vertebra`; the older OWLs rename at model-3.

## What each prior decision contributed / what is canonical now

| Source | Keeps | This ADR adds |
|---|---|---|
| ADR-020 | the SubProduct-vs-SubDomain typing test | the *spelling* of whichever type wins |
| ADR-025 | schema/content graph split, URIs-survive-migration | graph-name grammar; the `shapes` graph flag |
| ADR-031 | interface naming (`chorus_<resource>_<verb>`, REST paths) | nothing — interfaces ≠ model IRIs; both cite #3114 for teeth |
| #1772 | the migration inventory and targets | the grammar migrations converge TO |

## Enforcement spec (the teeth — gate/DAL contract, built by later cards)

The DAL (#3257) and the conformance gate MUST check, in order:

1. **Mint-only writes:** the write API accepts `(type, name, fields)` — never
   a caller-supplied IRI. The mint applies Level 3. (Kills the class.)
2. **Entity-IRI regex per declared type:**
   `^https://jeffbridwell\.com/chorus#([a-z0-9]+(-[a-z0-9]+)*)$` for
   Product/Domain; `^…#<type>-[a-z0-9-]+$` for all else, `<type>` =
   kebab of the rdf:type's local name. Reject on mismatch — refusal, not warn.
3. **Casing routing:** CamelCase subjects (class/property declarations) may
   only be written to `urn:chorus:ontology`; kebab instances only to
   `urn:chorus:instances`. Cross-writes refused (enforces ADR-025 at write
   time instead of by audit).
4. **Graph-name grammar:** new graphs must match
   `^urn:(chorus|gathering|borg):[a-z][a-z0-9/-]*$`.
5. **Write-freeze on legacy grains:** `-domain` suffix and `urn:jb:` targets
   refused with a pointer to this ADR.
6. Interface names stay ADR-031's lane; one CI name-test (#3114) carries both
   specs so there is a single teeth-card, not two.

## Consequences

- Kade's #3340 werk-OWL mints its domain IRIs from Level 3 on day one — the
  generator input is conformant by construction, which is the entire point
  of sequencing this ADR first.
- The DAL's "IRI-from-convention" AC stops being aspirational — its input
  contract is Rule 0 + the Level-3 table.
- The four OWL homes get their reconcile target (model-3): one class
  vocabulary, one entity namespace, instances re-minted only where they
  violate Level 3 (most already conform).
- `chorus:Proving`-class mismatches become impossible to land, not impossible
  to write — authors can still draft anything; the DAL and gate refuse it.
