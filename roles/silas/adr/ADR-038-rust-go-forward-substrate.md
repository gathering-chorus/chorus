# ADR-038: Rust as the go-forward language for substrate-grade code

**Status:** **Accepted** — SA (Silas) + COS (Wren) both ACCEPT, 2026-06-08. DE-drafted (Kade), SA-sharpened (cdhash-primary discriminator + no-runtime-in-trust-path; tag-not-glob enforcement; server.ts exemption + chorus-deploy/#3285 port-debt), COS-confirmed (teeth-on-what-we-already-operate-by; no Node-tier migration; case-by-case port). **The decision is sound now; the enforcement teeth are NOT live until #3291 (mapsTo) phase 1 ships** — see Enforcement. Merge the decision; teeth follow #3291. (Silas owns the merge.)
**Date:** 2026-06-08
**Author:** Kade (distinguished-engineer draft)
**Reviewers:** Wren — chief-of-staff / product lens · Silas — systems-architect lens
**Builds on:** ADR-032 (verb-contract-v1 — verbs are zero-dep Rust binaries), ADR-031 (naming-grain convention + the "teeth = a CI check" principle), ADR-036 (one build-deploy execution path)

## Context

The codebase is polyglot **by accretion, not by decision.** Two tiers coexist:

- **Rust substrate** — the werk verb binaries (ADR-032: zero-dep, signed, cdhash-tracked), `chorus-hooks` (the guard daemon).
- **Node/TypeScript services** — `chorus-api`, the MCP server, the web app, Clearing, Pulse.

The team already *operates* on a "new substrate code is Rust" assumption — but it has never been written down, so it has **no teeth and drifts.** The evidence is concrete and surfaced this session while rewriting the Version Control service design:

- `chorus_unpull_card` is a non-Rust MCP stand-in that **never got ported** — there is no `werk-unpull` Rust verb, even though it is the literal inverse of `werk-pull`.
- `chorus-werk-sync` is a **Bash script** where a `werk-sync` verb belongs.

Every "should this be Rust?" is re-litigated from memory, and the stand-ins stay invisible because no captured rule names them as debt. Per ADR-031: a convention is only real if something checks it.

**This ADR draws the line. It does NOT mandate rewriting the Node tier.**

## Decision

**Rust is the default implementation language for new substrate-grade code.**

1. **Substrate is Rust.** A component is substrate-grade — and therefore Rust, on the werk-pull blueprint (ADR-032) — when **both** hold: (a) it must carry a **stable signed identity (cdhash) across rebuilds**, and (b) it runs **without a language runtime in a hot or trust path.** The cdhash test is primary and non-arbitrary: macOS TCC binds permissions to a binary's cdhash (#2734), and only a compiled binary holds a stable signed identity across rebuilds — Node and Bash have none. The runtime test is its operational twin: hooks fire on every tool call, and verbs run under `act` / launchd where no Node is available. Concretely this captures the werk verbs, git/lifecycle operations, hooks and guards, signed CLI binaries, and coordination primitives **that run in the trust/deploy path.** New code of this class in another language is a violation.

   **Exempt by the same test:** a long-lived **Node daemon that holds no cdhash dependency and serves a request/response surface** — e.g. the MCP server / `server.ts` — is a coordination primitive but is **not** substrate-grade. It never needs a stable signed identity and runs in its own persistent runtime, not a hot trust path. The property test exempts it; this prose exempts it too, so "coordination primitive" cannot be read to pull it in.

2. **The Node/TS tier is NOT migrated wholesale.** The web app, the HTTP/API surface, and the MCP server stay TypeScript. Their value is ecosystem + velocity + existing investment; a request/response web tier does not need the properties Rust buys, and rewriting it for purity is cost without benefit (the Staples integration-trap lesson — do not move what works to satisfy a principle).

3. **The discriminator is the *property*, not the *layer*.** A thing is Rust when it must be a signed, zero-dep, long-lived binary in the trust/deploy path. It stays or becomes TS when it is request-scoped web/API glue. When a TS component *starts* needing substrate properties (signing, hermetic deploy, cdhash provenance), that is the trigger to port it — case by case, with a card, never a blanket migration.

4. **Existing non-conforming substrate is debt, ported one at a time.** Known port-debt: `chorus_unpull_card` → `werk-unpull` (Rust); `chorus-werk-sync` → `werk-sync` (Rust); and **`chorus-deploy`** — the largest substrate-grade Bash engine — whose in-flight Rust port is **#3285** (Silas). Each is rebuilt on the blueprint, the old name shipped as a deprecation alias per ADR-031's rollout. No big-bang: ADR-038 *names* the debt; #3285 pays the largest piece, and the two reinforce each other.

## Enforcement (teeth)

- **Rust-crate paths are path-enumerable — enforce now.** A CI check on `platform/services/werk-*`, `chorus-hooks`, and `chorus-inject`: a non-Rust source file added under those paths fails the gate. This is the gate of record, not a loom-shelf document (ADR-031).
- **`platform/scripts/` is NOT path-enumerable — do not blanket-glob it.** It holds *both* substrate-grade scripts (the port-debt above) *and* legitimate Bash glue (env-setup, wrappers, ops one-offs); a path rule would false-flag the glue.
- **The durable mechanism: enforcement is a graph query, not a glob.** It reads the **#3275 instance model** (skills / hooks / verbs as graph instances, each with `instanceType` + `ownedBy` + `status`). Each instance carries **`mapsTo: Iri|path`** → its source artifact (verb → crate dir, hook → rust module, skill → skill dir). The CI check is a **reverse lookup**: a changed file → the instance whose `mapsTo` path-*prefixes* it (**longest-prefix-wins** on nesting; an ambiguous prefix is flagged "needs disambiguation") → read its properties. **Both decision inputs are DERIVED, never hand-tagged:** substrate-grade = `instanceType ∈ {verb, hook}` (the cdhash/runtime test of decision #1, made structural), and language is derived from the `mapsTo` artifact. Rule: a substrate-grade instance whose artifact is not Rust **fails**; an instance with no `mapsTo` surfaces as "needs mapsTo" — visible, never silently unenforced. (Co-designed with Wren, 2026-06-08.)
  - **Cardinality (Wren + Silas, 2026-06-08):** the file→*instance* lookup this enforcement uses is **1** (longest-prefix-wins — "is this substrate-grade" needs exactly one answer); the parallel file→*domain* lookup the crawler uses is **1:N** (a file carries multiple domain edges — the coupling / monoculture signal). Both read the *same* declarative `Domain.hasMapsTo: [prefix]` rules in the model — one home, no parallel store. Longest-prefix-wins resolves the instance answer **without** collapsing the file→domain relation to 1:1.
- This extends ADR-031's CI check from the *name* axis to the *language* axis.
- **Teeth are not live yet.** The substrate-grade CI check depends on the instance↔artifact join — #3275 (the instance model, landed) + **#3291 (`mapsTo`, in flight)**. Until #3291 phase 1 ships there is no live enforcement: ADR-038 is a sound, accepted *decision*, and the Rust-verb family is the conformance target, but the gate that *enforces* "substrate-grade ⇒ Rust" goes live with #3291. Naming this so the ADR doesn't read as having teeth before the join exists.

## Consequences

- A citable, enforced answer to "should this be Rust?" — **yes** for substrate, **no** for the web/API tier, **port-when-properties-demand** at the boundary.
- The TS/Bash substrate stand-ins (`unpull`, `sync`) become *tracked port-debt* instead of invisible drift.
- **Cost, named honestly:** porting has a real price; this ADR explicitly avoids mandating it where the benefit is absent — no purity-driven rewrites of the working Node tier.
- **Risk:** "substrate-grade properties" requires judgment at the boundary. The CI check enforces the clear cases (substrate paths); boundary cases go through architecture review.

## Open questions for the reviewers

- **Wren (COS):** Does the line — substrate=Rust, web/API=TS, port-on-property-demand — serve the product and coordination goals, or does it over-constrain delivery velocity? Is "case-by-case port with a card" the right grain, or does it need a standing migration backlog?
- **Silas (SA):** Is "needs zero-dep / signable / hermetic properties" the right discriminator? Are the substrate paths cleanly enumerable for a CI check? Does this compose with ADR-036 (one build-deploy path) and ADR-032 (the verb blueprint) without conflict — i.e., is there any substrate-grade thing today that is legitimately *not* Rust and would be mis-flagged?
