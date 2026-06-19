# ADR-048: owl-api Security Model — authz-as-data, generator-enforced

**Status:** Proposed
**Date:** 2026-06-19
**Author:** Silas
**Card:** #3505 (spec) · implemented by #3506 (Wren) · conformance-tested by #3507 (Kade)
**Extends:** ADR-042 (model-driven auth — `chorus:requiresAuth` projected per surface) · ADR-047 (response contract)

## Context

ADR-042 landed model-driven auth as a single boolean (`chorus:requiresAuth` on a shape → the generator projects the guard). That's the seed: security is *declared in the model and enforced by the generator*, not hand-coded per endpoint. As owl-api becomes the platform every primitive/product/domain projects through, the security surface has to be uniform and model-declared too — or each generated surface re-invents (or omits) authz, and a single missed endpoint is the OWASP API1 (BOLA) hole.

The 2026-06-19 research pass (OWASP API Top 10 2023) named the gaps: object-level authz (BOLA/API1), unrestricted resource consumption (API4), missing security headers (API8), and the input-validation boundary. This ADR specs the model so the generator closes them uniformly.

## Decision

Security is **authz-as-data**: declared on the shape, projected and *enforced* by the generator — never per-endpoint code. Six parts.

### 1. Per-shape authorization scope (read/write asymmetry)

A shape declares who may read and who may write, as data:

```turtle
chorus:DomainShape
    chorus:requiresAuth  true ;                 # ADR-042 (kept)
    chorus:readScope     chorus:public ;        # or a role/scope IRI
    chorus:writeScope    chorus:silas .          # writes are strictly narrower than reads
```

The generator projects a guard per surface from these. **Read and write are independent** — a surface can be world-readable but role-write. Default-deny on write when `writeScope` is absent (fail-closed, not fail-open — the ADR-042 follow-on Silas owed).

### 2. Object-level authz (BOLA / OWASP API1)

The killer gap: a guard that checks "may you call this route" but not "may you touch *this object*." For owl-api, object-level authz is **model-derived**: an entity's `ownedBy` (Role) + the shape's `writeScope` together decide whether the caller may mutate *that instance*. The generator injects the per-object check on every write, uniformly — there is no route where it's skipped (that uniformity is the whole point of generator-injection vs per-endpoint).

### 3. Field-level exposure whitelist

Only fields the shape marks exposable appear in `data`. A shape property can carry `chorus:exposure` (`public` | `internal` | `secret`); the generator emits only `public` (and `internal` to authed callers), and **never** `secret` (credentials, tokens — the write-scrubber's concern, now also a read-boundary). Closes the over-exposure leak; ties to the no-secrets rule (write_scrubber) at the *read* side too.

### 4. SHACL as the write-validation boundary (gap-free)

Every write validates against the shape's SHACL **before any side effect** — the input-validation boundary, generated from the same shapes the read side projects. A violation → `422` + the per-field `errors[]` (ADR-047 §4), never a partial write. There is no log-and-continue, no default-permissive path. This is the single validation boundary (one model, one validator) — not per-endpoint hand-checks that drift.

### 5. SPARQL-injection safety + Fuseki exposure

- **SPARQL-injection:** all caller input that reaches a SPARQL query is parameterized/escaped (IRIs validated against the ADR-040 grammar, literals bound not interpolated). The generator owns query construction — callers never supply raw SPARQL.
- **Fuseki is not publicly exposed:** `localhost:3030` stays loopback-only; owl-api is the *only* writer to the graphs. The contract: external traffic reaches the model only through owl-api's generated, validated, authz'd surface — never Fuseki directly.

### 6. Resource limits + headers + audit (OWASP API4/API8)

- **Rate-limit / query-timeout** per caller (API4 — unrestricted consumption); `429` with `Retry-After` (ADR-047).
- **Security headers** (API8): the generator sets the standard set on every response.
- **Auth attempts → spine:** every allow/deny emits an auth event to the spine (the EmitContract carries it — ADR-046/this card's AC-3), so Borg sees the authz surface. PII/secret **redaction** in those logs (no token/credential values — same scrubber boundary).

## Consequences

- **Uniform, gap-free:** because authz is generator-injected from the model, there is no "the one endpoint someone forgot" — BOLA/over-exposure can't hide in a hand-written route.
- **Changeable as data:** tightening a scope or hiding a field is a model edit + regenerate (the regenerate-on-change discipline), not a code change.
- **Fail-closed:** absent `writeScope` → deny; non-`public` field → hidden; invalid input → `422`. The defaults are safe.
- **Cost:** the model must carry the scopes/exposure (more annotation per shape). That's the price of model-declared security; it's auditable in one place (the graph) instead of scattered across handlers.

## Scope / sequencing

Spec only (this card). #3506 (Wren) injects the enforcement in the projection layer; #3507 (Kade) asserts it in conformance (a write without scope → 403; a secret field → absent; bad input → 422). ADR-042's single-boolean guard is the proven seed; this generalizes it to read/write scopes + object-level + field-level + the input boundary.

## References

- ADR-042 (model-driven auth) · ADR-047 (response contract) · ADR-046 (EmitContract, carries auth events)
- OWASP API Security Top 10 2023 (API1 BOLA, API4 resource-consumption, API8 headers) · research pass 2026-06-19
- write_scrubber (no-secrets PreToolUse) — extended here to the read boundary (field-exposure)
