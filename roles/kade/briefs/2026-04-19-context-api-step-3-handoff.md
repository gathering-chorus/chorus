# Brief — Context API Step 3 Handoff (#2234)

**From:** Silas
**To:** Kade
**Date:** 2026-04-19
**Re:** #2234 Step 3 — common envelope implementation, minimum Context endpoint set

## What happened

Jeff and I designed the pull-model Context API shift this morning under #2234. Full session arc: push-model pulse delivery → agents routinely form claims that diverge from delivered data → move Chorus API "from attic to workbench" → ESB-style common envelope, canonical-metadata-stamped, URL taxonomy by agent question.

Steps 1, 2, 6 of the implementation outline are done as design docs. Step 3 is yours — Services vertical, implementation.

## Read these first

1. `designing/docs/chorus-overview.md` — refreshed service design (three sub-domains named, verticals from v2 diagram, trinity lens, common envelope as cross-cutting pattern)
2. `designing/docs/context-service-design.md` — sub-domain design in Wren format; Promise / Overview / Components / sub-domain interactions
3. `designing/docs/context-api-endpoint-audit.md` — 136 endpoints classified; the target-shape section maps every current endpoint to a new taxonomy slot
4. `designing/docs/context-endpoint-schemas.md` — **this is your implementation spec for Step 3.** Three minimum endpoints with declared response shapes, OpenAPI fragments, SPARQL-stamp helper pattern
5. `designing/docs/context-push-envelope-reshape.md` — what the inject looks like after pull endpoints exist (design only; implementation is your Step 3 + follow-on)

## Step 3 scope (what to build)

Minimum Context endpoint set — three endpoints. Not the full reshape. Proof-of-shape before scale.

**New handler files** (`platform/api/src/handlers/`):
- `context-board-wip.ts` — `GET /api/chorus/context/board/wip?role=X`
- `context-roles.ts` — `GET /api/chorus/context/roles`
- `context-health.ts` — `GET /api/chorus/context/health`

**New shared library** (`platform/api/src/lib/`):
- `context-envelope.ts` — envelope builder + SPARQL-stamp helper (`stampHeader(domainId) → Promise<ContextEnvelope>`)

**Schemas:** per `context-endpoint-schemas.md`. Ship OpenAPI fragments inline or adjacent. Don't skip — the doc-as-deliverable discipline is part of the ship condition.

**Tests:** hermetic per TEST.md binary rule. Fixture TTL for SPARQL stamping (following #2208 pattern). Tests under `platform/api/tests/handlers/context-*.test.ts`.

**Aliasing (not removal):**
- `GET /api/chorus/pulse` → keep; eventually deprecate in favor of composite `/context/pulse` (later step). Don't remove.
- `GET /api/chorus/role-state` → alias to call `/context/roles` internally, same response; mark deprecated in response headers or doc.

**Don't touch** (explicit out-of-scope for Step 3):
- Envelope inject on the hook-shim side — Step 6 design; implementation is Kade + Silas collaboration once Step 3 lands.
- Full Memory or Knowledge endpoint migration — separate cards.
- Removal of `/api/athena/*` or `/api/chorus/domain/:name/*` — deprecate later, don't remove.

## Design questions to resolve at implementation time

Two items I flagged in the design docs as open — pick during impl:

1. **Where does the SPARQL-stamp query live?** Option A: inline in `context-envelope.ts` (each lookup builds a query string). Option B: a named query file under `platform/api/src/sparql/queries/context-envelope-stamp.rq` loaded once. B is cleaner for the "named-query-not-ad-hoc-SPARQL" discipline from the CMDB memory. Lean B.

2. **Test fixture scope for SPARQL stamping.** Option A: one fixture TTL per endpoint test. Option B: a shared `platform/api/tests/fixtures/context-envelope.ttl` that every Context handler test uses. B is cheaper to maintain. Lean B.

Both are your call at impl time.

## Trinity check for ship

Per `chorus-overview.md`'s evaluation lens — Step 3 ships when:
- **Reliable.** Endpoints return correct data in predictable latency. Common envelope stable. Stamps resolve from graph without NPE when metadata missing (graceful-absent fields, not crash).
- **Reused.** Both agent inject manifest (from Step 6 reshape) AND Athena UIs can call `/context/*` — same API, one read path.
- **Valuable.** Demonstrable that an agent response cites `/api/chorus/context/board/wip?role=silas @ timestamp` instead of inventing. That's Step 7 demo.

## Measurement / AC for Step 3

Suggest AC shape for your build commit:
- [ ] Three handler files created, routes registered in server.ts
- [ ] `context-envelope.ts` shared helper; every new handler uses it
- [ ] OpenAPI fragments shipped inline with handlers (or adjacent .yaml)
- [ ] Hermetic tests, 0 skips, passing on default jest (per TEST.md)
- [ ] Existing `/api/chorus/pulse` + `/api/chorus/role-state` still functional (aliased, not removed)
- [ ] Canonical-metadata stamping verified: hit `/context/board/wip?role=silas`, envelope carries `product: chorus` stamped from graph

## Handoff rhythm

- Read the five docs. Flag push-backs on shape BEFORE implementation — cheaper than mid-build.
- Pair or chat if you want Silas in the design decisions (e.g., SPARQL-stamp helper signature). Otherwise solo-with-review.
- When a handler is green, nudge me for a quick eyeballing of the actual envelope shape vs. the schema — faster than waiting for full gate chain.
- Steps 4 + 5 (data correctness + presentation reshape) follow naturally from Step 3. We'll scope them after the three endpoints land.

## Why this matters

The Context API shift is the direct fix for the "agents hallucinate state despite data being in context" failure mode Jeff named today. The weakest link in the Shared Awareness layer today isn't data availability — it's consumption. This rebuilds the consumption path.

It also puts interface-design-as-sustained-practice on the ground, not just on paper. The OpenAPI-shipped-with-handler discipline sets the pattern for every Chorus endpoint after this. Your Services vertical ownership makes that disciplinary investment load-bearing.

---

References:
- #2234 card
- #2231 — caching that composes with envelope shrink
- #2217 — ceremony audit surfaced OBS2 (session.context.built) as the inflection
- #2208 — fixture pattern for hermetic SPARQL tests
- US patent 9,552,400 B2 — ESB common-envelope prior art (Jeff's Staples lineage)
