/**
 * #2208 — Data-driven regression for /api/chorus/domain/:name (hot path for
 * envelope enrichment — #2178). Exercises the SPARQL-section fallback path
 * with real data so shape drift in per-predicate queries is caught.
 */
import * as fs from 'fs';
import * as path from 'path';
import { fetchChorusDomain, type ChorusDomainDeps } from '../../src/handlers/chorus-domain';
import { makeSparqlFromTtl } from '../fixtures/oxigraph-sparql';

const FIXTURE_TTL = path.join(__dirname, '..', 'fixtures', 'athena-minimal.ttl');
const GOLDEN_PATH = path.join(__dirname, 'golden', 'chorus-domain-alpha.json');

const domainRegistry = {
  alpha: { product: 'demo', step: 'harvesting', description: 'Fixture domain.' },
  beta: { product: 'demo', step: 'building', description: 'Second fixture domain.' },
};

describe('#2208 data regression — /api/chorus/domain/:name', () => {
  test('alpha → response matches golden (SPARQL-section fallback path)', async () => {
    const sparql = makeSparqlFromTtl(FIXTURE_TTL, 'urn:chorus:ontology');

    const deps: ChorusDomainDeps = {
      domainRegistry,
      getCards: () => [],
      readDomainHtml: () => null,
      fetchCompleteness: async (sdId) => sdId === 'alpha-domain' ? { percentage: 50, present: [], missing: [], lifecycle: {} } : null,
      // chorus-domain expects sparql results to reach instances graph; our fixture loads into ontology graph.
      // Wrap the sparql to query both graphs by stripping GRAPH <urn:chorus:instances> → <urn:chorus:ontology>
      // so the same fixture exercises the query shapes.
      sparql: async (query) => sparql(query.replace(/urn:chorus:instances/g, 'urn:chorus:ontology')),
    };

    const r = await fetchChorusDomain(deps, 'alpha');
    expect(r.status).toBe(200);

    const body = r.body as Record<string, unknown>;

    if (process.env.UPDATE_GOLDEN === 'true') {
      fs.mkdirSync(path.dirname(GOLDEN_PATH), { recursive: true });
      fs.writeFileSync(GOLDEN_PATH, JSON.stringify(body, null, 2) + '\n');
      return;
    }

    const golden = JSON.parse(fs.readFileSync(GOLDEN_PATH, 'utf-8'));
    expect(body).toEqual(golden);
  });

  test('unknown domain → 404 unchanged', async () => {
    const sparql = makeSparqlFromTtl(FIXTURE_TTL, 'urn:chorus:ontology');
    const deps: ChorusDomainDeps = {
      domainRegistry,
      getCards: () => [],
      readDomainHtml: () => null,
      fetchCompleteness: async () => null,
      sparql,
    };
    const r = await fetchChorusDomain(deps, 'zeta');
    expect(r.status).toBe(404);
    const body = r.body as { error: string; validDomains: string[] };
    expect(body.error).toContain('zeta');
    expect(body.validDomains).toEqual(['alpha', 'beta']);
  });
});
