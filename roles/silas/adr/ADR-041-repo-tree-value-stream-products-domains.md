# ADR-041: Repo Tree — Value Stream → Products → Domains

**Status:** Accepted — 2026-06-13 (Silas, SA). Landed via #3372 (Kade + Wren design-review converged; Jeff GO 2026-06-13). **Amended 2026-09-24 (#4288, Jeff): domains, services and skills are peers of products under the value-stream step; see the Amendment section — it supersedes §1's nested tree.**
**Card:** #3372 (authors this + ADR-042 together — Jeff: "we need both, they can be in one card").
**Inputs:** Kade's `chorus-project-structure.html` (the worked tree, DE-hat session 2026-06-12 ~08:53–09:25) · Wren's `#3371` (directing-branch redraw) + `athena-product-design.html` (10-section template + value-stream OWL) · the 3-role convergence threads.
**Builds on:** ADR-040 (IRI-formation — the IRIs this tree mirrors) · DEC-1816 (top-level peers-not-nesting, superseded for the product layer by this ADR) · `#2640`/`#2913` (role-directory-IS-session-start, preserved here).
**Absorbs:** `#1841` (decompose `platform/`) — closes into this ADR; its execution becomes the first move-card under the inventory pass (`#2292`).

## Context

The repo has duplicate homes for the same concept — products land in three places, config in three, launchagents in two, three PM directories, retired role aliases never removed, a near-empty `building/` while the real build substrate sits in `platform/`. It is the no-competing-implementations disease (`chorus:principle-no-competing-implementations`) at the folder level.

