# Noun/Verb Peer Model + the generate → compose → product/service stack
**2026-06-23 — Jeff-driven shaping session (Kade capture). Spans Jeff (model), Silas (verb-domain/loom), Wren (owl-api), Kade (tests noun).**

## The frame Jeff drew

Nouns and verbs are **peer domains** — co-equal, neither subordinate to the other.

- **Nouns**: `tests`, `monitors`, `alerts`, … (the sets — each a domain).
- **Verbs**: `register`, `run`, `emit`, `trace`, `deploy`, … (a *bounded shared* vocabulary — its own domain).
- They **compose by binding** (`verb applies-to noun`), not by ownership.
- **Verbs have events** — `emit` is not a peer verb; each verb carries its events.

### The full stack (Jeff, verbatim intent)
1. **Generate** the atoms — *both* nouns and verbs — from the model (owl-api).
2. **Compose** them — **integrate** (wire on the MCP bus: a noun flowing through verb-transforms) **and/or orchestrate** (sequence by trigger: act / cron / NiFi) — *depending on need*.
3. **Into** products or services.

Integration is built once (the bus); orchestration is rented per problem, selected by the stream's trigger (event/calendar/continuous/data). This is the §4 grammar of `chorus-as-platform.html` with a **generation floor** added underneath.

## Empirically proven today (prove-by-running)

- `owl-api generate --class Test` → full REST surface (16 routes incl. `POST /tests` = **register**), `generate-openapi` → OpenAPI 3.1 (typed schema + error contract), `generate-page`, `generate-tests`, `generate-mcp` — all **binary output from the shape**.
- `owl-api generate-verb --verb athena-deploy` → the verb skeleton: trait (`AthenaDeployLogic`), input struct, `WIRING` (from `verbEdge`), `parse_args`, and `dispatch` with the woven `[trace] start` / `[emit] done`. **The only hand-code is `run()`'s body.**
- So: nearly the entire domain (noun API + verb scaffold) is generated; the irreducible hand-piece is `run()`'s executor (shell jest/cargo/bats) + the crawler that *recognizes* a test in source.

## The tests verb-family
`register` · `run` · `trace`, with events **on** the verbs:
- `run` → `tests.run.started` / `.passed` / `.failed` / `.skipped` / `.completed`
- `register` → `tests.registered` (`POST /tests`, already generated)
- `trace` → the correlated read over the emit stream
Uniform envelope: `{trace (card/commit), testId, pyramidLayer, hermeticity, synthetic}`. Consumers (gates, proving, alerting, dashboards) just read the stream — consumption is trivial *if the noun is modeled right*.

## The model corrections this surfaced (LOAD-BEARING — model-driven means wrongness generates everywhere)
1. **`covers → Domain`, not `SubDomain`.** The generated field list literally shows `covers|edge:SubDomain` (V1). The cross-domain join ("tests for domain X") only resolves if `covers`/`inDomain` are typed to real Domains. Every consumer joins through this.
2. **Mount all three kinds** — `definesVocabulary` `Test` **+ `TestResult` + `TestSuiteRun`** (today only `Test` is mounted, so the result/trace half has no surface).
3. **Complete the `chorus:tests` Domain** — it's a minimal mount today (missing 10 DomainShape fields: label/comment/ownedBy/purpose/atStream/atStep/status/gaps/hasDesignDoc/parent-Product).
4. **Dedupe** the legacy `chorus:tests-domain` node vs the V2 `chorus:tests`.
5. **Verb model:** `verbFamily` is a string baked into each verb instance (`verb-athena-deploy`) → that nests verb under noun (subordinate). Peer-model wants a verb *entity* + `appliesTo → noun` bindings, so a verb is defined once and composed, not re-instanced per noun.

## Live state (as of this session)
- 4,617 Tests moved `urn:chorus:instances` → `urn:chorus:domains:tests` (verified; instances=0).
- `chorus:instancesGraph "urn:chorus:domains:tests"` annotation on TestShape — live, derive resolves.
- Minimal mount (`chorus:tests a chorus:Domain ; definesVocabulary chorus:Test`) — live.
- `verb-shape.ttl` reference VerbShape loaded to `urn:chorus:ontology` (Jeff's go).
- **`/tests` HELD — not accepted.** Serves the half-mount; do not call done until the model is right (corrections above).

## Lanes
- **Verb-as-peer-domain** (the model shape, `appliesTo` bindings, the bounded verb set) → Silas (loom/verb domain) + Wren (owl-api generation).
- **Tests noun content + the run/register/trace executor logic** → Kade.
- **Model decisions** (is this the shape) → Jeff.
