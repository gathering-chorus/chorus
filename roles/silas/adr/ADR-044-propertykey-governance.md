# ADR-044: PropertyKey Governance — the config-as-data registry

**Date:** 2026-06-17
**Status:** Proposed
**Deciders:** Jeff (decision), Silas (architect/author), Kade (navigator — tests-domain consumer, #3473)
**Builds on:** **ADR-041 (the value-stream → product → domain taxonomy this registers config onto — NOT re-derived here)**, ADR-040 (IRI formation), #3433 (Property model), #3435 (effective-config read), #3437 (cascade resolver)
**First instance:** tests-domain `testType` / `:covers` / gate toggles (#3473, #3436)

This is the GENERIC governance standard for the whole properties program. The tests domain is the first instance, not the scope; the tests-specific ADRs reference this one.

## Context

Config-as-data (#3433) made a config change a *model write* instead of a code change: a `chorus:Property` (`propertyKey` / `propertyValue` / `propertyValueType`) attaches via `hasProperty` to any structural node and is read by generic engines (#3435).

But the Property model as shipped is **ungoverned**. `propertyKey`, `propertyValue`, and `propertyValueType` are all free `xsd:string`; `PropertyShape` asserts only that key/value/scope are *present*. It does not constrain `propertyValueType` to the five types, does not bind a key to allowed values, does not say which class a key may attach to, and has no lifecycle. Any string is a key; any value passes.

That is not config-as-data — it is **scattered JSON re-centralized, now wearing the authority of the model** (Kade, #3473). It is worse than env-scatter because it *looks* governed. This ADR adds the missing governance: a **PropertyKey registry** with **register-before-use**.

### Acceptance — the three shapes (Kade's consumer test, verified against real triples)
The registry is acceptable only if it expresses all three:
1. **enum-scalar** — `testType` (string, fixed enum, attaches to `Test`).
2. **lifecycle-toggle** — a flag whose values may expire under a card + TTL.
3. **typed-edge** — `:covers`, whose value is an IRI to a `Domain` (the constraint is `sh:class` on a node, not an enum on a literal; a "sub-domain" is a `Domain` that is `hasChild` of another — not its own class, per ADR-040).

A value-only row expresses #1 and #2 but **cannot** express #3. That gap drives the central decision below.

## Grounding — the level taxonomy is ADR-041's, not re-derived here

The levels a key registers *to* are already decided in **ADR-041 (Accepted)**: the value-stream steps (shaping / designing / directing / building / proving) → Products → Domains, with structural recursion via `hasChild` (product→product, domain→domain), content membership via `contains`, and step-recursion via `hasValueStream`. `chorus:Service` is a class in that model; a Property attaches to any of these via `hasProperty` (#3433's domain union: ValueStream, ValueStreamStep, Product, Domain, SubDomain, Service).

This ADR does **not** introduce or re-decide those levels — that would be a competing taxonomy (`principle-no-competing-implementations`). It adds one thing: governance for registering config keys *onto* the ADR-041 tree.

- **`appliesToClass`** on a PropertyKey is a pointer **into ADR-041's class tree** — it names the level a key is valid at. Choosing that level is the primary governance act (Jeff, 2026-06-17: "the taxonomy is most important — register it to the right level of the system").
- **The config-scope axis is `Product → Domain → Service` — NOT value-stream/step.** "Subproducts" (Loom/Athena/Werk/Borg — UI labels in chorus-product-tree.html) are **not a distinct level**: they are Products nested by `hasChild` recursion (product→product), per ADR-041:80 + ADR-040's 11-class vocabulary of record (which has **no `SubProduct` and no `SubDomain`**). Config attaches to what actually *runs*; the cascade walks **ADR-041's `hasChild`** up the tree, nearest-wins — `partOf` is usable only as its *declared inverse-fold*, never a competing edge — e.g. `gate.enforced` global → `werk` (a Product) → `gates-service`. The **Step** is the repo tree's grouping/navigation axis — orthogonal to config-scope, never a level a key is set at. The cascade **ceiling is the single root product (`chorus` = global defaults)**; the value stream sits above it but config never ascends into it — `chorus:*` = global is literally the root product node, regardless of vs. **ValueStream/Step is orthogonal** — the ownership/phase axis (ADR-041's tree), the same way ADR-041 holds "roles are orthogonal to the tree." Nothing is scoped at a step (Jeff, 2026-06-17: "not sure what config is scoped at vs or step" — correct: a step is a phase, not a runtime node with config; vs/step at the top of a config path are empty pass-through levels that break the walk). `appliesToClass` names the deepest level a key is *valid* at; the node it's set on names *where it applies*.
- **Dependency — the prerequisite, not a dissolved one (honest correction).** An earlier draft of this ADR claimed the prefix model "dissolves the #3437 containment wall." It does not. Today `partOf` (#3450) is ragged (`gates-service partOf werk-product`, skipping Domain) AND competes with `hasDomain` — two containment edges that disagree, so a node's scope path isn't yet unique. The cascade is correct in principle but **unreliable until one canonical single-parent edge is chosen and populated** (the #3437 ownership wall). That reconciliation is a prerequisite of this ADR's resolver, owned by the model/coherence lane (Silas).
  - **Single-root invariant (Jeff, 2026-06-17):** there is exactly **one product at the root — `chorus`** — and it is the global ceiling. More than one root product = an ambiguous "global." **Live violation today:** `borgProduct partOf chorusStream` makes Borg a *second* root product (sibling of `chorusProduct`), contradicting `chorus-product-tree.html` (Borg nested under Proving *inside* the one Chorus product). The cleanup re-parents every top-level stray under the single `chorus` root, so the global ceiling is unique.
- The PropertyKey **registry itself lives in the `properties` domain**, which ADR-041 places under `proving/borg/domains/properties` (Silas's).

Any level model in this ADR that diverges from ADR-041 is a bug in this ADR, not a second taxonomy.

## Decision

### 1. The registry is DATA, not hand-written SHACL
A key's governing definition is a **`chorus:PropertyKey` individual** — a registry row carrying its constraints as properties. A new key, or a changed enum, is a **model write**, not a code or shape edit. (Hand-authored SHACL per key would make config a code change again — defeating the point.)

**register-before-use:** every `chorus:Property.propertyKey` MUST resolve to a registered `PropertyKey`. The **DAL compiles** each registry row into write-time validation (key registered? value satisfies the key's constraints?). **owl-api projects** the same registry into the generated API + MCP constraints — governance lives in the OWL the generator reads; it cannot be a guard bolted onto the API, because owl-api generates the API *from* the OWL (Kade, #3414/#3467).

### 2. PropertyKey is `propertyKind`-discriminated: `datatype | object`
`:covers` forces the split — its constraint is `sh:class` on an object IRI, which a `{value, valueType, sh:in}` row cannot carry.
- **datatype** arm → `keyValueType` (string|int|bool|json|list) + `allowedValue` (0..n; the `sh:in` enum; absent = unconstrained literal of the type).
- **object** arm → `edgeTargetClass` (the `owl:Class` the value-IRI must belong to; the `sh:class` constraint).
- both carry `appliesToClass` (which class a Property with this key may attach to) and `lifecycleEnabled`.

### 3. The value carrier matches the kind — mismatch is un-representable
- **datatype**-key value → `chorus:propertyValue` (literal).
- **object**-key value → `chorus:propertyValueRef` (`owl:ObjectProperty` → IRI).

`propertyValueRef`, not a string-IRI in `propertyValue`, is the only thing that works for object keys: (a) `sh:class` can only constrain a real node, not a literal; (b) the edge must be SPARQL-traversable; (c) owl-api's generated edge-target-type constraint needs an object reference to check.

A SHACL **`sh:xone` invariant** binds carrier to kind: a Property has *exactly* `propertyValue` (datatype) **xor** *exactly* `propertyValueRef` (object). The "string-IRI smuggled into `propertyValue` for an object key" hole is made **un-representable**, not merely discouraged.

### 4. Lifecycle splits: capability on the key, value on the instance
- `chorus:lifecycleEnabled` (boolean, on the **PropertyKey**) — *may* values of this key expire? (capability)
- `chorus:expiresAt` + `chorus:sourceCard` (on the **Property instance**) — *this* value expires at T, authorized by card N (value).

Never fused — otherwise every value of the key inherits one TTL.

### 5. Traversal shape: the reified Property is the single source of truth; hub object-keys project a *derived* direct predicate
`:covers` is THE hub edge — traversed constantly (coverage / blind-spots / fitness), and the crawler writes a direct `chorus:covers` predicate today. The governed carrier (`test → hasProperty → [covers; valueRef → SubDomain]`) is **2-hop** vs the **1-hop** direct predicate.

Decision: the **reified governed `Property` is the single source of truth**. A `PropertyKey` MAY carry `hubProjected true`; for such keys the **DAL materializes a derived direct predicate** (`chorus:covers`) *from* the Property — never independently authored. `:covers` stays 1-hop traversable AND there is no dual-authoring drift (the direct edge is a projection, not a second source). The crawler's current direct `chorus:covers` becomes that projection. Non-hub object-keys stay pure 2-hop. *(The materialization mechanism is implementation — #3436 — not this ADR.)*

## The model
```turtle
chorus:PropertyKey a owl:Class ;
  rdfs:label "Property Key" ;
  rdfs:comment "Governed definition (registry entry) of a config key. register-before-use: every chorus:Property.propertyKey MUST resolve to a registered PropertyKey. The DAL compiles each to write-validation; owl-api projects it into the generated API+MCP. ADR-044." .

chorus:keyName          a owl:DatatypeProperty ; rdfs:domain chorus:PropertyKey ; rdfs:range xsd:string .   # "testType"
chorus:propertyKind     a owl:DatatypeProperty ; rdfs:domain chorus:PropertyKey ; rdfs:range xsd:string .   # "datatype" | "object"
chorus:appliesToClass   a owl:ObjectProperty   ; rdfs:domain chorus:PropertyKey ; rdfs:range owl:Class .    # which class a Property w/ this key attaches to
chorus:lifecycleEnabled a owl:DatatypeProperty ; rdfs:domain chorus:PropertyKey ; rdfs:range xsd:boolean .  # default false
# --- datatype arm ---
chorus:keyValueType     a owl:DatatypeProperty ; rdfs:domain chorus:PropertyKey ; rdfs:range xsd:string .   # string|int|bool|json|list
chorus:allowedValue     a owl:DatatypeProperty ; rdfs:domain chorus:PropertyKey ; rdfs:range xsd:string .   # 0..n = sh:in; absent = unconstrained literal
# --- object arm ---
chorus:edgeTargetClass  a owl:ObjectProperty   ; rdfs:domain chorus:PropertyKey ; rdfs:range owl:Class .    # sh:class on the value-IRI
chorus:hubProjected     a owl:DatatypeProperty ; rdfs:domain chorus:PropertyKey ; rdfs:range xsd:boolean .  # object-key: DAL also materializes a direct predicate (Decision 5)

# --- value carriers on the Property instance ---
chorus:propertyValue    a owl:DatatypeProperty ; rdfs:domain chorus:Property ; rdfs:range xsd:string .      # datatype-key value (existing #3433)
chorus:propertyValueRef a owl:ObjectProperty   ; rdfs:domain chorus:Property .                              # object-key value (IRI)
# --- lifecycle value (instance-level, only when key.lifecycleEnabled) ---
chorus:expiresAt        a owl:DatatypeProperty ; rdfs:domain chorus:Property ; rdfs:range xsd:dateTime .
chorus:sourceCard       a owl:DatatypeProperty ; rdfs:domain chorus:Property ; rdfs:range xsd:string .

# carrier-matches-kind invariant — a mismatched carrier is un-representable
chorus:PropertyCarrierShape a sh:NodeShape ; sh:targetClass chorus:Property ;
  sh:xone (
    [ sh:property [ sh:path chorus:propertyValue    ; sh:minCount 1 ; sh:maxCount 1 ] ;
      sh:property [ sh:path chorus:propertyValueRef ; sh:maxCount 0 ] ]      # datatype arm
    [ sh:property [ sh:path chorus:propertyValueRef ; sh:minCount 1 ; sh:maxCount 1 ] ;
      sh:property [ sh:path chorus:propertyValue    ; sh:maxCount 0 ] ]      # object arm
  ) .
# NOTE: per-key constraints (allowedValue → sh:in, edgeTargetClass → sh:class, keyValueType → sh:datatype)
# are COMPILED by the DAL from each PropertyKey row at write — NOT hand-authored SHACL per key.
# propertyValueType (#3433) is now redundant with the registry's keyValueType; registry is authoritative (reconcile in impl).
```

## Three-shapes proof (verified by Kade, 16:45)
```turtle
# 1. enum-scalar — datatype arm
chorus:pk-testType a chorus:PropertyKey ; chorus:keyName "testType" ; chorus:propertyKind "datatype" ;
  chorus:appliesToClass chorus:Test ; chorus:keyValueType "string" ;
  chorus:allowedValue "unit","integration","api","ui","perf","security","e2e","bdd" ; chorus:lifecycleEnabled false .
#   instance: <cov-x> chorus:hasProperty [ a chorus:Property ; chorus:propertyKey "testType" ; chorus:propertyValue "integration" ] .

# 2. lifecycle-toggle — datatype arm + lifecycle capability
chorus:pk-gateEnforced a chorus:PropertyKey ; chorus:keyName "gate.enforced" ; chorus:propertyKind "datatype" ;
  chorus:appliesToClass chorus:Service ; chorus:keyValueType "bool" ; chorus:lifecycleEnabled true .
#   instance: <svc-y> chorus:hasProperty [ a chorus:Property ; chorus:propertyKey "gate.enforced" ;
#     chorus:propertyValue "true" ; chorus:expiresAt "2026-07-01T00:00:00Z"^^xsd:dateTime ; chorus:sourceCard "3480" ] .

# 3. typed-edge — object arm, hub-projected
chorus:pk-covers a chorus:PropertyKey ; chorus:keyName "covers" ; chorus:propertyKind "object" ;
  chorus:appliesToClass chorus:Test ; chorus:edgeTargetClass chorus:Domain ;   # sub-domain = a Domain that is hasChild of another Domain; SubDomain is NOT a class of record (ADR-040)
  chorus:hubProjected true ; chorus:lifecycleEnabled false .
#   instance: <test-x> chorus:hasProperty [ a chorus:Property ; chorus:propertyKey "covers" ; chorus:propertyValueRef <subdomain-y> ] .
#   + DAL materializes the derived hub edge:  <test-x> chorus:covers <subdomain-y> .
```

## Consequences
- Governance is enforced **at write**, by the DAL, compiled from data — a new key / enum / edge-type is a model write, no deploy.
- owl-api **generates** a constrained API + MCP from the registry; the constraints cannot drift from the governance because they are projected from the same source.
- The three shapes are proven against real triples; `:covers` is a real, enforceable, SPARQL-traversable typed edge.
- **First instance:** tests-domain (#3473) declares `testType`, gate toggles, and `:covers` against this registry; the literal→Property promotion (#3436) targets it.
- **Deferred to implementation (not this ADR):** the DAL's compile-registry-to-validation, the hub-predicate materialization (#3436), and reconciling the now-redundant `propertyValueType` against the registry's `keyValueType` (registry authoritative).
