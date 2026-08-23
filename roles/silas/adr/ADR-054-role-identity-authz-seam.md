# ADR-054: The authN / authZ seam — role *identity* (Silas) vs role *authorization* (Wren), and giving the roles domain its own home

**Status:** Accepted — 2026-08-04 (#3730; first stated 2026-07-24). Shipped incrementally: identity substrate reproducible (#3726, landed 2026-08-02), DAL identity flip refusing env-trust (#3687 path, exercised live 2026-08-04), seam honored in practice on the 08-04 CLI-identity thread (Kade trace → Silas authN read → Wren authz half).
**Author:** Silas (SA / OWL-DBA), with Wren (roles/authz domain) input
**Card:** #3356 (identity + holdsRole-extraction, Silas) — pairs with a **fresh** roles-domain consolidation card (Wren, to be filed when Jeff points; **not** a reanimation of #1904, which is closed — see "Why this is the fourth attempt").
**Extends:** ADR-052 (Identity via Solid-OIDC / CSS). The prior roles-domain cards — #1904 (Done), #921 (Done), #2320 (Won't Do) — are all **closed**; the domain was validly scoped then and has since **drifted** past the model's current standards (see "Why the roles domain is thin today"). This ADR states the decision fresh against current standards rather than reanimating them.

## Context

All session (2026-07-24) Silas and Wren co-worked the identity seam by instinct — Silas wrote the Principals, Wren built the CSS accounts + Clearing surfaces — but never *named* the line between the security domain and the roles domain. The DEPLOY_ROLE→CSS-identity program forces it, and Jeff cut the line live.

### Current state — grounded from the live model (queried 2026-07-24, not remembered)

**Security (identity) — consolidated, has its own home:**
- Dedicated graph `urn:chorus:domains:security`, one class `chorus:Principal`, **7 instances** (silas, wren, kade, jeff, bridge, chorus-sdk, marknakib), each carrying `chorus:webId` + `rdfs:label`, agents additionally `chorus:holdsRole`.

**Roles (authorization) — modeled but thin, no home of its own:**
- `chorus:Role` class exists with **4 instances** (`role-jeff`, `role-silas`, `role-wren`, `role-kade`), a `roles-domain` athena subdomain is registered — **but the Role instances live in the shared `urn:chorus:instances` graph, not a dedicated `urn:chorus:domains:roles` graph** the way security has one.
- **`holdsRole` currently lives on the Principal side, in Silas's security graph** (`Principal --holdsRole--> Role`). So the binding that is conceptually an authorization fact is physically an identity-graph fact today.

**Why the roles domain is thin today — DRIFT, not fraud.** The roles domain was carded three times: #1904 "roles domain in Athena" (**Done**, Apr 11), #921 "define roles domain" (**Done**), #2320 "spike: extract roles domain as-built" (**Won't Do**). It's tempting (I did it, first draft) to read the two *Done* cards as closed-green-over-unbuilt — but that's **anachronistic** (Wren's review): #1904/#921 are *April*, and the standards that make roles look unbuilt today — per-domain graphs (ADR-051, **Jul 9**) and the `holdsRole` binding (#3618, recent) — **did not exist then**. Judging April work by July standards maligns valid work and, worse, sends us hunting the wrong problem. The honest read: **roles was validly modeled to its April scope, then the model's standards advanced (per-domain graphs, the binding) and roles didn't keep pace.** That's *drift* — a change-management gap, not an acceptance failure. The fix is the same either way (consolidate now against current standards); the *framing* matters because "fraud" would send us auditing closures while "drift" sends us to the real lever: regenerate/re-consolidate domains when the model's standards move.

## Decision

### 1. The seam is authN / authZ (Jeff's cut, 2026-07-24)

- **Role IDENTITY — authentication — Silas (security-trust).** Principal, WebID, credential, JWT mint+verify, the allow-set (is this a real, recognized principal), and the **identity** enforcement primitives at Silas's doors (owl-api verify, the write-DAL #3651). Answers *"are you who you claim."*
- **Role AUTHORIZATION + AUTOMATION — Wren (roles).** The role→capability model, the `holdsRole` binding, and the action path (MCP, verbs, pipelines). Answers *"what may this identity do."*
- **Enforcement is DISTRIBUTED, not all Silas's** (Wren's review, 2026-07-24 — corrects an earlier over-claim). Identity checks fire at Silas's doors; **authorization/action checks fire in Wren's automation** — e.g. `no-self-accept` is `werk-accept`'s `can_accept`, agent-vs-Jeff/guest is the cards bouncer (`sdk.ts`). Neither domain owns "all enforcement."
- **Handshake:** verified identity in (Silas) → authz *decides* (Wren) → **enforced at whichever door the action hits** (identity at Silas's, authz/action at Wren's) → automation executes (Wren). A collaboration point, not a wall.
- **Decidable test:** *"what is silas / what does it own / how do roles coordinate?"* → roles (Wren). *"may THIS principal act as silas?"* → security (Silas).

