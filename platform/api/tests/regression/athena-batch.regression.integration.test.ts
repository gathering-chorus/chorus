/**
 * #2208 — Data-driven regression for additional Athena GET handlers.
 *
 * Batches products, subproducts, owners, blast-radius through the same
 * oxigraph + TTL + golden JSON pattern. Adds blast-radius consume edge
 * to the fixture so the traversal has something to find.
 */
import * as fs from 'fs';
import * as path from 'path';
import { fetchAthenaProducts } from '../../src/handlers/athena-products';
import { fetchAthenaSubproducts } from '../../src/handlers/athena-subproducts';
import { fetchAthenaOwners } from '../../src/handlers/athena-owners';
import { fetchAthenaBlastRadius } from '../../src/handlers/athena-blast-radius';
import { fetchAthenaSteps } from '../../src/handlers/athena-steps';
import { fetchAthenaMachines } from '../../src/handlers/athena-machines';
import { fetchAthenaHealth } from '../../src/handlers/athena-health';
import { makeSparqlFromTtl } from '../fixtures/oxigraph-sparql';

const FIXTURE_TTL = path.join(__dirname, '..', 'fixtures', 'athena-minimal.ttl');

function loadQuery(name: string): string {
  return fs.readFileSync(path.join(__dirname, '..', '..', 'src', 'sparql', `${name}.sparql`), 'utf-8').trim();
}

function envelope(queryName: string, data: unknown, _durationMs: number, extra: Record<string, unknown> = {}) {
  return {
    _meta: { source: 'athena', query_name: queryName, graph: 'urn:chorus:ontology', ...extra },
    data,
  };
}

function stripVolatile(body: unknown): unknown {
  const b = body as { _meta: Record<string, unknown>; data: unknown };
  return {
    _meta: Object.fromEntries(Object.entries(b._meta).filter(([k]) => k !== 'duration_ms' && k !== 'timestamp')),
    data: b.data,
  };
}

async function assertGolden(name: string, actual: unknown): Promise<void> {
  const goldenPath = path.join(__dirname, 'golden', `${name}.json`);
  if (process.env.UPDATE_GOLDEN === 'true') {
    fs.mkdirSync(path.dirname(goldenPath), { recursive: true });
    fs.writeFileSync(goldenPath, JSON.stringify(actual, null, 2) + '\n');
    return;
  }
  const golden = JSON.parse(fs.readFileSync(goldenPath, 'utf-8'));
  expect(actual).toEqual(golden);
}

describe('#2208 data regression — athena batch', () => {
  const sparql = makeSparqlFromTtl(FIXTURE_TTL, 'urn:chorus:ontology');
  const deps = { sparql, loadQuery, envelope };

  test('/api/athena/products', async () => {
    const r = await fetchAthenaProducts(deps);
    expect(r.status).toBe(200);
    await assertGolden('athena-products', stripVolatile(r.body));
  });

  test('/api/athena/subproducts', async () => {
    const r = await fetchAthenaSubproducts(deps);
    expect(r.status).toBe(200);
    await assertGolden('athena-subproducts', stripVolatile(r.body));
  });

  test('/api/athena/owners', async () => {
    const r = await fetchAthenaOwners(deps);
    expect(r.status).toBe(200);
    await assertGolden('athena-owners', stripVolatile(r.body));
  });

  test('/api/athena/subdomains/:id/blast-radius', async () => {
    const r = await fetchAthenaBlastRadius(deps, 'demo-alpha-domain');
    expect(r.status).toBe(200);
    await assertGolden('athena-blast-radius', stripVolatile(r.body));
  });

  test('/api/athena/steps', async () => {
    const r = await fetchAthenaSteps(deps);
    expect(r.status).toBe(200);
    await assertGolden('athena-steps', stripVolatile(r.body));
  });

  test('/api/athena/machines', async () => {
    const r = await fetchAthenaMachines(deps);
    expect(r.status).toBe(200);
    await assertGolden('athena-machines', stripVolatile(r.body));
  });

  test('/api/athena/health', async () => {
    const r = await fetchAthenaHealth(deps);
    expect(r.status).toBe(200);
    await assertGolden('athena-health', stripVolatile(r.body));
  });
});
