/**
 * #2208 — Data-driven regression for /api/athena/subdomains.
 *
 * Real SPARQL engine (oxigraph) over a checked-in TTL fixture. Asserts
 * response shape against a golden file. No faked bindings.
 *
 * If the ontology shape shifts (predicate renamed, graph moved, subdomain
 * class changed), this test fails and the diff is human-readable.
 */
import * as fs from 'fs';
import * as path from 'path';
import { fetchAthenaSubdomains } from '../../src/handlers/athena-subdomains';
import { makeSparqlFromTtl } from '../fixtures/oxigraph-sparql';

const FIXTURE_TTL = path.join(__dirname, '..', 'fixtures', 'athena-minimal.ttl');
const GOLDEN_PATH = path.join(__dirname, 'golden', 'athena-subdomains.json');

function loadQuery(name: string): string {
  return fs.readFileSync(path.join(__dirname, '..', '..', 'src', 'sparql', `${name}.sparql`), 'utf-8').trim();
}

function envelope(queryName: string, data: unknown, _durationMs: number, extra: Record<string, unknown> = {}) {
  return {
    _meta: { source: 'athena', query_name: queryName, graph: 'urn:chorus:ontology', ...extra },
    data,
  };
}

describe('#2208 data regression — /api/athena/subdomains', () => {
  test('fixture TTL + real SPARQL → response matches golden JSON', async () => {
    const sparql = makeSparqlFromTtl(FIXTURE_TTL, 'urn:chorus:ontology');
    const r = await fetchAthenaSubdomains(
      { sparql, loadQuery, envelope },
      {},
    );

    expect(r.status).toBe(200);

    // Strip duration_ms from _meta — it's non-deterministic
    const body = r.body as { _meta: Record<string, unknown>; data: unknown };
    const stripped = {
      _meta: Object.fromEntries(Object.entries(body._meta).filter(([k]) => k !== 'duration_ms' && k !== 'timestamp')),
      data: body.data,
    };

    if (process.env.UPDATE_GOLDEN === 'true') {
      fs.mkdirSync(path.dirname(GOLDEN_PATH), { recursive: true });
      fs.writeFileSync(GOLDEN_PATH, JSON.stringify(stripped, null, 2) + '\n');
      return;
    }

    const golden = JSON.parse(fs.readFileSync(GOLDEN_PATH, 'utf-8'));
    expect(stripped).toEqual(golden);
  });
});
