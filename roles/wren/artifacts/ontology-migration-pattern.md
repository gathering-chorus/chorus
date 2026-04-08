# Ontology-Driven Migration Pattern

The OWL ontology is the authority for the reorg. SPARQL queries are the test suite.

## The Model

```
Product (Jeff owns)
  └── Sub-product (Role owns) ← ownedBy → Role
       ├── primaryPhaseProduct → Phase
       └── hasValueStream → ValueStream

Domain (Role owns) ← ownsDomain → Role
  ├── belongsToProduct → Product/Sub-product
  ├── servesValueStream → ValueStream
  └── primaryPhase → Phase

Service (Role owns) ← ownedBy → Role
  ├── operatesIn → Domain
  ├── supportsStream → ValueStream
  └── runsOn → Machine
```

## Migration Card AC Generator

For each sub-product migration, the SPARQL queries in `migration-queries.sparql` generate AC:

1. **Q1 passes** → sub-product has an owner
2. **Q2 passes** → all services have domains
3. **Q3 informs** → domains without services are design-only (acceptable if conceptual)
4. **Q4 passes** → all services have owners
5. **Q5 passes** → sub-product has a primary phase
6. **Q6 informs** → domains serve at least one value stream
7. **Q7 passes** → sub-product is fully linked (owner + phase + domain + service)

A sub-product migration card closes when Q7 returns that sub-product.

## Validated Example: Cards

```
CardsProduct
  ownedBy: Wren
  primaryPhaseProduct: Directing
  hasValueStream: ChorusStream

CardsDomain
  belongsToProduct: CardsProduct
  servesValueStream: ChorusStream
  primaryPhase: Directing

CardsService
  ownedBy: Wren
  operatesIn: CardsDomain, CoordinationDomain
  supportsStream: ChorusStream
```

Q7 returns Cards. Migration complete.

## Vocabulary

- **Product** = top-level (Gathering, Chorus) — Jeff owns
- **Sub-product** = Cards, Clearing, Demo, Hooks, Loom, Convergence, Bridge — roles own
- **Domain** = bounded context (cards, photos, nudge) — roles own
- **Service** = running code (Cards CLI, Clearing Express app) — roles own
- **Practice** = cross-domain discipline (convergence) — not a domain
- **Phase** = value stream stance (Designing, Building, Directing, Proving)

Do not call sub-products "products" in conversation. Do not call services "products." The ontology is precise — use its vocabulary.
