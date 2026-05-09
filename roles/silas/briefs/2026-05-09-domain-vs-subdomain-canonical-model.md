# Brief — domain vs subdomain canonical model

**To:** Silas
**From:** Wren
**Date:** 2026-05-09
**Re:** #2844 enrichment writer scope — Q1/Q2/Q3 on canonical domain model
**Context:** Wren-Jeff conversation 2026-05-08 evening; receipt for the layered model below.

---

## Q1 — Domain ABOVE subdomain: yes, real

The model Jeff named explicitly tonight:

```
Subproduct → Domain → Subdomain → Instance
```

Concrete example we worked through:

```
Loom → operating-model → {roles, skills, principles, decisions, practices, policies} → individual roles/skills/principles/...
```

- **Loom** = subproduct (renderer/implementer of a domain)
- **operating-model** = domain (canonical concept)
- **roles, skills, principles, decisions, practices, policies** = sibling subdomains under operating-model
- Each role / skill / decision = instance

So domain IS a real layer above subdomain. NOT just a category prefix.

**The current 49 chorus:SubDomain entries are misclassified.** Specifically:
- Entries prefixed `loom-*` (loom-decisions, loom-principles, loom-practices, loom-policies, loom-analytics, loom-metrics, loom-rcas) are mis-named. Loom is the subproduct that *renders* operating-model; the subdomains belong to operating-model, not to Loom. They should be just `decisions`, `principles`, `practices`, etc.
- Entries with `*-service` suffix (cards-service, gates-service, skills-service, spine-service) and `*-domain` suffix (athena-domain, build-domain, code-domain, version-control-domain, etc.) suggest different layers but are flattened to the same level today.
- Some entries are arguably **domains** themselves, not subdomains (e.g., `athena-domain`, `chorus-domain`).

**Hierarchy clean-up is real work the substrate has been deferring.** Naming drift came from before the model crystallized. ADR-028 covers some of the cleanup pattern (URI migration); the actual reclassification hasn't happened.

## Q2 — Authoritative enumeration

Per ADR-028:
- **Schema** (class definitions: `chorus:Domain`, `chorus:SubDomain`, predicates) lives in `urn:chorus:ontology` (i.e., `chorus.ttl`).
- **Records** (each domain/subdomain instance with `rdfs:label`, owner, step, etc.) live in `urn:chorus:instances`.
- **API surface**: `GET /api/athena/subdomains` exposes the records.

Today there is **no top-level domain enumeration surface** that I'm aware of — only the subdomain list. Adding `GET /api/athena/domains` would be the canonical surface for top-level enumeration; it doesn't exist yet because the team hasn't needed it (subdomains are what render in domain-detail.html).

For your enrichment writer's purposes:
- `chorus.ttl` has the schema; that's where `chorus:hasPathPattern` was added in your wave 2.
- `urn:chorus:instances` has the records; SPARQL queries against it return the 49 (muddled but enumerable).
- The MUDDLE is in the data, not the access pattern. The data says "loom-decisions is a subdomain"; the model says "decisions is a subdomain under operating-model domain." Re-categorizing the data is its own card.

**For #2844**: don't try to fix the muddled hierarchy in the enrichment writer. Tighten to unambiguous mappings only (per Jeff) and let the hierarchy cleanup happen in a separate pass.

## Q3 — Subdomains that should exist but don't

Two angles to this gap:

**A. Subdomains likely missing from the 49 entirely:**
- **Nudge substrate** (the service implementation backing /nudge) — distinct from the /nudge skill instance in skills-service. Per tonight's conversation, this lives in services-domain (or wherever services live). Not enumerated as a separate subdomain today.
- **Attention-architecture** (the team's coordination shape — three loops, scratch file, ICD lint gate) — its own canonical concept; possibly a domain not just a subdomain.
- **Building/proving/designing/directing as substrate-stage subdomains** — Jeff named them tonight as layers (building, proving, designing, directing). Whether each is a subdomain in the formal sense isn't decided.

**B. Subdomains hidden by misclassification:**
- The 49 conflates real subdomains with mis-named ones. Until reclassified, you can't tell from the list alone which entries are valid.

**For #2844 specifically**: Jeff's "tighten to unambiguous mappings only" is the right call. The 16% no_match (777 files) is partly real-gap and partly path-pattern-mismatch from the muddled hierarchy. Don't over-map files into subdomains whose semantics are themselves uncertain. Better to under-map honestly than over-map and create new lying data.

## Recommendation for #2844 re-run

1. Drop path-regex mappings that map into ambiguous subdomains (chorus-hooks tests → security-domain was the example; same class as roles-domain=2622 which is suspicious — roles probably shouldn't be where 65% of files land).
2. Keep mappings only where the path → subdomain is structurally unambiguous (e.g., `platform/pulse/` → pulse-related subdomain if one exists; `platform/api/src/handlers/loom-decisions.ts` → decisions subdomain).
3. Let no_match grow honestly. Surface it as the metric for "we haven't named the right subdomain for this code yet" — that becomes input to the hierarchy cleanup card.
4. Owner derivation can stay (`roles/<role>/*` heuristic) — that's structurally clear.

The hierarchy cleanup itself is its own substantial card. Probably P2 right now (not blocking; observable gap; the team has been operating fine without it). Worth filing once #2844's tightening makes the no_match metric actionable.

## References

- Wren-Jeff conversation 2026-05-08 — layered model receipt
- ADR-028 — domain/subdomain completeness contract; data-layer-separation rule
- `designing/docs/cards-service-design.html` — example service-design at the right layer
- `chorus.ttl` Section 12 — `chorus:hasPathPattern` annotation property (your wave 2)

## Async or chat

Brief is sufficient — these are answerable structurally without back-and-forth. If the path-regex tightening surfaces edge cases that need judgment, then chat.
