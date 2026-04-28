/**
 * #2208 — Regression coverage for the remaining 11 athena handlers.
 *
 * Most are pure-sparql (code, coverage, pages, endpoints, completeness,
 * actors/scenarios/contract/integrations/persistence/prior-art via facets)
 * and plug directly into the oxigraph harness. Cards + alerts need injected
 * data/file deps; fakes provided inline.
 *
 * Most golden bodies are empty-result shapes because the fixture only seeds
 * one service + one persistence + one reads edge. Shape itself regresses —
 * if the envelope or empty-data structure drifts, these fail.
 */
import * as fs from 'fs';
import * as path from 'path';
import { fetchAthenaSubdomainCards } from '../../src/handlers/athena-subdomain-cards';
import { fetchAthenaSubdomainAlerts } from '../../src/handlers/athena-subdomain-alerts';
import { fetchAthenaSubdomainCode } from '../../src/handlers/athena-subdomain-code';
import { fetchAthenaSubdomainCoverage, fetchAthenaSubdomainTestCoverage } from '../../src/handlers/athena-subdomain-coverage';
import { fetchAthenaSubdomainPages } from '../../src/handlers/athena-subdomain-pages';
import { fetchAthenaSubdomainEndpoints } from '../../src/handlers/athena-subdomain-endpoints';
import { fetchAthenaSubdomainCompleteness } from '../../src/handlers/athena-subdomain-completeness';
import {
  fetchAthenaSubdomainActors,
  fetchAthenaSubdomainScenarios,
  fetchAthenaSubdomainContract,
  fetchAthenaSubdomainIntegrations,
  fetchAthenaSubdomainPersistence,
  fetchAthenaSubdomainPriorArt,
} from '../../src/handlers/athena-subdomain-facets';
import { makeSparqlFromTtl } from '../fixtures/oxigraph-sparql';

const FIXTURE_TTL = path.join(__dirname, '..', 'fixtures', 'athena-minimal.ttl');
const SD_ID = 'demo-alpha-domain';

function loadQuery(name: string): string {
  return fs.readFileSync(path.join(__dirname, '..', '..', 'src', 'sparql', `${name}.sparql`), 'utf-8').trim();
}

function envelope(queryName: string, data: unknown, _d: number, extra: Record<string, unknown> = {}) {
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

describe('#2208 regression — subdomain handler batch (11 handlers)', () => {
  const sparql = makeSparqlFromTtl(FIXTURE_TTL, 'urn:chorus:ontology');
  const deps = { sparql, loadQuery, envelope, now: () => 1_000_000 };

  // Pure-sparql handlers
  test('/api/athena/subdomains/:id/code', async () => {
    const r = await fetchAthenaSubdomainCode(deps, SD_ID);
    expect(r.status).toBe(200);
    await assertGolden('athena-subdomain-code', stripVolatile(r.body));
  });

  test('/api/athena/subdomains/:id/coverage', async () => {
    const r = await fetchAthenaSubdomainCoverage(deps, SD_ID);
    expect(r.status).toBe(200);
    await assertGolden('athena-subdomain-coverage', stripVolatile(r.body));
  });

  test('/api/athena/subdomains/:id/test-coverage', async () => {
    const r = await fetchAthenaSubdomainTestCoverage(deps, SD_ID);
    expect(r.status).toBe(200);
    await assertGolden('athena-subdomain-test-coverage', stripVolatile(r.body));
  });

  test('/api/athena/subdomains/:id/pages', async () => {
    const r = await fetchAthenaSubdomainPages(deps, SD_ID);
    expect(r.status).toBe(200);
    await assertGolden('athena-subdomain-pages', stripVolatile(r.body));
  });

  test('/api/athena/subdomains/:id/endpoints', async () => {
    const r = await fetchAthenaSubdomainEndpoints(deps, SD_ID);
    expect(r.status).toBe(200);
    await assertGolden('athena-subdomain-endpoints', stripVolatile(r.body));
  });

  test('/api/athena/subdomains/:id/completeness', async () => {
    const r = await fetchAthenaSubdomainCompleteness(
      { sparqlQuery: sparql, envelope, now: () => 1_000_000 },
      SD_ID,
    );
    expect(r.status).toBe(200);
    await assertGolden('athena-subdomain-completeness', stripVolatile(r.body));
  });

  // Facet family (6 sub-endpoints, one handler)
  test('/api/athena/subdomains/:id/actors', async () => {
    const r = await fetchAthenaSubdomainActors(deps, SD_ID);
    expect(r.status).toBe(200);
    await assertGolden('athena-subdomain-actors', stripVolatile(r.body));
  });

  test('/api/athena/subdomains/:id/scenarios', async () => {
    const r = await fetchAthenaSubdomainScenarios(deps, SD_ID);
    expect(r.status).toBe(200);
    await assertGolden('athena-subdomain-scenarios', stripVolatile(r.body));
  });

  test('/api/athena/subdomains/:id/contract', async () => {
    const r = await fetchAthenaSubdomainContract(deps, SD_ID);
    expect(r.status).toBe(200);
    await assertGolden('athena-subdomain-contract', stripVolatile(r.body));
  });

  test('/api/athena/subdomains/:id/integrations', async () => {
    const r = await fetchAthenaSubdomainIntegrations(deps, SD_ID);
    expect(r.status).toBe(200);
    await assertGolden('athena-subdomain-integrations', stripVolatile(r.body));
  });

  test('/api/athena/subdomains/:id/persistence', async () => {
    const r = await fetchAthenaSubdomainPersistence(deps, SD_ID);
    expect(r.status).toBe(200);
    await assertGolden('athena-subdomain-persistence', stripVolatile(r.body));
  });

  test('/api/athena/subdomains/:id/prior-art', async () => {
    const r = await fetchAthenaSubdomainPriorArt(deps, SD_ID);
    expect(r.status).toBe(200);
    await assertGolden('athena-subdomain-prior-art', stripVolatile(r.body));
  });

  // Non-sparql deps
  test('/api/athena/subdomains/:id/cards', async () => {
    const r = await fetchAthenaSubdomainCards(
      { getBoardCards: () => [], envelope, now: () => 1_000_000 },
      SD_ID,
    );
    expect(r.status).toBe(200);
    await assertGolden('athena-subdomain-cards', stripVolatile(r.body));
  });

  test('/api/athena/subdomains/:id/alerts', async () => {
    const r = await fetchAthenaSubdomainAlerts(
      {
        listAlertFiles: () => [],
        readAlertFile: () => '',
        now: () => 1_000_000,
        envelope,
      },
      SD_ID,
    );
    expect(r.status).toBe(200);
    await assertGolden('athena-subdomain-alerts', stripVolatile(r.body));
  });
});
