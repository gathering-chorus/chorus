# ADR-028: Completeness requirements for domains and subdomains

**Status:** Accepted (Silas + Wren co-author, 2026-05-04) — drafted 2026-05-02 in chorus session, reconstructed to disk 2026-05-03 by Silas. Wren co-author review 2026-05-04 verified the three reconstructed sections; patches applied. Plain-language rewrite 2026-05-04 (#2716): retired "substrate-class" / "Class B" / "Move N" jargon in favor of direct names. **Pending: Jeff acceptance.**

**Date:** 2026-05-02
**Builds on:** ADR-025 (ontology vs instances graph separation)
**Source of pattern:** the loom-principles arc (`/loom/principles-reference-impl.html`) + the recipe abstracted from it (`/loom/cookbook-substrate-class-domain.html`)
**Cards bound by this ADR:** #2254 (alerts), #2469 (urn migration parent), #2655 (commits MCP), #2652 (cards CLI), all future domain-completeness work

---

## What this ADR is

A checklist of what a domain — or subdomain — needs to be complete. "Complete" means: a peer can find every part of the domain, write to it through one obvious path, read it through one obvious path, cite it without paraphrasing, and trust that the contents stay in sync with what the code says they are.

This applies to domains whose contents are *governed records* — principles, decisions, ADRs, alerts, cards, commits, practices, policies, services, and the like. It does not apply to domains that hold *runtime data* (photos, music, telemetry); those have different completeness needs.

## Why we need it

Chorus accumulated a dozen of these governed-record domains, each with its own ad-hoc shape. The first one to be done deliberately — principles — proved the pattern transfers: graph as single source, API as only write path, drift test as gate, cite-by-ID never inline, hook backstops the affordance.

But until that pattern is named in an ADR, the next domain reverts to ad-hoc — parallel write paths, paraphrased text in role files, no drift tests, hand-edits to TTL alongside API writes. Jeff named it on 2026-05-02: *"we must not stop until both [cards and commits] conform to our musts list."* That mandate needs the musts list to be a single citable artifact, not embedded inside the principles page where future readers would think it's principles-specific.

The 2026-05-02 commits regression (cards #2661 / #2682 / #2662) showed what happens without this ADR: a refusal hook shipped before the data and APIs underneath it were ready, violating the order discipline below. Without ADR-028 on disk, no card could cite the violation at gate-arch and no gate could refuse it on contract grounds.

## The seven completeness requirements

A domain is complete when all seven hold.

**1. Data layer is separated.**
- Schema (OWL classes, RDFS properties, axioms, SHACL shapes, comments) lives in `urn:chorus:ontology`.
- Records (every populated content row) live in `urn:chorus:instances`.
- These graphs do not mix. Renaming a graph location does not rename the subject — URIs are preserved across migrations.
- Every record declares `rdfs:label` and `rdfs:comment` minimum; class-specific fields beyond.
- Every subdomain links to every record it contains via `chorus:contains`. The reverse `chorus:hasDomain` edge is not enough on its own — the page renderer queries by `chorus:contains` to enumerate, and missing that edge makes the record invisible.

**2. One write path.**
- All mutations go through `POST/PUT/DELETE /api/athena/subdomains/{subdomainId}/{entityType}`.
- `POST` without a label returns HTTP 400.
- Every INSERT/DELETE template declares SPARQL prefixes (`chorus`, `rdfs`, `skos`, `dcterms`, `owl`) via the shared `SPARQL_PREFIXES` constant.
- URIs mint as `https://jeffbridwell.com/chorus#<subdomainId>-<entityType>-<slugify(label)>`.
- Literals escape double-quotes (SPARQL injection).
- No other write surface exists. No direct TTL edits, no SPARQL UPDATE outside Athena handlers, no script-direct writes.

**3. One read path.**
- Canonical reads at `GET /api/athena/subdomains/{subdomainId}/{entityType}` return `{entities: [...]}`.
- Any predecessor read path 308-redirects to canonical.
- No parallel read implementations. Drift starts the moment two read shapes exist for the same data — and one of them is wrong.
- During an active migration window, the subdomain-detail SPARQL UNIONs across both graphs (the SubDomain record may live in `urn:chorus:ontology` while its instances live in `urn:chorus:instances`).

**4. Citations point to records, never paraphrase them.**
- Role fragments contain a redirect-to-canonical and emphasis URIs only. No inlined record text in role files.
- Every consumer (script, hook, page, role fragment) references a URI, not a paraphrased name.
- `claudemd-gen.sh` regenerates role CLAUDE.md after any fragment edit.
- Protocol contract hash unchanged when only role-level emphasis changes.

**5. A hook backstops the old path.**
- Pre-commit rejects staged commits that add records to the file the API replaces.
- The hook is scoped — it ignores files outside its jurisdiction.
- A bypass exists (`<DOMAIN>_DIRECT_EDIT_SKIP=1`) for one-off migration or schema-only commits, and the bypass emits a spine event for audit.
- The shape: typed API becomes the path of least resistance, hook bites the path it replaced.

**6. A drift test guards the contract.**
- A baseline (a static page or snapshot) holds against the live graph; divergence fails CI.
- Subdomain integration tests pass on every commit.
- Hook hermetic tests pass deterministically (no live state dependency).
- Count assertions are floors (`≥N`), never exact — graphs grow.

**7. The subdomain itself is plumbed.**
- The subdomain passes the completeness gate (lifecycle, actors, scenarios declared).
- Meta-alerts wired for: stale instance index, API errors on canonical path, orphan records (records whose subdomain has no `chorus:contains` edge to them).
- If exposed via MCP: single-verb-single-target tools (`<domain>_list`, `<domain>_get`, `<domain>_create`); no `*_op(action, ...)` collapses.
- If exposed via MCP: `X-Chorus-Role` header propagates end-to-end into spine events on every invocation.
- If exposed via MCP: tool descriptions name (a) what it does, (b) when to reach for it, (c) what it is NOT for.

## Order discipline

The seven are sequenced. They are not a checklist to attack in parallel.

- Land them serially, not in parallel. The hook backstop (req. 5) only earns its keep after reqs. 1–4 have made the typed path more attractive than the path being blocked. Skip the order, ship a hook that punishes peers for using the only path they have.
- Ship the drift test (req. 6). It is the most-skipped requirement. The drift test is what makes the contract a contract; without it, paraphrase grows back.
- Be atomic per requirement, not across them. Each card commits one requirement end-to-end. Cross-class atomicity ("decisions and practices together") is sacrificed for testability and reversibility.
- Do not open the refactor phase before the data-correctness phase closes. Restructuring on dirty data carries the dirt forward.

## Three meta-invariants the seven flow from

- **I-1: Graph is the source of truth.** No surface caches. No surface paraphrases. Drift starts the moment a copy lives somewhere else.
- **I-4: One source, multiple call shapes.** REST, MCP, SPARQL, page render — all delegate to the same handler reading the same graph. New transports are surfaces over the source, not new sources.
- **I-10: Atomic per record class, not atomic across classes.** Each migration child (Principle, Practice, Alert, ...) lands as its own atomic unit; cross-class atomicity is explicitly sacrificed for testability and reversibility.

The remaining eight invariants in the canonical I-N list at `loom/principles-reference-impl.html` either collapse into one of the seven requirements (I-2/I-3 → req. 1, I-5/I-6 → req. 7, I-7 → req. 4, I-8 → req. 6, I-9 → req. 5) or are domain-specific (I-11 three-readings is principles-specific).

## Domains backed by an adapter, not a graph

Some governed-record domains have a non-graph source of truth. **Commits** is the canonical example: git refs and objects are the data; `git-queue.sh` is the canonical adapter; the spec doc is the contract. The seven requirements still apply, with this mapping:

- **Req. 1 (data layer separation)** maps to **schema/data separation**: the spec lives at a single citable artifact (e.g., `commits-service-design.md`); data is git refs/objects; no parallel specs.
- **Req. 2 (one write path)** maps to **one canonical adapter**: one named entry point for mutations (e.g., `git-queue.sh commit`, `git-queue.sh push`), sourced by every consumer; no peer-direct path to the underlying substrate.
- **Req. 3 (one read path)** maps to **one canonical query**: one named read surface for state (or an explicit decision to leave reads unmediated, with the gap named — see addendum on validation schemas).
- **Req. 4 (cite-by-ID)** is unchanged: SHAs, branch names, card-ids in messages. Role briefs MUST link to the spec doc and cite SHAs / branch-names / card-ids; MUST NOT paraphrase the spec flow. The 2026-05-02 commits regression had role briefs paraphrasing v2.5 commit flow while v3 was being drafted — paraphrase is exactly the drift this clause refuses.
- **Req. 5 (hook backstops)** is unchanged: the hook bites the path the adapter replaces.
- **Req. 6 (drift test)** maps to **adapter-vs-spec drift test**: a test that asserts the adapter's behavior matches the spec doc; divergence fails CI. This is the most commonly skipped requirement for adapter-backed domains — without it, the spec rots while the adapter accretes ad-hoc behavior.
- **Req. 7 (subdomain plumbing)** is unchanged.

## Addendum on validation schemas

When validation logic for a domain (or its adapter) is spread across multiple sites — a regex in one script, a procedural check in another, a pre-commit hook, a pre-push hook, an MCP tool input schema — the contract is no longer single-sourced. Drift between sites becomes inevitable.

- Validation logic lives in a single declarative schema (JSON Schema, SHACL, Zod, or domain-equivalent), referenced by every site that needs to validate.
- No ad-hoc procedural checks duplicated across hooks, scripts, and tool inputs. If a check needs to fire in N places, the schema fires there N times — but the schema itself lives in one place.
- The 2026-05-02 commits audit surfaced validation in **five** places for the same role-prefix invariant: (1) `branch-check.sh` regex, (2) `git-queue.sh` procedural in `do_commit`/`do_push`, (3) pre-commit hook, (4) pre-push hook (#2598), (5) chorus-api commits-service Zod input + `check_branch` validation in the MCP wrapper (#2641 territory). That five-site duplication is the anti-pattern this addendum names.

## Where active arcs stand against this contract

| Card | Domain | Where in the arc | Status against contract |
|---|---|---|---|
| #2254 | alerts | data-correctness phase, mid | Walking the seven (Wren + Silas conformance audit, 2026-05-02) |
| #2469 | urn migration | cross-cutting | Required for any req-1 work that lands new record URIs |
| #2655 / #2661 / #2682 / #2662 | commits | data-correctness incomplete; refactor-shaped work shipped | Hook backstop (req. 5) shipped before reqs. 1–2 closed. **Order-discipline violation.** Recovery before reform per the cookbook. |
| #2652 | cards CLI | data-correctness phase, mid | Conformance audit completed (Silas + Wren chat, 2026-05-02); leniency-tightening sweep underway |

## Consequences

**Positive.**
- Future domain reshapes have a single citable contract. "Does this card violate ADR-028?" becomes an answerable question at gate-arch.
- Order discipline is enforceable, not advisory. The 2026-05-02 commits regression becomes the kind of mistake that gets refused at the gate, not absorbed and rationalized after the fact.
- The adapter-backed mapping makes the contract apply to non-graph domains (commits, builds, deploys) without requiring a graph rewrite.

**Negative.**
- Some existing domains (commits in particular) are mid-arc and partially out of conformance. Recovery work is real and prioritized over reform work.
- The seven requirements are demanding; for a small or transient domain they would be over-engineering. The cookbook's "when to reach for this recipe" filter applies — apply only to domains that hold governed records.

**Risks.**
- Adoption risk: the contract only earns its keep if it is actually cited at gate-arch. If gate-arch reviewers do not reach for it, this becomes another doc nobody reads.

## References

- ADR-025: ontology vs instances graph separation (load-bearing for req. 1)
- `/loom/principles-reference-impl.html`: the worked example (loom-principles)
- `/loom/cookbook-substrate-class-domain.html`: the recipe (canonical source for order discipline)
- `designing/docs/commits-service-design.md`: rewritten 2026-05-02 to align with this contract
- 2026-05-02 chorus chats: cards conformance audit (silas ↔ wren), commits conformance audit (silas ↔ kade) — sources for the adapter-backed mapping and the validation-schema addendum
