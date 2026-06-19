# ADR-047: owl-api Response Contract — the uniform, self-describing, versioned envelope

**Status:** Proposed
**Date:** 2026-06-19
**Author:** Silas
**Card:** #3505 (spec) · implemented by #3506 (Wren, crate injection) · conformance-tested by #3507 (Kade)
**Extends:** ADR-042 (model-driven auth) · ADR-046 (EmitContract) · ADR-040/041/045 (the model)

## Context

owl-api generates a JSON API per Athena primitive (ValueStream/Product/Domain/Service) + per product, all from one OWL/SHACL model (#3509 landed the shapes). Today each generated surface returns an ad-hoc shape (`items` / `vocab` / `contains`), and only `/borg/properties` serves an OpenAPI doc. That ad-hoc-ness is the thing that makes the API *not* look like an open-source-grade platform — every consumer (agents, UIs, SDKs, external integrators) re-learns each surface.

The leverage is the **generator layer**: define the contract once, inject it in the projection, and *every* primitive — present and the 30+ future domains — inherits it. One fix, uniform everywhere. This ADR is the contract; #3506 injects it.

A 2026-06-19 web-research pass (SAFe/RFC/OWASP/Google-AIP sources) graded the current surface and is folded in below — the conformance gaps are cited inline, not paraphrased.

## Decision

Every owl-api response — read, write, AND error — is wrapped in ONE envelope, generated from the shape. No per-endpoint shaping.

### 1. The envelope

```jsonc
{
  "apiVersion": "v1",                         // contract version (this ADR's shape); also in the path: /v1/<surface>
  "kind": "Domain",                            // the primitive/class (or "Error")
  "id": "chorus:properties",                   // the entity IRI (collections omit)
  "self": "/v1/domains/properties",            // canonical URL of this resource
  "generatedFrom": {
    "graph": "urn:chorus:ontology",
    "shape": "chorus:DomainShape",
    "shapeVersion": "2026-06-19",              // per-shape, model-declared (see §2)
    "commit": "534805b9"                       // the chorus.ttl commit the model deployed from
  },
  "data": { ... } | [ ... ],                   // the projected fields (read) — the ONLY place payload lives
  "links": { "partOf": "/v1/products/borg", "definesVocabulary": ["/v1/..."] },  // relation edges → URLs (§3)
  "count": 35,                                  // collections only
  "requiresAuth": true,                         // projected from chorus:requiresAuth (ADR-042)
  "deprecation": null                           // RFC 9745 Deprecation/Sunset, when set (§5)
}
```

- **`data` is the only payload slot.** Fields come from the shape's `sh:property` paths (the real fields — closes the generic `name/label/status` projection gap). Datatype props → scalars; edge props → `links`.
- **Generated, never hand-authored.** Every envelope field traces to the model (`generatedFrom` makes that auditable) or the request.

### 2. Versioning — two independent axes (Google AIP-185 lineage)

- **`apiVersion`** — the *contract* version (this envelope's shape). Path-prefixed (`/v1/...`) AND in the envelope. Changes only when the envelope/contract changes. Coarse, infrastructure-wide.
- **`shapeVersion`** — the *primitive's* version, declared per-shape in the OWL (`chorus:shapeVersion` on the NodeShape) and projected. Changes when a shape's fields change. Fine, per-primitive.

They are orthogonal: a Domain shape can rev (`shapeVersion`) without an `apiVersion` bump, and vice-versa. Both are model-declared and generator-injected — no hand-maintained version constants (the #3402 hardcode class, retired by ADR-042's pattern).

### 3. Links / HATEOAS

Relation edges in the shape (`partOf`, `hasDomain`, `definesVocabulary`, `hosts`, `inStream`) project into `links` as URLs (single → string, multi → array). This makes the graph traversable through the API without out-of-band knowledge — an agent or tool follows `links`, never constructs URLs. (Lean on the model: the edges already exist; `links` is their projection.)

### 4. Errors — RFC 9457 (Problem Details), as the same envelope

The research's #1 conformance gap: errors are non-conformant. Fix: errors use the SAME envelope with `kind: "Error"` and an RFC-9457-shaped `data`, served as `application/problem+json`:

```jsonc
{ "apiVersion": "v1", "kind": "Error",
  "data": { "type": "/errors/validation", "title": "...", "status": 422,
            "detail": "...", "instance": "/v1/domains/x",
            "errors": [ { "field": "purpose", "detail": "minCount 1 unmet" } ] },  // per-field, projected from SHACL violations
  "generatedFrom": { ... } }
```

- **Status codes (RFC 9110/6585):** `422` SHACL/validation failure (per-field `errors[]` from the violation report), `401`/`403` auth (ADR-042), `404` no such entity, `412` If-Match precondition, `428` precondition required, `429` rate-limit.
- Per-field `errors[]` is the SHACL validation report projected — the model is the input-validation boundary (security §), so the error detail is generated, not hand-written.

### 5. Deprecation (RFC 9745 / RFC 8594)

When a shape or surface is being retired, the envelope's `deprecation` carries `{ since, sunset, replacedBy }` and the response sets `Deprecation:` + `Sunset:` headers + a `replacedBy` link. Model-declared (`chorus:deprecatedSince`/`chorus:sunset` on the shape), generator-projected. This is the strangler-fig made visible at the API boundary — a retiring surface announces itself.

### 6. Correlation + traces (research finding O1 — and we already have the pieces)

owl-api already mints/accepts a trace via `effective_trace` and emits `trace_id` on `api.request.served`; Chorus already has a `traces` table + reader (#2101/#3163). The contract requires:
- **Accept + propagate W3C `traceparent`** (`00-{trace-id}-{span-id}-{flags}`) — bridge the existing chorus `trace_id` to the W3C standard so external tracing tools join, AND it lands in our trace table.
- **Echo `X-Request-Id` / `traceparent`** on every response so a caller can cite it.
- **Propagate downstream to Fuseki** so `upstream_ms` attributes to the same trace.
- One trace id is the universal join key: caller → owl-api → Fuseki, and logs↔metrics↔traces join on it. (Detail lands in the EmitContract spec, this card's AC-3.)

### 7. Transport hygiene (research should-fixes, each independently landable)

`HEAD`/`OPTIONS`, `Cache-Control` + `ETag` (= `generatedFrom.commit` → `304` on `If-None-Match`), `If-Match` optimistic concurrency on writes (`412` on mismatch), `Vary: Accept` for content-negotiation (json→`data`, html→rendered map), cursor pagination on collections (AIP-158), `/livez` + `/readyz`, OpenAPI **3.1** (maps cleaner from SHACL than 3.0.3), a served OpenAPI per surface (`/<surface>/openapi`, today only properties) + a discovery root listing primitives+versions.

## Consequences

- **Uniform leverage:** one envelope, every primitive — agents/UIs/SDKs/integrators learn the pattern once. The "API is a big part of how the system looks as an open-source candidate" (Jeff) becomes true by construction.
- **Self-describing + auditable:** `generatedFrom` ties every response to the exact graph/shape/commit — no drift between what's served and what's modeled.
- **No per-endpoint code:** the contract is generator-injected (#3506). Adding a domain inherits it free.
- **Cost:** the envelope adds bytes per response (acceptable; the consistency + tooling-interop is the point — same conform-to-standards lesson as the model itself).

## Scope / sequencing

- This ADR is the **contract spec**. #3506 (Wren) injects it in the owl-api projection layer. #3507 (Kade) wires the conformance runner that asserts it.
- **Prove-one-first (Wren's caveat):** #3506 projects ONE shape end-to-end through this contract into a landed API before fanning out to the other three. The contract is uniform; the *proof* is incremental.
- Companion specs on this card: the API security model (AC-2), the normalized Borg EmitContract (AC-3), the deployable-grain decision (AC-4) — authored alongside this ADR.

## References

- ADR-042 (model-driven auth) · ADR-046 (EmitContract) · ADR-040/041/045 (the model #3509 landed)
- RFC 9457 (Problem Details) · RFC 9110/6585 (status) · RFC 9745/8594 (Deprecation/Sunset) · W3C Trace Context · Google AIP-158/185 · OpenAPI 3.1 · OWASP API Top 10 2023
- Research pass 2026-06-19 (folded inline) · code review 2026-06-19 (emit/openapi-gen/product-index/write-routes/auth/effective_trace already exist in the crate)