This matters **now** because the owl-api fan-out (#3351 generate-legs, the crawler-rewrite leg #3185/#3242) is about to generate a domain API + page per class. If those land into today's scattered tree, they ingest into — and generate from — hollow or duplicated homes, and the graph the crawler validates against has no single physical truth to compare to. The tree must be decided before the fan-out writes, not after.

The fix is **one decided tree where the directory structure mirrors the Athena graph** — so the crawler can validate tree-vs-graph and drift fails loudly instead of accreting. This is the canonical-model discipline (Jeff's Staples lineage) applied to the filesystem: the value-stream/product/domain coordinate (ADR-040's IRIs; the legibility coordinate of `#3145`) is the one axis every surface projects.

Framing (Kade's, kept): **Chorus is "Backstage for agents," one generation deeper.** Backstage = software catalog + ownership + docs + golden-path templates for human developers. Chorus has each piece a generation down: descriptors are OWL assertions + frontmatter (not `catalog-info.yaml`), the crawler ingests them into the graph, and the graph generates the catalog, the APIs, and the pages. The catalog is the grounding surface the agents run on.

## Decision

### 1. Three levels, value-stream at the top — four peers under each step (amended 2026-09-24)

```
chorus/
├── README.md · LICENSE · CONTRIBUTING.md          ← open-source front door
├── catalog/                                        ← GENERATED from the graph
│                                                     (products/domains/skills/services .md)
│                                                     CI: catalog == graph == tree
│
├── <step>/                                          ← shaping · designing · directing · building · proving
│   ├── products/<product>/     the COMPOSITION: which domains, services, skills this product is made of;
│   │                           its pages, its product-level tests. Never the parts themselves.
│   ├── domains/<domain>/       the model (OWL+SHACL+tree.json) and the code athena-make generates from it.
│   │                           ONE home per domain; a domain two products share lives here once.
│   ├── services/<service>/     implemented units (binaries, daemons, launch scripts)
│   └── skills/<skill>/         reusable procedures (/verbs), usable by more than one product
│
├── shaping/     products: loom            domains: principles · practices · policies · decisions · roles ·
│                                                    analytics · metrics        skills: the framework skills
├── designing/   products: athena          domains: domains(the model) · services(design corpus) ·
│                                                    knowledge(doc corpus, doc-catalog) · domain-context
├── directing/   products: clearing ⊃ pulse ⊃ spine (child products, §2)
│                                          domains: cards · priorities · messages · heralds · senses ·
│                                                    working-memory · events · memory · time
├── building/    products: werk · convergence
│                                          domains: version-control · cicd · builds · deploys · pipelines ·
│                                                    toolchain · code · tests · integrations · pods⚠
├── proving/     products: borg            domains: gates · alerts-monitors · logs · rcas · properties ·
│                                                    security-trust · infrastructure
│
├── lib/                                             ← minimal, shrink-only ratchet
└── roles/{wren,silas,kade}/                         ← session-start anchor ONLY
```

The original 2026-06-13 tree nested `domains/` under each product (`<step>/products/<product>/domains/<domain>`). That shape is superseded by the Amendment below; the step → product and product → domain *assignments* above are unchanged, only the folder where a domain lives moved up one level.

**Five value-stream steps only:** shaping, designing, directing, building, proving. **Operating is not a step** — it folds into proving (Jeff, 2026-03-27: "operations are part of proving — deploys, builds, test runs"). v1's "Operating" domains split by side: build-side verbs (builds, toolchain, pipelines, cicd) → `building/werk`; run-side (deploys, infrastructure, alerts-monitors, logs) → `proving/borg`. Convergence builds integration fabric → `building`.

All 34 generated-API domains (live from owl-api :3360) are placed. The four weakest placements are flagged ⚠ in Kade's source doc (heralds, streams→folded, time, pods); their resolution is below.

### 2. The directing branch — Jeff's redraw (`#3371`)

`clearing` is the top-level coordination product; **`pulse` and `spine` are child products under it** (recursive `hasChild`, Jeff twice: 2026-06-04 and 2026-06-12). This dissolves the "what step is spine" question — spine inherits directing through clearing.

- **clearing** (parent product, Wren): domains `cards`, `priorities`, `messages`, `heralds`, `senses`.
  - `priorities` = a **distinct domain** (the operating layer — headings/readout above the board); not yet a graph domain, Wren mints it via DAL on her #3351 leg. Verbs persist as skills.
  - `heralds` = a **distinct domain**: delivery **contracts + skills** over the ONE delivery engine; it is **never a second delivery path** (Wren's pin). The engine lives in pulse/spine; heralds names the contract.
  - **Traceability note (divergence from #3371, both concur):** #3371's redraw *suggested* folding heralds→messages and priorities→cards. This ADR keeps them as distinct domains instead — heralds-as-contract is not messages-as-transport, and priorities-as-operating-layer is not cards-as-work-units; both are cleaner separated, and Wren's one-engine pin survives unchanged. The reversal is intentional, recorded here so it reads as a decision, not a drift.
  - `senses` = the I/O skills domain — listening (`/listen`), seeing (`/look`, `/lc /lk /ls /lw`), speaking (`nudge`). Under **clearing**, not spine (Jeff re-ruled 2026-06-12: "ok on senses, i agree"). Skills live under the domain they operate.
- **pulse / working-memory** (child product): the **home of the hot coordination state** — domains `messages`, `streams`, `alerts`, `cards` as live registers. Not registers-by-reference over state homed elsewhere; pulse IS the now (Jeff's #3080 instinct: "isn't the data store also the pulse"). Short-term/working memory (his 2026-05-24 seed); **not** "attention" — attention is Loom practice over senses + working-memory, never a data domain.
- **spine** (child product): `events` (the durable signal trunk — emit lib, registry, chorus.log), `memory` (long-term — index.db, semantic search, role memory), `time` (wall-clock, cadence, calendar).

**Two boundary pins (`#3371`):**
- **Board state vs history:** the hot board lives in pulse (working-memory); history ages out to spine + the graph. One card is hot in pulse and durable in spine — same fact, two lifetimes, not two homes.
- **Alerts firing vs rules:** a *firing* alert is pulse state (working-memory); the alert *rule* is a borg artifact (proving/alerts-monitors). Render-vs-definition, the ADR-035 boundary applied to alerts.

### 3. Value-stream OWL (from `athena-product-design.html`)

The tree mirrors the graph, and the graph's value-stream layer is the schema: `ValueStreamShape` / `StepShape`, with `inStream` (a step belongs to a value stream) distinct from `hasValueStream` (a step *encapsulates* its own nested value stream — Jeff's recursion rule, e.g. Building →hasValueStream→ werk's verb sequence). A fold renders `hasValueStream` as the recursion edge. **Predicate discipline (carried from the 2026-06-12 mint ruling):** `contains` = content membership (Domain → record instances); `hasChild` = structural recursion (domain→domain, product→product); `hasValueStream` = step→nested-stream. They are folds, never interchangeable — `messages hasChild heralds` is structural, not containment. The v0 generator only emits a `/contains` fold today; the `hasChild` fold is a route in Wren's #3351 shapes leg.

### 4. Rules

- **One home per concept.** Every duplicate (3× products, 3× config, 2× launchagents, 3× PM dirs, role aliases) retires into exactly one place. A move-card that leaves the old copy alive is **not done** — it carries a retirement gate (`[[feedback_retirement_gates_are_structural_memory]]`), the same structural-memory pattern as the tombstones in #3380 and the ratchet in #3370.
- **Tree fills by ATTRITION, not a bulk rewrite.** New work lands in its final home from day one; existing pieces relocate on-touch; a grep-gate blocks new accretion into retired homes. `platform/` empties by attrition — each fan-out leg generates a domain's API + page and rips the superseded `chorus-api` surface (server.ts shrinks leg by leg); `platform/services/*` crates are already atomic and just relocate.
- **Flat views are GENERATED catalogs, never symlinks.** `chorus/catalog/` is a generator deliverable (a fan-out leg), emitted from the Athena graph with one real path per entity, sharing ONE taxonomy source with the future board hierarchy (`#2159`) — one model, two renderings (repo md + board). Symlinks are rejected: they re-materialize broken on checkout (the `workflow-engine/dist` self-loop outage class), render dead on GitHub, and create competing import paths.
- **Roles are orthogonal to the tree.** The value-stream hierarchy is the WHAT; roles are the WHO — an ownership axis carried in the graph (`ownedBy`) and rendered in the catalog, **never a directory level**. `roles/{wren,silas,kade}/` exists only as the session-start anchor (role state, briefs, memory) — the protected primitive from `#2640`. Role artifacts that are really domain artifacts (docs, ADRs, dashboards in role dirs today) migrate to their domain homes; what a role "shows" is a generated page over the domains it owns, not a folder it keeps.
- **Minimal `lib/`, shrink-only.** Shared libs (chorus-sdk, the spine emit contract) live in the smallest possible `lib/` with a shrink-only ratchet — NOT dissolved into consumers (that recreates the five-private-copies drift this tree exists to kill; Wren + Kade converged here against the dissolve option).
- **Model-churn = repo-churn, acknowledged.** Because the tree mirrors the graph, an ontology change is a directory change. The four ⚠ placements (heralds, streams, time, pods) are named so nobody relitigates them mid-move; the no-move-before-ADR + each-move-retires-its-source rules are the guard against the half-migrated state (the werk v1/v2, Athena v1/v2 pain class).

### 5. Open calls — decided

- **Spine's step:** directing (converged — spine is the ledger, proving is the auditor; Borg consumes the witness, doesn't own it). Resolved structurally by #2: spine is a child of clearing, inheriting directing.
- **Pulse's home:** own child product under clearing (revised from "own domain under spine" by Jeff's #3371 redraw).
- **Page SERVING home (Wren's open flag):** **the generated API serves its own page.** Each owl-api instance already owns its read surface and its IoC seam (#3354); serving the page from the same process keeps page-and-data on one origin (no cross-origin hop, no second deploy target), and the rip-out rule stays clean — the page dies with the hand-built endpoint it replaces, in the same leg. `:3340` static serving is retired per-leg as each domain's generated API takes over its page. Same-origin means **no CORS header at all** — #3373's permissive `*` workaround *deletes itself* rather than narrowing (Kade confirmed: it existed only for the :3340-page/:3360-data cross-origin split). **Generator-leg scope note (Kade):** owl-api gains a new capability it lacks today — serving static HTML, not just JSON. Small but real new scope, named for the generate-leg.

### 6. Sequencing teeth

1. **ADR-041 lands first** — before #3351's generate-legs write any path, before the crawler-rewrite leg (no hollow homes for it to ingest), before any move-card.
2. `#2292` inventory pass tags every existing piece with its destination path (its first output absorbs #1841).
3. Move-cards execute as lifts, each retiring its source; fan-out legs land their generated chunks directly in the new homes.
4. The **tree-vs-graph CI check** — the #3354 conformance-walker pattern applied to the filesystem — lands in the crawler-rewrite leg (Silas) so enforcement has a named owner and vehicle, not just an aspiration: the crawler walks the graph, the check compares it to the tree. **Attrition-direction tolerance (Kade's review):** during the fan-out both half-states exist transiently (a dir staged before its graph node; a generated node before its dir), so the HARD-fail is scoped to the direction that matters — *every GENERATED domain must have its directory* — while the reverse (dir-without-node) stays ADVISORY until a "tree complete" milestone. Same lesson as #3376: an in-progress state must not read as broken.

## Amendment 2026-09-24 — peers, not nesting (#4288)

**Jeff, 2026-09-24 10:24–10:38:** "i feel like athena-make is our path to make our repo work /chorus/<vs step>/products/<product> i think domains and services are peers of products" · "> 1 product may depend on a single domain i think skills may be another child of vs step" · "update the adr".

**Why the change.** Two products can depend on one domain (Jeff, 2026-05-14: "a product is a composition of domains and implemented services"). Under the nested tree that domain would need a copy or a symlink, and §4's first rule forbids both. So the domain is a peer of the product, and the product folder holds the composition, not the parts. Skills have the same shape — reusable across products — so they are a fourth peer.

**Decision.**
1. Under each value-stream step there are exactly four children: `products/<product>`, `domains/<domain>`, `services/<service>`, `skills/<skill>`. Nothing nests under a product except what belongs to that product alone (its pages, its composition file, its product-level tests).
2. A domain has ONE home, `<step>/domains/<domain>/`, holding its model and the code athena-make generates from it. A product names the domains it is composed of in the graph (`partOf` / `hasChild` edges) and the catalog renders that; the folder never repeats it.
3. The tree is a build-time projection of the model (ADR-061, 2026-09-24): `athena-make generate-target` computes the path from the class's chain, `RepoKind` gains `Skill` with collection `skills`, and the chain is step → kind → name — no product segment above a domain. A file outside its projected path is drift the crawler reports (#4287), not a domain it guesses.
4. Child products (§2: clearing ⊃ pulse ⊃ spine) stay products; their domains live under the step's `domains/`, not under the child product.

**What this retires.** The nested `products/<p>/domains/<d>` shape; any tree-vs-graph check (§6.4) that assumed it; the `roles/kade/ontology/surface-domain-4222.ttl` directory rows that encode product-nested paths, on the move-card that relocates each domain.

**What it does not change.** §4's rules (one home, attrition not bulk rewrite, generated catalogs never symlinks, roles orthogonal, minimal lib, model-churn = repo-churn) and §6's sequencing all stand; the move-cards under §6.3 now lift each domain to `<step>/domains/<domain>/`.

**Open, decided on the first move-card:** whether `services/` holds every implemented unit or only those not owned by a single domain (a domain's own generated API lives with the domain; a shared daemon such as chorus-hooks lives in `services/`).

## Consequences

- The fan-out starts landing into final homes immediately — no rework, no second migration.
- `platform/` becomes a temporary holding pen that empties by attrition; nothing forces a risky big-bang move.
- The crawler gains a filesystem to validate the graph against (and vice-versa) — tree-vs-graph drift becomes a loud CI failure.
- Risk: model churn now churns the repo. Mitigated by the four named ⚠ placements + the no-move-before-ADR rule; we accept that an ontology change is a reviewed directory change, which is the point (legibility), not a bug.

## Status note

Accepted 2026-06-13; amended 2026-09-24 (#4288). Originally: Proposed. ADR-042 (security) is its sibling from #3372. Both reviewed-converged by Kade + Wren on the inputs; final acceptance Jeff's.
