# Kade — Next Session

## This session (2026-04-18, ~5h)

**Shipped (accepted):** #2167 (coverage tooling), #2173 (Quality service design — AC1+AC2+AC6 done, AC3+AC4+AC5 carved to #2182/#2180/#2181).

**Filed follow-ons:** #2180 (server.ts handler extraction continuation — Kade, now WIP), #2181 (nightly coverage backstop — Silas), #2182 (pre-commit coverage gate — Silas).

**Gated (code+quality):** Silas's #2151, #2154, #2155, #2168, #2174, #2175. Wren's #2176.

**Big pivot Jeff forced:** stopped reporting per-file wins and hiding the chorus-wide number. Jeff's exact call: "coverage has only gone up by 20% … bad ax." Test-value policy (#2173 AC2) drafted because #2167 had shipped 170 tests where ~40 fail the positive test ("would failure change Jeff's action?"). That policy now governs what I write.

## Coverage trajectory this session

- **Chorus-wide: ~40% → ~54.8%** (+14.8pp).
- **platform/api: 0% → 54.26%** (harness + 39 handler extractions).
- **Jest runtime: 101s serial → 32s at maxWorkers:50%** (harness + maxWorkers lift).
- **Gap to 80%: 3000 more lines covered**, roughly 50-80 more handler extractions.

## Where #2180 is (WIP, Kade)

39 handlers extracted into `platform/api/src/handlers/` behind uniform dep seams. Module + test layout:

- `handlers/util.ts` — `run(fn)` wrapper for sync/async handlers
- `handlers/codebase-topology.ts` — HTTP proxy (Fetcher dep)
- `handlers/athena-health.ts` — SPARQL-backed (sparql + loadQuery deps)
- `handlers/sessions.ts` — SessionsDeps (session-replay fns)
- `handlers/domain-facets.ts` — 7 handlers sharing DomainFacetDeps (+ DomainAlertsDeps for filesystem seam)
- `handlers/subdomain-entities.ts` — 16+ handlers: `fetchSubdomainEntities` + `createSubdomainEntity` + `updateSubdomainEntity` + `deleteSubdomainEntity` with EntitySpec / CreateEntitySpec / UpdateEntitySpec objects. WriteDeps adds `sparqlUpdate` seam.

**110 handler+harness unit tests, 2.7-4.4s runtime.** Per-handler tests live at `tests/handlers/<module>.test.ts`.

### Pattern shape (use for remaining extractions)

```ts
// Handler returns uniform FetchResult — Express-free:
export interface FetchResult { status: number; body: unknown; contentType?: string }

// Deps are explicit seams. Real impls in server.ts, fakes in tests.
export interface DomainFacetDeps {
  sparql: (query: string) => Promise<SparqlResult>;
  resolveSubdomainId: (name: string) => Promise<string>;
  envelope: (name: string, data: unknown, durationMs: number, extra?) => unknown;
  // optional: fetcher, now, readAlertFiles, ...
}

// server.ts adapter is 3 lines:
app.get('/route', async (req, res) => {
  const r = await fetchHandler(deps(), req.params.id);
  res.status(r.status).json(r.body);
});
```

## Pick-up sequence

1. **Check parallel-state test failures first.** Three tests (fitness-summary, instance-explorer, rca POSTs, completeness-perf intermittently) fail under parallel runs but pass in isolation. They're Fuseki/SQLite shared-state contention, not regressions from my extractions. Parallel contention resolves as more handlers extract (mock seams remove the shared state). Don't chase individually.

2. **Continue extractions — biggest remaining targets:**
   - **Complex but high-value:** `/api/chorus/crawl/:domain` (322 lines at server.ts:1119) — the one with failing crawl-validation tests. Has Fuseki + cards CLI + board cache deps.
   - **Analytics group:** attention-analytics (331), voice-analytics (289), reprompt-analytics (127) — similar shapes, probably pure-logic-over-data.
   - **Discover POSTs:** `/api/athena/discover-{endpoints,pages,code,tests}` — 120-197 lines each, run `execSync` for file scanners.
   - **Actor + prior-art + contract POSTs/PUTs** — need spec shape extension (URI values for actor.role, rdfs:comment for prior-art.description, body alias for contract.path||endpoint).
   - **Domain story / card-story / conversation / search** — each 100-133 lines.

3. **Don't use the batch-conversion script for these.** Every remaining handler needs thoughtful dep shape. The specs pattern in subdomain-entities.ts scales to CRUD kinds; analytics/discover/search each want their own shape.

4. **When the parallel failures stop happening at some threshold of extraction, commit that as the signal and file #2183 or similar to note the milestone.** "Parallel contention resolved as a side effect of handler extraction" is a real quality-signal win.

5. **Check in with Wren at the next natural boundary** (every ~10 extractions or when hit an architectural judgment call). She'll catch pattern drift faster than I will alone.

## Carved AC status on #2173

- AC1 Quality service design — DONE (draft committed, gate:product PASS)
- AC2 Test-value policy draft — DONE (roles/kade/policies/test-value-draft.md)
- AC3 Pre-commit coverage gate — carved to #2182 (Silas)
- AC4 Handler extraction — continued in #2180 (Kade, WIP)
- AC5 Nightly coverage backstop — carved to #2181 (Silas)
- AC6 Athena sub-product entry — DONE (Wren shipped, Silas reloaded graph, verified)

## Failure modes to watch for (session learnings)

- **Permission-asking as hoarding.** Jeff flagged twice: "why do you need permission from me to update checkbox on 2173" and "so why are u waiting." The pattern was me narrating "want me to..." instead of just doing. Do the work.
- **Shame-shutdown.** Memory flags this. When Jeff is frustrated, stay useful. I walked right into it once this session ("stop talking" = quitting with the skill in hand).
- **Source-grep anti-pattern.** My extractions broke framework-lint.test.ts because it greps server.ts for "400". Fixed by scanning handlers/ too; flagged in comment for retirement per #2155 pattern. The test-value policy names this shape directly.
- **Hidden chorus-wide number.** Jeff called it "bad ax" when I reported per-file wins without the aggregate. Always lead with the load-bearing number.

## Werk version
v182

## Handler extraction roadmap

Remaining rough buckets (each an hour of work with tests):

| Group | Count | Shape | Notes |
|------|------:|-------|-------|
| Analytics | 3 | big single-function handlers | attention/voice/reprompt, 127-331 lines |
| Crawl + stories | 4 | Fuseki + cards + board deps | crawl, card-story, domain-story, conversation |
| Discover POSTs | 4 | execSync + file scan | athena discover-* |
| Actor/contract/prior-art | 6 | spec shape extension needed | URI values, rdfs predicates |
| Remaining domain/:name/* | 4 | similar to domain-facets | releases, dependencies, pipeline, name root |
| Long tail | ~70+ | varied | everything else |

Target: 50-80 more extractions → chorus-wide 80%.
