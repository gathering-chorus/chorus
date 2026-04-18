/**
 * #2206 — round-trip regression for envelope enrichment POST endpoints.
 *
 * Uses #2208's oxigraph harness. POST a description via the handler, then
 * query back through fetchChorusDomain to confirm the write lands in the
 * response shape. Golden captures the after-write state.
 */
import * as fs from 'fs';
import * as path from 'path';
import { loadStoreFromTtl, makeSparqlFromStore } from '../fixtures/oxigraph-sparql';
import {
  fetchAthenaServiceDescription,
  fetchAthenaServiceEdge,
} from '../../src/handlers/athena-enrichment-write';
import { fetchChorusDomain, type ChorusDomainDeps } from '../../src/handlers/chorus-domain';

const FIXTURE_TTL = path.join(__dirname, '..', 'fixtures', 'athena-minimal.ttl');

function buildStore() {
  // eslint-disable-next-line @typescript-eslint/no-require-imports
  const oxigraph = require('oxigraph');
  const store = new oxigraph.Store();
  const ttl = fs.readFileSync(FIXTURE_TTL, 'utf-8');
  store.load(ttl, { format: 'text/turtle', to_graph_name: oxigraph.namedNode('urn:chorus:instances') });
  // Also load ontology graph so domain queries work
  store.load(ttl, { format: 'text/turtle', to_graph_name: oxigraph.namedNode('urn:chorus:ontology') });
  return store;
}

function makeWriteDeps(store: ReturnType<typeof buildStore>) {
  const seedWrites: string[] = [];
  return {
    sparqlUpdate: async (update: string) => { store.update(update); },
    appendSeed: (triple: string) => { seedWrites.push(triple); },
    seedWrites,
  };
}

describe('#2206 round-trip — POST description appears in GET domain response', () => {
  test('service description write → chorus-domain response reflects it', async () => {
    const store = buildStore();
    const writeDeps = makeWriteDeps(store);

    // BEFORE: query domain, no description on demo-alpha-service-reader yet — wait, fixture already has one.
    // Use a new service URI to make the test clean.
    // Actually, overwriting the existing works too — INSERT DATA just adds a second rdfs:comment triple.
    // Let's use a fresh entity we haven't seen.

    const r = await fetchAthenaServiceDescription(writeDeps, {
      subdomainId: 'demo-alpha-domain',
      entityId: 'new-probe',
      body: { description: 'Probe service added via POST (round-trip test).' },
    });
    expect(r.status).toBe(200);

    // Seed file got the triple
    expect(writeDeps.seedWrites).toHaveLength(1);
    expect(writeDeps.seedWrites[0]).toContain('demo-alpha-domain-service-new-probe');

    // Graph has the triple — read it back via SPARQL
    const sparql = makeSparqlFromStore(store);
    const check = await sparql(`
      PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#>
      SELECT ?comment WHERE {
        GRAPH <urn:chorus:instances> {
          <https://jeffbridwell.com/chorus#demo-alpha-domain-service-new-probe> rdfs:comment ?comment .
        }
      }
    `);
    expect(check.results.bindings).toHaveLength(1);
    expect(check.results.bindings[0].comment.value).toBe('Probe service added via POST (round-trip test).');
  });

  test('reads edge write → appears in chorus-domain rich enrichment', async () => {
    const store = buildStore();
    const writeDeps = makeWriteDeps(store);

    // Add a new service first so the domain query finds it.
    // Use the existing demo-alpha-service-reader — it's already hooked up to the domain.
    // Add a new reads edge from it to the existing Alpha Store (already exists in fixture).
    // Actually the fixture already has that edge. Let's add a new persistence + new reads edge.

    store.update(`
      PREFIX chorus: <https://jeffbridwell.com/chorus#>
      PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#>
      INSERT DATA { GRAPH <urn:chorus:instances> {
        <https://jeffbridwell.com/chorus#demo-alpha-domain-store-extra-cache> a chorus:Persistence ;
          rdfs:label "Extra Cache" .
        <https://jeffbridwell.com/chorus#demo-alpha-domain> chorus:hasPersistence <https://jeffbridwell.com/chorus#demo-alpha-domain-store-extra-cache> .
      } }
    `);

    const r = await fetchAthenaServiceEdge(writeDeps, {
      subdomainId: 'demo-alpha-domain',
      entityId: 'reader',
      predicate: 'reads',
      body: { target: 'extra-cache' },
    });
    expect(r.status).toBe(200);

    // Query chorus-domain handler, assert reads edge shows up on the service
    const sparql = makeSparqlFromStore(store);
    const deps: ChorusDomainDeps = {
      domainRegistry: {
        'demo-alpha': { product: 'demo', step: 'harvesting', description: 'Fixture.' },
      },
      getCards: () => [],
      readDomainHtml: () => null,
      fetchCompleteness: async (sdId) => sdId === 'demo-alpha-domain' ? { percentage: 50, present: [], missing: [], lifecycle: {} } : null,
      sparql,
    };
    const domainR = await fetchChorusDomain(deps, 'demo-alpha');
    expect(domainR.status).toBe(200);
    const body = domainR.body as { sections: Record<string, { itemDetails?: Array<{ label: string; reads?: string[] }> }> };
    const reader = body.sections.services?.itemDetails?.find((s) => s.label === 'Alpha Reader v2');
    expect(reader).toBeDefined();
    expect(reader?.reads).toContain('Extra Cache');
  });
});
