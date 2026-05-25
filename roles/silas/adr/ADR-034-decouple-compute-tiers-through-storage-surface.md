# ADR-034: Decouple Compute Tiers Through the Storage Surface

**Status:** Accepted
**Date:** 2026-05-25
**Author:** Silas
**Cards:** #3081 (this ADR) · #3080 (chorus-api tier split — the reference instance) · references the borg `borg.ttl` Surface model + #3069 (crawlers as instances)

*Two compute tiers decouple only through the data stores; the store's concurrency model is the constraint that says how much decoupling it can buffer.*

## Context

Two pieces of work converged on the same shape on 2026-05-25.

**The borg surface ontology.** `borg.ttl` models infrastructure as three *surfaces* (Jeff's framing, `borg.ttl` §5): **compute** (`borg:Host`, `borg:Environment` — what runs and where), **storage** (`borg:Resource` — what data exists, reached by `persistsIn` / Athena `chorus:Store`), and **network** (`borg:NetworkEndpoint` — how things connect). Each surface is a `borg:Surface` with its own entities and failure modes.

**The #3080 chorus-api tier split.** chorus-api was one Node process hosting two kinds of work on one event loop: *serving* (search, cards, context, nudge) and *ingestion* (crawl, reindex, embed, cache-warm). Any heavy ingestion job froze all serving for every role + Jeff at once — the shared-fate freeze behind the all-session `eventloop.blocked` alerts. The fix (#3080) splits them into an **ingestion tier** (workers) and a **serving tier** (thin API), with the **data stores as the contract**: workers write the stores, the API reads them, *neither tier calls the other*.

These are not two problems. They are one architecture seen twice:

- The two tiers (ingestion workers, serving API) are **compute-surface** entities — `borg:Environment` instances.
- The decoupling buffer between them — SQLite, LanceDB, Fuseki — is the **storage surface** — `chorus:Store` instances.
- "No tier calls the other; the store is the buffer" is the same statement as: **dependency flows through the storage surface, never the compute/process surface.**

The first instinct was to treat #3080 as a one-off chorus-api refactor. But the moment ingestion's only real dependency was named as *the data, not the API process*, it became an instance of a general pattern the borg ontology already half-describes. Capturing it once stops both `borg.ttl` and the #3080 design doc from rediscovering it, and gives every future tier-split a named pattern plus a single design-time test.

## Decision

**(1) Decouple compute tiers through the storage surface.** When two compute tiers must be isolated so one cannot block the other, the dependency between them flows through a shared **storage-surface** entity (a `chorus:Store`), never through a **compute-surface** edge (process-to-process call, in-process function, shared event loop). One tier writes the store; the other reads it; neither invokes the other. The store is the buffer. A process-to-process dependency (HTTP self-call, shared loop, `setInterval` co-tenancy) re-couples the tiers and re-introduces shared fate — it is the anti-pattern this ADR names. (The #3080 crawler curling its *own* API, `GET /api/chorus/crawl`, is exactly that anti-pattern; #3069 removes it.)

**(2) The Store's concurrency model is the constraint.** A storage-surface decoupling is only safe if the store supports the concurrent access the design asks of it. This is a property of the store, so it belongs in the ontology: **`chorus:Store` carries a `concurrencyModel` attribute** with values:

- `multi-reader-single-writer` — many concurrent readers + one writer at a time (e.g. SQLite in WAL mode; readers never block on the writer).
- `multi-writer` — concurrent writers tolerated (serialized or conflict-resolved), e.g. SQLite with multiple writer processes serialized via `busy_timeout`; LanceDB manifest-versioning across writers.
- `server` — a standalone process arbitrates all access; multi-process is the native mode (e.g. Fuseki, Postgres).

The design's required access pattern (how many concurrent readers, how many concurrent writers, per tier) must be **covered by the store's declared `concurrencyModel`, and proven by a spike before migration** — not assumed. The spike is design-time, per the store, reusable across every decoupling that uses that store.

**(3) Reference, don't restate.** `borg.ttl` (the Surface model) and the #3080 design doc reference this ADR for the pattern + the constraint, rather than re-deriving them. New tier-splits cite ADR-034 and supply only their own access-pattern + spike.

## Consequences

**What this enables.** Tier decoupling becomes a checklist, not a rediscovery: (a) name the two compute tiers; (b) name the shared `chorus:Store`(s) they communicate through; (c) confirm the store's `concurrencyModel` covers the required concurrent access; (d) spike it on scratch stores before migrating. #3080 is the reference instance.

**What it constrains.** No compute tier may depend on another compute tier's *process*. If a design needs tier A to call tier B synchronously, either they are not actually separable, or the call must be re-expressed as a store write/read. This is a real limit and the point of the pattern.

**The honest open edge (from #3080 AC4).** The concurrency spikes run so far prove **multi-reader / single-writer**: SQLite WAL (1 writer 153 commits/s + 4 readers ≈43k FTS q/s, 0 `SQLITE_BUSY`, 0 err, reader p99 0.1ms, `integrity_check=ok`) and LanceDB (1 writer + 4 readers, 0 conflict, all appends landed). Fuseki is `server` by construction. **But #3080's post-split design has three SQLite *writers*** (reindex, crawl, embed-mark) — a `multi-writer` access pattern that is **not yet proven**. Per Decision (2), that spike (N concurrent SQLite writers + readers, measuring `SQLITE_BUSY` rate and write-wait p99 under realistic cadence) is required before the multi-writer migrations land. ADR-034 names the constraint; it does not waive it.

**Rollout discipline — the property ships with its consumer (Kade, #3081 review).** ADR-034 *specifies* `concurrencyModel`, but the property is **not declared in `borg.ttl` ahead of a reader** — an unread ontology property rots (the tagging-rots-without-a-gate failure). It lands with its first concrete consumer: either #3080 reading it to decide tier boundaries, or the borg dashboard rendering it (operator value — the store's concurrency model was exactly what was invisible during the crawler exit-5 lock-contention week, #3073). The borg entity renderer iterates a known-property map, so the addition is render-safe when it comes; the gate is *consumer-exists*, not *render-safe*.

**Why an ADR, not just a doc note.** The pattern spans two subproducts (borg, the chorus-api/convergence work) and will recur for any future split (e.g. presentation vs API, harvester vs serving). A decision that two documents both reference needs a single home, or it drifts — `chorus:principle-no-competing-implementations` applied to architecture decisions themselves.

## References

- `borg.ttl` §5 SURFACES (compute / storage / network; `borg:Surface`, `belongsToSurface`) — the surface ontology this pattern formalizes.
- #3080 `designing/docs/chorus-api-tier-split.html` — the reference instance (ingestion / serving tiers, data stores as contract, AC4 concurrency validation).
- #3069 — crawlers as declared instances; removes the API self-call (the compute→compute anti-pattern).
- Athena v2 `chorus.ttl`: `chorus:Store` (`persistsIn`), `chorus:Infrastructure` (`runsOn`) — the structural-layer classes borg's storage/compute surfaces refine.
- AC4 spike harnesses: `/tmp/ac4-sqlite-spike.py`, `/tmp/ac4-lance-spike.js` (single-writer proofs; multi-writer SQLite pending).
