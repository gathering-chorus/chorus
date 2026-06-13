# ADR-042: Generator-Layer Security — Gathering-Realm Phase 1

**Status:** Proposed — 2026-06-13 (Silas, SA + ops/security owner DEC-022). Pending: Jeff (final). Kade + Wren converged on the inputs 2026-06-12.
**Card:** #3372 (sibling of ADR-041).
**Inputs:** gathering's working security (`solid-auth.service.ts` + `service-token.middleware.ts`, verified live 2026-06-12) · Kade's `owl-api-research-synthesis.md` (TopBraid blast-radius, dry-run mutation, EDG warn-and-proceed rot) · the owl-api generator (#3354 IoC seam) · the Jeff+Silas SA conversation 2026-06-12.
**Builds on:** ADR-040 (IRI/provenance vocabulary — authored-vs-hydrated, mintKey) · ADR-012 (network-bind security) · `#3356` (DEPLOY_ROLE forgeability) · `#3355` (shared-security spin-off, parked).
**References:** `#2444` (threat model) · `#3373` (the first live localhost-only instance) · `bootstrap-is-security` (#2659 family).

## Context

Most of chorus is not security-scoped at all. Today's model is the **machine boundary**: one user, localhost binds, macOS TCC, two PreToolUse hooks (write_scrubber, sensitive-paths). chorus-api on :3340 is no-auth by design (DEC-093), the MCP tools trust whoever reaches them, cards/DAL/spine writes are open, and `DEPLOY_ROLE` is a stamp anyone can wear (#3356). That was a defensible Gall-simple system while chorus was personal tooling on one box. It stops being defensible at exactly the line Jeff named 2026-06-12: **serious about open source.**

The structural unlock that keeps this from becoming a retrofit-the-audit-that-never-ends: **most unscoped code is already scheduled to die.** The owl-api fan-out (ADR-041) replaces hand-built surfaces leg by leg with generated APIs that are *born* wrapped. Retrofitting auth onto ~200 server.ts endpoints would be securing a building under demolition. So security arrives by attrition, the same way `platform/` empties — **as a generator layer, not per-endpoint code.**

Gathering already built the working version we extend (Gall's law — grow the working simple system, don't design a new complex one):
- **Human lane:** SOLID login → session carries a **WebID** → `authorized-users` maps WebID→role → middleware walks 401/403/role.
- **Agent lane:** `service-token.middleware` — a Bearer **JWT signed with a shared secret**, payload asserting a WebID; the four agent WebIDs already exist as pod profiles (`/pods/jeff/_agents/{wren,silas,kade,bridge}/profile/card.ttl#me`). This lane is **non-interactive by nature** — exactly the daemon/cron identity the system lacks.

## Decision

### 1. Security, logging, and error-handling are GENERATOR layers

The owl-api generator emits routes **pre-wrapped** — authn, request logging, validation, and error shaping inject ONCE at the IoC seam (the #3354 telemetry point), and every generated route inherits them. No per-route wiring, ever. This is #3354's "ilities are structural" extended to security: a generated API cannot be born unauthed once the realm dependency is wired, the same way ADR-040 makes a non-conformant API unable to be born.

**Generator-leg implementation note (Kade's review):** the seam today is `http_response(status, body)` — a pure RESPONSE builder. Authn reads the REQUEST (token/headers) and must run BEFORE response shaping, so the seam signature widens to carry request context (the IoC *point* is right; the seam grows an input). Name this in the generator leg so it doesn't assume the current pure-response signature suffices.

(Logging note carried in: chorus-api stamps local time with a `Z` UTC suffix — every cross-source correlation is silently off 4h; the generated logging layer fixes timestamp discipline once, at the seam.)

### 1a. Off-process worker isolation — separate PROCESS, not worker_thread (Wren's #3382 build finding, load-bearing)

The read path (semantic search + the lance handle) and the write path (embed, #3379) move off chorus-api's event loop. The transport is **a separate OS process** (the #3379 embed-worker model: standalone node, OS-scheduled fair share, niceable) — **NOT** `worker_threads`. The distinction is load-bearing and not stylistic: FTS uses a worker_thread (fts-worker.ts) because its #3079 cost is ONE heavy synchronous SQLite call, and a thread isolates that call. Lance is a different isolation problem — `lance-cpu` is a **process-global native thread pool** doing continuous background saturation; a worker_thread runs in chorus-api's process, so those native threads keep pegging chorus-api's cores and **the wedge survives**. The search worker therefore reuses the FTS pool's request/response *semantics* (injected-spawn, id-correlation, death-recovery) but with the embed-worker's *process isolation* (`child_process.fork` → WorkerLike adapter, not `new Worker`). **Rule: worker_threads is wrong for native-CPU-pool backends (lance); use a separate process.** This is the contract #3382 builds against and #3381's variant search inherits (one off-process lance path, prod + variant, reached via an injected worker endpoint).

### 2. Authn lifted from gathering — Phase 1 is a REALM, not a new project

**Phase 1 (now, what the fan-out depends on):** chorus is configured as a separate **realm inside gathering's working security, in place** — same middleware code, instantiated twice with different config:
- per-entity `SERVICE_TOKEN_SECRET` (chorus verifies only chorus-signed tokens; holding gathering's secret gets you nothing in chorus — the hard boundary under the symmetric scheme),
- chorus identity root `/pods/chorus/_agents/*` (a chorus agent's WebID says *chorus*, not *jeff*),
- chorus's authorized-WebIDs resolved from **the graph** (not a second `authorized-users` file to drift — WebID → Role instance → owned domains, which fuses with §4),
- optional `aud` (audience) claim — tokens minted *for* chorus say so; belt over the separate-secret suspenders.

Same security *system*, two security *realms*. **Zero gathering behavior change.**

**Phase 2 (parked, #3355):** spin off a `shared-security` project + move the SOLID server out of gathering, both products depending on it. Explicitly deferred — Jeff 2026-06-12: "just don't want to do the new project now bc of impacts to gathering." **Forcing event:** when a second consumer beyond chorus+gathering needs the realm, or the open-source cut requires the server to stand alone. The fan-out's generated-API auth dependency repoints to Phase 1, never waits on Phase 2.

### 3. Verified WebID replaces forgeable DEPLOY_ROLE

The agent-JWT lane (§2) is the `DEPLOY_ROLE` killer (#3356): a verified, signed WebID instead of an env-var stamp anyone can wear. The **non-interactive identity path is designed in, not bolted on** — daemons and cron jobs (the 6am tier) present a signed token asserting their WebID; nothing forgeable. Specimen cited: the 2026-06-12 `daily-signal-scan` crash + vikunja-401s were exactly what scheduled jobs with stale/absent credentials look like — the failure mode this path prevents.

### 4. Authz derived from the model, with a provenance-downgrade tooth

Write permission is **derived from model annotations**, not hand-maintained ACLs: the authored-vs-hydrated + `mintKey` annotations (ADR-040) ⇒ who may write what. Authored facets are writable only via the verified-identity DAL; derived facets are **read-only by construction** (the generated read API has no write routes — #3354). Wren's #3351 promotes these annotations onto the Product/Service shapes, so the generator's authz derivation gets them for free.

**The tooth (Kade's research):** the generator/DAL **refuses provenance downgrades** (derived→authored, or a mintKey change) without a witnessed arch/security gate or an explicit override. Backing: TopBraid validates the **blast radius** of a change, not just the payload; Wikidata's bot-vs-human pain is what lane-symmetry-without-teeth looks like. Fold in **dry-run mutation** (write-without-commit returns the SHACL violation report) so agents pre-flight before mutating — the pre-flight that makes the refusal usable. EDG's warn-and-proceed default rotted in the field: **the DAL stays fail-closed, absolute** (warn-and-proceed is the merged≠live twin at the data layer).

### 5. Mixed-state operating rules

Coverage will be mixed (secured generated APIs beside not-yet-migrated hand-built surfaces) for the length of the fan-out. Mixed is livable with four rules — the threads between halves are what made werk v1/v2 and Athena v1/v2 painful, so we ban the thread:

1. **No unsecured twin.** A capability is secured or it isn't — never both. When a wrapped generated API lands, the unsecured surface it supersedes dies in the same leg (ADR-041's rip-out rule doing security duty). The downgrade attack — "ignore the locked door, the old one's still open" — is the one thing that turns mixed into broken.
2. **Token-always clients.** Agents and verbs send the realm token on *every* call from day one. Unsecured endpoints ignore it; secured ones verify it. Callers never branch on which side of the line an API is on — the mixed state is invisible to operations (kills the "is it auth or is it down" triage class).
3. **`securedBy` on the Service class** — an operational property the catalog and Borg render as a ratchet that only moves up ("secured: N/M services, all mutation surfaces covered"). Nobody remembers which APIs are protected; the graph knows.
4. **One mechanism, two postures.** Mixed *coverage* is fine; mixed *mechanisms* are not. Everything secured is secured the same way — the Phase-1 realm.

### 6. Structural expiries, not TODOs

- **localhost-default interim:** first generate-legs ship bound to 127.0.0.1 (owl-api already binds loopback). The expiry is **structural**: once #3355/Phase-1 realm authn is wired, the generator **refuses unauthed generation** — not a TODO, a born-refusal. The interim cannot fossilize.
- **Document content endpoint:** localhost-only **by construction** until generated authz lands — the doc corpus never rides the tunnel (which today exposes only :3000/:3470). Wren's named clause; #3373's CORS comment is cited as the first live instance of the localhost-default-with-structural-expiry pattern.
- **Page serving (from ADR-041 §5):** generated APIs serve their own pages; #3373's cross-origin CORS workaround retires per-leg as each page moves onto its API's origin.

### 7. Evolution notes — each pinned to a forcing event

Gall's law applied to the *target* architecture too: don't build it until the working thing demands it.
- **Asymmetric keys** ⇐ open-sourcing. `SERVICE_TOKEN_SECRET` is HS256 — anyone who can verify can mint. Fine for one box on a LAN; wrong when every generated API verifies in the wild. ADR commits the asymmetric upgrade (signer holds private key; generated APIs verify with public) as the open-source gate.
- **Token issuer** ⇐ the first rotted deploy-time token. Until then, tokens minted at deploy into the LaunchAgent env. A local short-lived-token issuer arrives when a deploy-time token rots like the Vikunja token did 2026-06-12.
- **WebID namespace re-home** ⇐ #1772 (namespace convergence) landing. The agent WebIDs pinned to `localhost:3000/pods/...` re-home to the canonical namespace *with* #1772, so identity migrates once, not twice.

The **scoping triage** (the inventory half of `#2444`): name the real exposure now — what binds beyond loopback, which surfaces mutate state (cards, DAL, deploy verbs, spine emit). Put interim teeth (realm tokens, §2) only on the long-lived mutation surfaces; everything else gets secured by being replaced (ADR-041 attrition). The threat model scopes the LAN/bind posture.

### 8. Network binding — internal services bind localhost (ADR-012 intent, restored) — #3390

ADR-012 decided non-app services bind `127.0.0.1`; ADR-019 (Docker→native LaunchAgent) superseded it and recorded that the binding was **never re-implemented** — services bind `0.0.0.0`, an unintended LAN exposure. This addendum restates the rule in a live home (ADR-012 is superseded; this is its current authority):

- **Default: internal services bind `127.0.0.1`.** A service with no cross-machine consumer must not listen on all interfaces. The generated layer already does this by construction (owl-api binds loopback) — so the hole closes by **attrition** as the fan-out replaces hand-built surfaces (ADR-041); hand-built services bind localhost in the interim via `CHORUS_BIND=127.0.0.1`.
- **Documented LAN exceptions** (legitimately reachable cross-machine / fronted, allowed on a non-loopback bind, each with a reason): `chorus-api :3340` (Bedroom→Library health/ops, ADR-016), `loki :3102` (Bedroom promtail→Library), `clearing-HTTPS :3471` (LAN mic, getUserMedia secure-context, #1782), `caddy :3000` (tunnel front for the app). The cross-machine exception is preserved (ADR-012/016).
- **Regression guard:** deep-health check 17 (#3390) warns on any internal service (mcp, clearing-HTTP, messaging, fuseki, mysqld) listening on `0.0.0.0` — so the decision can't be silently lost again, the failure mode that produced this gap.

## Consequences

- Security stops being a retrofit across N hand-built endpoints and becomes a property of the generator — it arrives free with every fan-out leg.
- The team operates safely through a long mixed-coverage window because callers never branch on it and coverage is an upward-only ratchet.
- The forgeable `DEPLOY_ROLE` (#3356) and credential-less daemons (the 2026-06-12 specimen) both close on the same verified-WebID lane.
- Risk: HS256 symmetric secret is the known weak point — explicitly time-boxed to the open-source forcing event, not pretended away.
- Phase 1 touches gathering's config surface only (new realm), not its behavior — the spin-off (#3355) waits for its forcing event so the extraction is a relocation, not a redesign.

## Status note

Proposed, sibling of ADR-041. Both converged by Kade + Wren on the inputs; final acceptance Jeff's. Phase-1 realm is what #3382 (off-process search) and #3351 (generate-legs) build against; the security domain (`proving/borg/security-trust`) stays the ops home, while security-the-ility lives in the generator.
