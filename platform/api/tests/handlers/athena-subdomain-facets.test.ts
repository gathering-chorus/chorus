/**
 * athena-subdomain-facets — unit tests (#2187).
 *
 * Six specific functions sharing a generic body. Tests focus on:
 *   - 404 when subdomain doesn't exist (exists-query returns no bindings)
 *   - success maps bindings with all fields present
 *   - optional fields are undefined when absent in binding
 *   - label fallback to URI fragment when rdfs:label missing
 *   - SPARQL throw → 500
 *
 * One generic suite + one "shape" assertion per specific fn to lock
 * collectionKey + predicate for each route.
 */
import {
  fetchAthenaFacet,
  fetchAthenaSubdomainActors,
  fetchAthenaSubdomainScenarios,
  fetchAthenaSubdomainContract,
  fetchAthenaSubdomainIntegrations,
  fetchAthenaSubdomainPersistence,
  fetchAthenaSubdomainPriorArt,
  type AthenaFacetDeps,
  type SparqlFacetBinding,
  type FacetSpec,
} from '../../src/handlers/athena-subdomain-facets';

function existsOk() {
  return { results: { bindings: [{ s: { value: 'x' } }] } };
}
function existsEmpty() {
  return { results: { bindings: [] } };
}
function dataResult(bindings: SparqlFacetBinding[]) {
  return { results: { bindings } };
}

function deps(over: Partial<AthenaFacetDeps> = {}): AthenaFacetDeps {
  return {
    sparql: async () => existsOk(),
    now: () => 1_000_000,
    ...over,
  };
}

const actorSpec: FacetSpec = {
  queryName: 'test-actors',
  collectionKey: 'actors',
  itemVar: 'actor',
  predicate: 'hasActor',
  fields: [
    { sparqlVar: 'role', outputKey: 'role', rdfProp: 'chorus:actorRole' },
    { sparqlVar: 'action', outputKey: 'action', rdfProp: 'chorus:actorAction' },
  ],
};

describe('fetchAthenaFacet — generic (#2187)', () => {
  test('returns 404 when exists-query yields zero bindings', async () => {
    const r = await fetchAthenaFacet(deps({
      sparql: async () => existsEmpty(),
    }), 'missing', actorSpec);
    expect(r.status).toBe(404);
    const body = r.body as { data: { error: string }; _meta: { error: boolean } };
    expect(body.data.error).toContain('missing');
    expect(body._meta.error).toBe(true);
  });

  test('exists-query runs first, then main query when subdomain exists', async () => {
    const queries: string[] = [];
    await fetchAthenaFacet(deps({
      sparql: async (q) => {
        queries.push(q);
        return queries.length === 1 ? existsOk() : dataResult([]);
      },
    }), 'x', actorSpec);
    expect(queries).toHaveLength(2);
    expect(queries[0]).toContain('LIMIT 1');
    expect(queries[1]).toContain('chorus:hasActor');
  });

  test('maps bindings with all optional fields present', async () => {
    let step = 0;
    const r = await fetchAthenaFacet(deps({
      sparql: async () => {
        step++;
        if (step === 1) return existsOk();
        return dataResult([{
          actor: { value: 'https://jeffbridwell.com/chorus#silas' },
          label: { value: 'Silas' },
          role: { value: 'architect' },
          action: { value: 'reviews' },
        }]);
      },
    }), 'x', actorSpec);
    const body = r.body as { data: { actors: Array<{ uri: string; label: string; role: string; action: string }> } };
    expect(body.data.actors[0]).toEqual({
      uri: 'https://jeffbridwell.com/chorus#silas',
      label: 'Silas',
      role: 'architect',
      action: 'reviews',
    });
  });

  test('optional fields are null when absent in binding (JSON-visible)', async () => {
    let step = 0;
    const r = await fetchAthenaFacet(deps({
      sparql: async () => {
        step++;
        if (step === 1) return existsOk();
        return dataResult([{ actor: { value: 'https://jeffbridwell.com/chorus#x' } }]);
      },
    }), 'x', actorSpec);
    const body = r.body as { data: { actors: Array<{ label: string; role: string | null; action: string | null }> } };
    expect(body.data.actors[0].label).toBe('x');
    expect(body.data.actors[0].role).toBeNull();
    expect(body.data.actors[0].action).toBeNull();
  });

  test('SPARQL throws on exists-query returns 500', async () => {
    const r = await fetchAthenaFacet(deps({
      sparql: async () => { throw new Error('fuseki down'); },
    }), 'x', actorSpec);
    expect(r.status).toBe(500);
  });

  test('SPARQL throws on main query returns 500', async () => {
    let step = 0;
    const r = await fetchAthenaFacet(deps({
      sparql: async () => {
        step++;
        if (step === 1) return existsOk();
        throw new Error('query broke');
      },
    }), 'x', actorSpec);
    expect(r.status).toBe(500);
  });

  test('count in meta equals items length', async () => {
    let step = 0;
    const r = await fetchAthenaFacet(deps({
      sparql: async () => {
        step++;
        if (step === 1) return existsOk();
        return dataResult([
          { actor: { value: '#a' }, label: { value: 'A' } },
          { actor: { value: '#b' }, label: { value: 'B' } },
        ]);
      },
    }), 'x', actorSpec);
    const body = r.body as { _meta: { count: number } };
    expect(body._meta.count).toBe(2);
  });
});

// Shape locks — each specific fn emits the right envelope and query predicates
describe('specific facet functions have correct envelope+query shape (#2187)', () => {
  test.each([
    [fetchAthenaSubdomainActors, 'subdomain-actors', 'actors', 'hasActor'],
    [fetchAthenaSubdomainScenarios, 'subdomain-scenarios', 'scenarios', 'hasScenario'],
    [fetchAthenaSubdomainContract, 'subdomain-contract', 'endpoints', 'hasContract'],
    [fetchAthenaSubdomainIntegrations, 'subdomain-integrations', 'integrations', 'hasIntegration'],
    [fetchAthenaSubdomainPersistence, 'subdomain-persistence', 'stores', 'hasPersistence'],
    [fetchAthenaSubdomainPriorArt, 'subdomain-prior-art', 'items', 'hasPriorArt'],
  ])('%p emits envelope=%s, collectionKey=%s, query uses %s', async (fn, envName, collectionKey, predicate) => {
    const queries: string[] = [];
    const r = await (fn as (d: AthenaFacetDeps, id: string) => Promise<{ status: number; body: unknown }>)({
      sparql: async (q) => {
        queries.push(q);
        if (queries.length === 1) return existsOk();
        return dataResult([]);
      },
    }, 'x');
    expect(r.status).toBe(200);
    const body = r.body as { _meta: { query_name: string }; data: Record<string, unknown> };
    expect(body._meta.query_name).toBe(envName);
    expect(body.data[collectionKey as keyof typeof body.data]).toBeDefined();
    expect(queries[1]).toContain(`chorus:${predicate}`);
  });
});
