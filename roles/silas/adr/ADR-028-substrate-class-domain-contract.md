# ADR-028: Substrate-Class Domain Contract

**Status:** Accepted (Silas + Wren co-author, 2026-05-04) — drafted 2026-05-02 in chorus session, reconstructed to disk 2026-05-03 by Silas from chorus-index drafts after the v3 commits regression confirmed why this contract needs to be on disk and citable. Wren co-author review 2026-05-04 verified the three reconstructed sections; patches applied. **Pending: Jeff acceptance.**

**Date:** 2026-05-02
**Builds on:** ADR-025 (ontology vs instances graph separation)
**Source of pattern:** `/loom/principles-reference-impl.html` (the principles arc as the proof) + `/loom/cookbook-substrate-class-domain.html` (the recipe abstracted from that arc)
**Cards bound by this ADR:** #2254 (alerts), #2469 (urn migration parent), #2655 (commits MCP), #2652 (cards CLI implementation), all future substrate-class-domain reshapes

---

## Context

Chorus has accumulated substrate-class domains — principles, decisions, ADRs, services, alerts, cards, commits, practices, policies — that each followed their own ad-hoc shape. The principles arc (#2447 → #2314 → #2157 → #2473 → #2449) was the first time a substrate-class domain went through a deliberate replication-ready treatment: graph as single source, API as only write path, drift test as gate, cite-by-ID never inline, hook backstops the affordance.

That arc demonstrated a contract that transfers. The principles-reference-impl page documents the worked example; the cookbook page abstracts the recipe. But until that contract is named in an ADR, future substrate-class-domain work drifts back to ad-hoc shape — domains accumulate parallel write paths, paraphrased text in role files, no drift tests, hand-edits to TTL alongside API writes.

Jeff named on 2026-05-02: *"we must not stop until both [cards and commits, currently being reshaped] conform to our musts list."* That mandate requires the musts list be a single citable artifact, not embedded in the principles page where future readers would think it's principles-specific.

The 2026-05-02 commits regression (cards #2661 / #2682 / #2662 — chorus_commit MCP wrapper with refusal taxonomy) is the canonical example of what an unanchored contract permits. The v3 amendment shipped a Move-5-shaped refusal hook before Moves 1–4 had closed for the commits domain — a violation of the cookbook's order discipline ("MUST NOT open Phase 2 before Phase 1 closes; restructuring on dirty data carries the dirt forward"). Without ADR-028 on disk, no card could cite the violation at gate-arch and no gate could refuse it on contract grounds. The ADR exists to make order discipline citable.

## Decision

Every substrate-class domain — defined as a domain whose contents are *governed instances* (principles, decisions, ADRs, alerts, cards, commits, practices, policies, services, +) rather than *runtime data* — MUST honor seven universal contracts plus three structural invariants, sequenced by the cookbook's order discipline. Domain-specific decisions ride alongside but do not override these.

### The seven universal MUSTs

**1. Graph separation (Layer 1 — data)**
- MUST: schema lives in `urn:chorus:ontology` (OWL classes, RDFS properties, axioms, SHACL shapes, comments only)
- MUST: instances live in `urn:chorus:instances` (every populated content row)
- MUST NOT: mix instance data into ontology graph
- MUST: preserve URIs across migrations (renaming graph location ≠ renaming subject)
- MUST: instances declare `rdfs:label` + `rdfs:comment` minimum; class-specific fields beyond
- MUST: subdomain links to instance via `chorus:contains` on every create

**2. Single write path (Layer 2 — write)**
- MUST: write through `POST/PUT/DELETE /api/athena/subdomains/{subdomainId}/{entityType}`
- MUST: reject `POST` without label → HTTP 400
- MUST: declare SPARQL prefixes (`chorus`, `rdfs`, `skos`, `dcterms`, `owl`) in every INSERT/DELETE template via shared `SPARQL_PREFIXES` constant
- MUST: mint URIs as `https://jeffbridwell.com/chorus#<subdomainId>-<entityType>-<slugify(label)>`
- MUST: escape double-quotes in literals (SPARQL injection)
- MUST NOT: any other write surface (no direct TTL edits, no SPARQL UPDATE outside Athena handlers, no script-direct writes)

**3. Single read path (Layer 3 — read)**
- MUST: canonical reads at `GET /api/athena/subdomains/{subdomainId}/{entityType}` returning `{entities: [...]}` shape
- MUST: any predecessor read path 308-redirects to canonical
- MUST NOT: parallel read implementations — drift starts the moment two read shapes exist for the same data
- MUST: UNION across both graphs in subdomain-detail SPARQL during migration window (SubDomain may live in ontology while instances live in instances)

**4. Cite-by-ID, never inline (Layer 5 — citation)**
- MUST: role fragments contain redirect-to-canonical + emphasis URIs only — no inlined content text
- MUST: any consumer (script, hook, page, role fragment) reference URIs, not paraphrased names
- MUST: `claudemd-gen.sh` regenerates role CLAUDE.md after any fragment edit
- MUST: protocol contract hash unchanged when only role-level emphasis changes (fragments are not in `core_paths`)

**5. Hook backstops the affordance (Layer 6 — enforcement)**
- MUST: pre-commit reject staged commits that add instance triples to the watched file (the path the API replaces)
- MUST: hook is scoped — ignore non-watched paths
- MAY: bypass via `<DOMAIN>_DIRECT_EDIT_SKIP=1` for one-off migration / schema-only commits; bypass MUST emit a spine event for audit
- The typed API becomes the path of least resistance; the hook bites the path it replaces

**6. Drift test as gate (Layer 9 — test)**
- MUST: a baseline (static page or snapshot) holds against the graph; divergence fails CI
- MUST: subdomain integration tests pass on every commit
- MUST: hook hermetic tests pass (deterministic, no live state dependency)
- SHOULD: count assertions are floors (`≥N`), never exact — graphs grow

**7. Subdomain plumbing (Layer 8 — governance)**
- MUST: subdomain passes completeness gate (lifecycle, actors, scenarios declared)
- MUST: meta-alerts wired for: stale instance index, API errors on canonical path, orphan instances (no `chorus:contains` edge to a subdomain)
- MUST: if MCP exposed: single-verb-single-target tools (`<domain>_list`, `<domain>_get`, `<domain>_create`); no `*_op(action, ...)` collapses
- MUST: if MCP exposed: `X-Chorus-Role` header propagates end-to-end into spine events on every invocation
- MUST: if MCP exposed: tool descriptions carry (a) what it does, (b) when to reach for it, (c) what it is NOT for

### Order discipline (from cookbook-substrate-class-domain.html)

The seven MUSTs are sequenced. They are not a checklist to attack in parallel.

- **MUST: land moves serially, not in parallel.** Move 5 (hook backstop) only earns its keep after Moves 1–4 have made the typed path more attractive than the path being blocked. Skip the order, ship a hook that punishes roles for using the only path they have.
- **MUST: ship Move 3 (drift test).** It's the most-skipped move. The drift test is what makes the contract a contract; without it, paraphrase grows back.
- **MUST: be atomic per move, not across moves.** Each card commits one move end-to-end. Cross-class atomicity ("decisions and practices together") is sacrificed for testability and reversibility.
- **MUST NOT: open Phase 2 (refactor / placement) before Phase 1 closes.** Restructuring on dirty data carries the dirt forward.

### Structural invariants

- **I-1: Graph is the source of truth.** No surface caches. No surface paraphrases. Drift starts the moment a copy lives somewhere else.
- **I-4: One source, multiple call shapes.** REST, MCP, SPARQL, page render — all delegate to the same Athena handler reading the same graph. New transports are surfaces over the source, not new sources.
- **I-10: Atomic per class, not atomic across classes.** Each migration child (Principle, Practice, Alert, ...) lands as its own atomic unit; cross-class atomicity is explicitly sacrificed for testability and reversibility.

These three are the meta-invariants. The remaining eight in the canonical I-N list at `platform/api/public/loom/principles-reference-impl.html` (11 total) either collapse into one of the seven MUSTs (I-2/I-3 → MUST 1, I-5/I-6 → MUST 7, I-7 → MUST 4, I-8 → MUST 6, I-9 → MUST 5) or are domain-specific (I-11 three-readings is principles-specific). I-1, I-4, I-10 are what the seven MUSTs flow from.

## Class B addendum — adapter-as-source domains

Some substrate-class domains have a non-graph source of truth. **Commits** is the canonical example: git refs and objects are the data; `git-queue.sh` is the canonical adapter; the spec doc is the contract. The seven MUSTs apply via mapping:

- **MUST 1 (graph separation)** maps to **schema/data separation**: spec lives at a single citable artifact (e.g., `commits-service-design.md`); data is git refs/objects; no parallel specs.
- **MUST 2 (single write path)** maps to **single canonical adapter**: one named entry point for mutations (e.g., `git-queue.sh commit` and `git-queue.sh push`), sourced by every consumer; no agent-direct path to the underlying substrate.
- **MUST 3 (single read path)** maps to **single canonical query**: one named read surface for state (or an explicit decision to leave reads unmediated, with the gap named — see Addendum 2).
- **MUST 4 (cite-by-ID)** is unchanged: SHAs, branch names, card-ids in messages. **Role briefs MUST link to the spec doc and cite SHAs/branch-names/card-ids; MUST NOT paraphrase the spec flow.** The 2026-05-02 commits regression had role briefs paraphrasing v2.5 commit flow while v3 was being drafted — paraphrase is exactly the drift this clause refuses.
- **MUST 5 (hook backstops)** is unchanged: the hook bites the path the adapter replaces.
- **MUST 6 (drift test)** maps to **adapter-vs-spec drift test**: a test that asserts the adapter's behavior matches the spec doc; divergence fails CI. **This is the most commonly skipped MUST for Class B domains** — without it, the spec rots while the adapter accretes ad-hoc behavior.
- **MUST 7 (subdomain plumbing)** is unchanged.


## Addendum 2 — single declarative schema for validation

When validation logic for a substrate-class domain (or its Class B adapter) is spread across multiple sites — e.g., a regex in one script, a procedural check in another, a pre-commit hook, a pre-push hook, an MCP tool input schema — the contract is no longer single-sourced. Drift between sites becomes inevitable.

- **MUST: validation logic lives in a single declarative schema** (JSON Schema, SHACL, Zod, or domain-equivalent), referenced by every site that needs to validate.
- **MUST NOT: ad-hoc procedural checks duplicated across hooks, scripts, and tool inputs.** If a check needs to fire in N places, the schema fires there N times — but the schema itself lives in one place.
- The 2026-05-02 commits audit surfaced validation in **five** places for the same role-prefix invariant: (1) `branch-check.sh` regex, (2) `git-queue.sh` procedural in `do_commit`/`do_push`, (3) pre-commit hook, (4) pre-push hook (#2598), (5) chorus-api commits-service Zod input + `check_branch` validation in the MCP wrapper (#2641 territory). The fifth is the MCP boundary — the service re-validates `role↔branch` before reaching `git-queue.sh`, which gives the typed `branch-mismatch` refusal. That is the anti-pattern this addendum names.

## Application to active arcs

| Card | Domain | Phase | Status against contract |
|---|---|---|---|
| #2254 | alerts | Phase 1 mid-arc | Walking the seven MUSTs (Wren + Silas conformance audit, 2026-05-02) |
| #2469 | urn migration | Cross-cutting | Required for any Move 1 that lands new instance URIs |
| #2655 / #2661 / #2682 / #2662 | commits | Phase 1 incomplete; Phase-2-shaped work shipped | Move 5 (refusal hook in chorus_commit) shipped before Moves 1-2 closed. **In violation of order discipline.** Recovery before reform per cookbook. |
| #2652 | cards CLI | Phase 1 mid-arc | Conformance audit completed (Silas + Wren chat, 2026-05-02) — leniency-tightening sweep underway |

## Consequences

**Positive:**

- Future substrate-class-domain reshapes have a single citable contract. "Does this card violate ADR-028?" becomes an answerable question at gate-arch.
- Order discipline is enforceable, not advisory. The 2026-05-02 commits regression becomes the kind of mistake that gets refused at the gate, not absorbed and rationalized after the fact.
- The Class B mapping makes the contract apply to non-graph domains (commits, builds, deploys) without requiring a graph rewrite.

**Negative:**

- Some existing domains (commits in particular) are mid-arc and partially out of conformance. Recovery work is real and prioritized over reform work.
- The seven MUSTs are demanding; for a small/transient domain they would be over-engineering. The cookbook's "when to reach for this recipe" filter applies — apply only to domains that meet the substrate-class definition.

**Risks:**

- Adoption risk: the contract only earns its keep if it's actually cited at gate-arch. If gate-arch reviewers don't reach for it, ADR-028 becomes another doc nobody reads.

## References

- ADR-025: ontology vs instances graph separation (load-bearing for MUST 1)
- `/loom/principles-reference-impl.html`: worked example
- `/loom/cookbook-substrate-class-domain.html`: the recipe (canonical source for order discipline)
- `designing/docs/commits-service-design.md`: rewritten 2026-05-02 to align with this contract
- 2026-05-02 chorus chats: cards conformance audit (silas ↔ wren), commits conformance audit (silas ↔ kade) — sources for Class B mapping and Addendum 2 specifics
