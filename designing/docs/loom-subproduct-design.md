# Loom — Subproduct Design

First subproduct-design doc — peer to the service-design docs, intended as the template the other five subproducts follow.

This is a **coherence exercise**: Loom modeled up and down (Chorus product → Loom → subdomains), with the OWL and SHACL state, and the incoherences the audit surfaced. It is **AS-IS** — not a remediation plan. The to-be is downstream v2 work.

Wren, 2026-05-14. Source: four-subagent audit — OWL (`chorus.ttl` + `framework.ttl`), SHACL (`shapes.ttl` + inline shapes), doc-catalog + RDF instance data, live Fuseki graph.

---

## What Loom is

The operating-model / team-knowledge layer of Chorus — "the team's persistent product memory." A subproduct of the **Chorus** product, at the **Shaping** value-stream step, owned by **Jeff**. Its subdomains carry the normative substrate: principles, practices, policies, decisions, roles, skills, RCAs.

`designing/docs/chorus-reference-model.html` already defines Loom in these terms — it is the closest existing Loom design, and this doc builds on it.

## Structure — up and down

**UP:** a single inbound edge — `chorusProduct hasSubProduct loom`. That is the entire upward link. There is no reciprocal `loom partOf chorusProduct`; containment is one-directional.

**DOWN — intent-set (7, per Jeff's dictation):** principles, practices, policies, decisions, roles, skills, rcas.

**DOWN — what the graph actually has (12 `hasDomain` edges):**

| subdomain | owner | step | content instances |
|---|---|---|---|
| loom-principles | wren | Shaping | **27 Principle** |
| loom-practices | wren | Shaping | **40 Practice** |
| loom-policies | wren | Shaping | 0 — shell |
| loom-decisions | wren | Shaping | 0 — shell (948-line `roles/wren/decisions.md` is the de-facto store; ungraphed, blocked on #2152) |
| loom-rcas | **silas** | Proving | 0 — shell; no `RCA` OWL class exists |
| loom-metrics | wren | Shaping | 0 — *not in intent set* |
| loom-analytics | wren | Shaping | 0 — *not in intent set* |
| roles-domain | wren | Shaping | 0 Role instances (1 Document) |
| cards-service | wren | — | service node — *not in intent set* |
| skills-service | wren | — | 0 Skill instances — *not in intent set* |
| gates-service | silas | — | service node — *not in intent set* |
| chorus-domain | **silas** | Proving | 52 Documents — *not in intent set; a platform doc-dump catch-all mis-parented under Loom* |

**Divergences:**
- Graph has **12**, intent-set is **7**. Six extras: metrics, analytics, cards-service, skills-service, gates-service, chorus-domain.
- **"skills" is missing** as a content domain — `skills-service` is a service node, not the skills content domain; there is no `loom-skills`.
- **Hollow shells:** only **2 of 12** subdomains hold content instances — `loom-principles` (27) and `loom-practices` (40). The rest are declared and connected but empty.
- `chorus-domain` (Silas-owned, 52 Documents, step Proving) is wired under Loom but is not a Loom content domain — it reads as a platform docs catch-all attached to the wrong parent.
- `loom-rcas` is the only Loom subdomain not owned by Wren (Silas) and the only one at Proving — worth resolving whether RCAs belong to Loom-as-Wren-product or are a shared/Silas surface.

## The OWL

**Classes** (`chorus.ttl` §2c, ~L222–243; `Product` at ~L697): `Product` is standalone. `Domain` is the parent of `SubProduct`, `SubDomain`, `CollectionDomain` as subclasses. **Structural oddity:** `SubProduct` is `rdfs:subClassOf Domain` yet is used as a product-level container above domains — the hierarchy says "a SubProduct is a kind of Domain," which collides with its actual role.

**Loom's own OWL:** an *instance* of `SubProduct` (~L743–749) — substantive: label, comment, `ownedBy jeff`, `hasDomain` ×12, `consumes` ×6.

**Subdomain OWL:** `roles-domain`, `loom-principles`, `loom-practices` are substantive (carry `contains` structure). `loom-policies`, `loom-decisions`, `loom-rcas`, `loom-metrics`, `loom-analytics` are thin — label + `ownedBy` + `primaryStep` only.

**Predicate-layer incoherences (the core finding — and they correct the going-in framing):**
- **`hasDomain` is declared twice with conflicting signatures** — `Document → SubDomain` (L554, a tagging predicate with a SHACL shape) and `Product → Domain` (L2207, true containment). One IRI, two incompatible semantics. *This is the real `hasDomain` defect — not the containment-vs-dependency overload we assumed.*
- **`consumes` is the genuinely overloaded predicate** — declared `Product → Service` (L2230), used `SubDomain → SubDomain` throughout (`loom-* consumes security-domain` etc.). The ontology has `dependsOn` explicitly for this and it's ignored.
- **`belongsTo` — used 30+ times, never defined.** Carries the entire Borg containment tree on an undeclared predicate. The single biggest hole.
- **`partOf`** — neither declared nor used. Phantom concept.
- `dependsOn`, `expresses` — declared without `rdfs:domain`/`rdfs:range`.
- **`framework.ttl` runs a parallel vocabulary** — `fw:Domain owl:equivalentClass chorus:Domain`, plus `fw:ownedBy` / `fw:dependsOn` with no `owl:equivalentProperty` bridge to the `chorus:` versions. The same real-world domains are modeled twice (`fw:photos` and `chorus:photos-domain`).
- **`SubDomain` is the spine of the instance graph** — ~45 instances, the `rdfs:range` of `hasActor`/`hasScenario`/`hasContract`/`hasPage`/`hasPipeline`, and the target of the `DocumentShape` SHACL constraint. It is *not* vestigial — which is exactly why the as-is/to-be refactor to remove it is expensive.
- `Decision` and `RCA` are **not OWL classes** — `loom-decisions` and `loom-rcas` hold (or would hold) instances of undeclared types.

## The SHACL

15 shapes across 4 files. Loom's **skeleton is constrained** — `ProductDomainShape`, `SubProductParentShape`, `SubProductDomainShape`, `SubDomainParentShape`, `SubDomainInstancesShape` enforce structural minimums (a SubProduct must have ≥1 parent and ≥1 domain; a SubDomain must have a parent).

Loom's **substance is not constrained:**
- `Domain` (the class) has no shape.
- `Principle`, `Policy`, `Practice`, `Skill` — Loom's actual content types — have **zero shapes**. Nothing validates what a principle or a policy must contain.
- `Decision`, `RCA` — no OWL class, so no shape is even possible.
- `CatalogDocShape` targets `chorus:CatalogDoc`, **a class not declared in the OWL** — a shape constraining a phantom.

## Coherence findings — synthesis

1. **The predicate layer is broken under the model.** `hasDomain` double-declared with conflicting signatures; `belongsTo` load-bearing but undefined; `consumes` overloaded into dependency duty; `partOf` phantom. The relationships *are* the model — and they are the least coherent layer.
2. **Two vocabularies, no bridge.** `chorus:` and `fw:` model the same domains, classes, and ownership twice with no equivalence axioms tying them.
3. **The graph diverges from intent.** Loom's `hasDomain` is 12 where intent is 7 — six extras, "skills" missing, and `chorus-domain` is a Silas doc-dump mis-parented under Loom.
4. **Hollow shells.** 5 of 7 intent subdomains carry zero content instances. Only principles (27) and practices (40) are populated. Decisions live as 948 lines of ungraphed markdown.
5. **Substance is unconstrained.** SHACL guards Loom's hierarchy, nothing guards Loom's content; two content types aren't even classes.
6. **Full duplication.** Every triple is duplicated across `urn:chorus:ontology` and `urn:chorus:instances`.

## What this means — Loom as template

A coherent subproduct-design needs, minimally: a defined upward edge; one clean containment predicate (not a double-declared `hasDomain`); every subdomain either populated or explicitly marked *planned*; content types that are real OWL classes with SHACL shapes; one vocabulary, not two.

Loom-as-template surfaces precisely the fixes v2 needs. This doc is AS-IS — the value of the coherence exercise is that the incoherences are now named with receipts. Resolution is the v2 work; replicating this audit shape across the other five subproducts is the path.
