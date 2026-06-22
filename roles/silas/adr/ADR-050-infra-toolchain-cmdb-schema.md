# ADR-050: The Infrastructure / Toolchain CMDB — OWL schema for ADR-033's operational facets

**Status:** **Proposed** — 2026-06-22 (Silas, SA + ops owner DEC-022). Shaped live with Jeff on 2026-06-22, off the back of the morning power-outage incident (electric-company meter swap → Bedroom down → six health nudges that I had to triage *by hand* — `launchctl` / `ping` / `arp` / port-probes — to reconstruct a dependency graph that should already exist as queryable structure).
**Deciders:** Jeff Bridwell, Silas
**Builds on:** ADR-033 (crawler decomposition — the operational facets), ADR-045 (a Domain is an `owl:Class`), ADR-041 (repo tree: ValueStream → Products → Domains → Services), ADR-043 (monitoring + alerting as two domains), ADR-030 (act as orchestration tool).
**Context cards:** #3560 (first proven slice — Fuseki recoverability).
**Design of record (to render):** `designing/docs/infra-toolchain-cmdb-design.html` (AC of #3560).

## Context

ADR-033 decomposed the crawler into a **context-harvest layer (Borg v2)** — per-facet heralds that turn filesystem and operational reality into queryable structure in the graph. Its herald registry already declares the **operational facets** as Silas-owned, write-home = the graph:

| Facet | Source | Write-home |
|---|---|---|
| disk / storage | `df` / volumes | graph (working) |
| network | machines, ports, reachability | graph (working) |
| compute | CPU / load / processes | graph (working) |
| alerts / monitors | borg alert rules + firings | graph (working) |
| logs | Loki | Loki (pull-on-demand) |

**The herald is decided. The domain it writes into was never modeled.** ADR-033 names a *write-home* ("the graph") but no *schema* — there are no `Tool` / `Compute` / `Storage` / `Network` classes for the operational heralds to project into. The `toolchain` domain (#3350) is a bare stub ("dev tools & config"); an `infrastructure` domain does not exist as a modeled class. Result: the operational half has nowhere *typed* to land, so its output is not first-class, queryable, or composable — an **unmodeled Borg reflection domain**.

The cost is concrete. This is a CMDB by any other name (CIs + relationships + service mapping), and the canonical CMDB failure is a hand-curated inventory that drifts into *confident wrongness*. We already own that failure in miniature: the 2026-03-12 hand-drawn topology is stale. The morning incident was the absence of the modeled version — a cascade that a `runsOn × dependsOn` query would have predicted, reconstructed instead by hand after the fact.

## Decision

Model the **operational-context domain as OWL** — the CMDB schema that ADR-033's operational facets project into.

### 1. Two domains, both Silas-owned, each a punned `owl:Class` (ADR-045)
- `chorus:infrastructure` — the substrate.
- `chorus:toolchain` — the technologies that run on it (replaces the #3350 stub's scope).

Each is an `owl:Class` (so the generator projects it) **and** an individual of `chorus:Domain` (so it sits in the ADR-041 tree with `ownedBy` / `atStep`). Member vocabulary binds via `chorus:contains` (ADR-045 §3).

### 2. Infra CI classes = the compute / storage / network triad (Jeff, 2026-04-15)
- `chorus:Host` (**compute**) — a machine. Library (M1, primary), Bedroom (M2 Pro, secondary).
- `chorus:Volume` (**storage**) — a disk / mount. Carries the disk budget (warn 90 % / crit 95 %, C1–C7).
- `chorus:Network` — the LAN (`192.168.86.0/24`, DHCP-volatile).

### 3. Toolchain CI classes
- `chorus:Tool` — one technology / Configuration Item (Fuseki, Loki, Lance, MySQL, Vikunja, MCP, WordPress, act, Prometheus, Grafana).
- `chorus:Toolchain` — a **purpose-grouped composition** of Tools (persistence, observability, coordination, pipeline, web). Carries chain-level facts no Tool can: `provides` (the capability), `recoveryStrength` (weakest-link rollup), `chainHealth`, chain-level `dependsOn`.

### 4. CI relationships (the edges that make it a CMDB, not a list)
- `Tool runsOn → Host` · `Tool persistsTo → Volume` · `Tool reachableVia → Network`
- `Tool dependsOn → Tool | Toolchain` · `Toolchain hasTool → Tool` · `Toolchain dependsOn → Toolchain | Host`
- `Host hasVolume → Volume` · `Host onNetwork → Network`

### 5. Reuse `Service`, don't build a second inventory (no-competing-implementations)
Where a `com.*` LaunchAgent exists, `Tool runsAs → Service` (the existing class) rather than re-describing it. `Tool` adds only what `Service` lacks: `dependsOn`, `backedUp`, `managedBy`, `runsOn`. Non-service items (`act` = CLI, `Lance` = embedded lib, LAN = infra) carry an empty `runsAs`.

### 6. Discovered, not declared (the CMDB-rot defense)
Instances are **projected by ADR-033's operational heralds** (pull-not-push) and reconciled by re-crawl — never hand-curated. The model is the *schema*; reality *populates* it; a reconciler flags the model-vs-actual delta. Same anti-drift DNA as owl-api-generates-from-model: the CMDB is the system's self-model, kept honest by reconciliation, not by a person updating a wiki. A hand-maintained CMDB is worse than none — it is trusted and wrong.

### 7. CI attributes
- On `Tool`: `kind`, `status` (`present | declared | missing | unverified`), `holdsState`, `stateAt`, `backedUp`, `healthProbe`, `managedBy`, `endpoint`, `version`.
- Operations (`backup`, `restore`, `probe`, `compact`) are **verbs run against a CI** — they reuse the MCP verb registry, they are not classes in this schema. Each verb carries `guarantee` (`consistent | bounded | idempotent | reversible | off-machine`) and `verifiedBy` (the probe/test that proves the guarantee holds).

## Consequences

- **Cascade becomes a query.** *"If Bedroom dies, what dies with it?"* = `runsOn × dependsOn` traversal — answerable before the outage, not reconstructed after. The morning's six-nudge scramble becomes one query.
- **Recoverability becomes a typed Storage fact.** "Off-machine backup" = `Tool persistsTo Volume` + `backupTarget → Volume onHost ≠ self`. #3560 is the field `backedUp:false` on one CI flipping true.
- **Composes with the structural facets.** Code / test heralds already write to the graph (ADR-033); the operational CIs join the same graph — one self-model, not a side database.
- **Self-auditing.** *"Every Tool whose guarantee is `off-machine` and status is `missing`"* is the morning's incident as a one-line query.

## Build order (continues ADR-033's)

ADR-033's build order ends at *"more facets → an operational one."* We are there.

1. **Schema** (this ADR): the classes + edges, modeled, proven through owl-api (renders the domain from real instances).
2. **One operational facet end-to-end = #3560 Fuseki slice**: the `Fuseki` Tool + its `Volume` + `backedUp`, discovered live by the storage herald, with `backup` / `restore` verbs bound and `verifiedBy` a tested round-trip.
3. **Fan out** the remaining heralds (compute, network, the rest of the tools) onto the proven schema.

## Not in scope

- **The other unmodeled Borg reflection domains.** "We keep finding unmodeled Borg domains" (Jeff, 2026-06-22) is a real signal that the whole reflection layer is under-modeled — but the move here is to model *one* operational domain well as the exemplar, not to boil the Borg ocean. A deliberate reflection-layer pass is a later, separate decision.
- **Cross-machine harvest engine.** Per ADR-033, the harvest engine runs on Library; operational CIs describe both machines, but the engine stays single-machine.