### 2. The invariant / governed line (the heart of the roles shape)

The frame is **invariant RULES (code) operating OVER governed DATA** — *not* two separate stored buckets (Wren's review sharpened this). The constitution is not a thing stored somewhere-else; it is **invariant logic that evaluates governed data**. `no-self-accept` = the rule `accepter ≠ owner` is invariant code; the accepter/owner *values* are governed data. Even `only-Jeff-authorizes` reads data — "Jeff" is a Principal/Role in the graph; the *rule* "only that principal authorizes" is the invariant. So:

- **Invariant = the rule (code/hook).** Not editable as data. Distributed across both domains' doors (see §1).
- **Governed = the data the rule evaluates** — which surfaces/verbs a role owns, who the principals are. Wren's authz model, evolvable.

**Enforced-in-code vs norm-only — do not claim a guarantee we don't have** (Wren's sharpest catch). The invariants are *not uniformly enforced today*:
- **Enforced in code:** `no-self-accept` (`werk-accept` `can_accept`), agent-vs-Jeff (the bouncer), and — since #3682 (2026-08-23) — `guest-cannot-authorize`: the cards-add door (`enforceGuestCannotAuthorize`, cards SDK) refuses `DEPLOY_ROLE=jeff` attribution when `CHORUS_ORIGIN_PRINCIPAL` names a non-Jeff human Principal, emitting `card.authorization.guest_refused`. Clearing-facing surfaces set the origin WebID from the verified CSS session (post-#3669); absent origin = direct-terminal paths, unchanged. Negative-proof + regression tests: `platform/tests/3682-guest-cannot-authorize.bats`.
- **Historical (closed by #3682):** `guest-cannot-authorize` was norm-only — no code enforced it; Wren upheld it *by judgment* (caught Mark's account-page request being dressed as owner-authorized). Residual honest scope: the accept/go-recording path (werk-demo verdict) does not yet check origin — it inherits protection from no-self-accept plus Jeff's in-person go; plumbing origin there is follow-on, not assumed done.

The line "where invariant ends and governed-data begins" IS the authN/authZ seam. A *rule* that must never be editable-as-data belongs in code (at whichever door); the *data* it evaluates belongs in the roles graph.

### 3. Three concrete moves

1. **Roles gets its own domain graph** `urn:chorus:domains:roles` (Wren, on the fresh consolidation card), placed and shaped per ADR-040 (IRI convention), ADR-045 (a domain is an owl:Class), and the create-each-domain-properly SHACL floor. The `Role` instances migrate out of the shared `urn:chorus:instances` graph into it.
2. **The binding migrates as an inverse `heldBy` (Role-subject), not `holdsRole` (Principal-subject).** Direction is load-bearing, not a modeling detail (Wren's review): a Principal-subject triple *inside the roles graph* would be a cross-graph statement *about Silas's Principals* — awkward and wrong-owned. Re-expressed as `Role heldBy Principal`, every triple in the roles graph is subject-owned by the roles domain (and it matches the "the arrow points the other way — an identity holds a role" reasoning). Silas owns the `Principal` + `webId` it binds **from**; Wren owns the `Role` + the `heldBy` edge. Paired hand-off, verified by a NOT-EXISTS FK check (no binding left in the security graph; every one resolves in the roles graph).
3. **THE ACTUAL DELIVERABLE — switch role derivation from a string parser to a graph query.** Moving the edge is **inert** on its own: role-from-identity is resolved *today* by a **string parser** (`role_from_webid`, owl-api `lib.rs:1122`) that pattern-matches the WebID — it never consults `holdsRole` at all. So relocating the edge changes nothing unless derivation *also* switches to **querying `heldBy`**. That switch — parser → edge-query — is the real work of this program; without it, moves 1–2 are cosmetic. (Caught by looking at the running system, not the model.)
4. **This ADR is the connective tissue** — it names the seam across #3356 (Silas) and Wren's fresh roles-domain card so both build to one boundary.

### 4. Completing the DEPLOY_ROLE→CSS-identity retirement (ADR-052 lane)

- **Silas's half:** extend the #3613 identity seam (ES256/JWKS) to the MCP/verb surface. First plug-in point: the `/mcp` door today trusts a plaintext `X-Chorus-Role` header over loopback (zero crypto), and `service-token.ts` already carries fail-open JWT mint scaffolding — that is the natural seam to turn into *verified-identity-in*.
- **Wren's half:** rewire authz + automation (MCP `X-Chorus-Role`, DAL `DEPLOY_ROLE` attribution, verb attribution) to **consume** the verified identity instead of trusting a declared string.

## Consequences

- The seam is named once; #3356 and Wren's fresh roles-domain card stop both-owning it (the bottleneck-relief this ADR exists for).
- The roles domain gets a real home and stops squatting in the shared instances graph — the model reads honestly (roles are a domain like any other).
- The constitution is documented as **invariant rules (code) over governed data** — with an honest ledger of which are *enforced-in-code* (no-self-accept, agent-vs-Jeff) vs *norm-only* (guest-can't-authorize). This closes the door on "make the authz rules editable data," and surfaces the real gap rather than papering it.
- **Surfaces a live gap:** `guest-cannot-authorize` is enforced by role judgment, not code. This ADR names it; closing it (a hook/gate that refuses a guest-originated authorization) is a follow-on card, not assumed done. *(Closed 2026-08-23 by #3682 — see the enforced-in-code ledger above.)*
- Cost: a graph migration (the binding + `Role` instances relocate, re-expressed as `heldBy`) with an FK-integrity proof, **the parser→query switch in owl-api (the load-bearing part)**, and an ADR-052-lane change to the MCP door. All bounded, all paired.

## Open (pending Wren's fresh roles-domain card + Jeff's continued steer)

- The exact **Role shape** — what fields `chorus:Role` carries in its new home, and the capability-model vocabulary — is Wren's model call (the fresh card), not decided here.
- The precise **catalogue of invariant rules**, and — for each — **whether it must be enforced-in-code or may stay a documented norm**, needs Jeff's steer. This ADR fixes the *principle* (invariant-rule-over-governed-data) and names the one live gap (`guest-cannot-authorize` is norm-only); it does not settle the full enumeration or which gaps get carded for enforcement.
- **The `#3669` served-doc residual** — CSS may still store absolute `localhost` subject-IRIs *inside* served WebID profile documents (the baseUrl flip can't rewrite already-stored pod docs). This is **separate** from the allow-set (all 7 Principal.webId are verified public-origin, reconciled by #3669 legs 4/5). Whether the residual was closed in the #3669 land is **not verified** either way — confirm separately; do not assume closed.

_Related: ADR-052 (identity via Solid-OIDC), ADR-040 (IRI convention), ADR-045 (a domain is an owl:Class). The prior roles-domain cards #1904/#921 (Done) and #2320 (Won't Do) were validly scoped to their April context and have since drifted past current standards; this ADR states the decision fresh rather than reanimating them._
