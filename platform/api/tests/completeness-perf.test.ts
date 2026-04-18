/**
 * Completeness query performance + shape tests — #1979
 *
 * The monolithic 11-OPTIONAL cross-graph query causes Fuseki timeout
 * on populated domains. Fix: split into parallel per-section queries.
 * These tests verify the response shape is unchanged and performance
 * stays under budget.
 */

import { startTestApp, type TestApp } from './lib/test-app';

const fs = require('fs');
const path = require('path');

// Structural test — no integration needed
describe('#1979: Completeness query structure', () => {

  let harness: TestApp;

  beforeAll(async () => { harness = await startTestApp(); });
  afterAll(async () => { if (harness) await harness.close(); });
  test('no single SPARQL query has more than 2 OPTIONAL cross-graph joins', () => {
    const serverSrc = fs.readFileSync(
      path.join(__dirname, '..', 'src', 'server.ts'), 'utf-8'
    );
    // Find the completeness endpoint block
    const endpointStart = serverSrc.indexOf("'/api/athena/subdomains/:id/completeness'");
    const endpointBlock = serverSrc.slice(endpointStart, endpointStart + 5000);

    // Count OPTIONAL clauses that cross graphs (OPTIONAL { GRAPH <...> { ... } })
    // in any single query template literal
    const queryBlocks = endpointBlock.match(/`[^`]*OPTIONAL[^`]*`/gs) || [];
    for (const q of queryBlocks) {
      const crossGraphOptionals = (q.match(/OPTIONAL\s*\{\s*GRAPH\s*</g) || []).length;
      expect(crossGraphOptionals).toBeLessThanOrEqual(2);
    }
  });
});

describe('#1979: Completeness response shape', () => {

  let harness: TestApp;

  beforeAll(async () => { harness = await startTestApp(); });
  afterAll(async () => { if (harness) await harness.close(); });
  test('completeness returns all expected fields', async () => {
    const res = await fetch(`${harness.baseUrl}/api/athena/subdomains/alerts-monitors-domain/completeness`);
    expect(res.status).toBe(200);
    const body = await res.json();
    const data = body.data;

    expect(data).toHaveProperty('subdomain');
    expect(data).toHaveProperty('sections');
    expect(data).toHaveProperty('present');
    expect(data).toHaveProperty('missing');
    expect(data).toHaveProperty('percentage');
    expect(data).toHaveProperty('lifecycle');

    // All 16 section keys present
    const sectionKeys = Object.keys(data.sections);
    expect(sectionKeys).toEqual(expect.arrayContaining([
      'label', 'comment', 'owner', 'step', 'actors', 'scenarios',
      'contract', 'prior_art', 'pages', 'integrations', 'services',
      'persistence', 'pipeline', 'logs', 'gaps', 'edges',
    ]));
    expect(sectionKeys).toHaveLength(16);
  }, 15_000);

  test('lifecycle gates have correct required fields', async () => {
    const res = await fetch(`${harness.baseUrl}/api/athena/subdomains/alerts-monitors-domain/completeness`);
    const body = await res.json();
    const lc = body.data.lifecycle;

    expect(lc.create.required).toEqual(['label', 'owner', 'step', 'comment']);
    expect(lc.wip.required).toEqual(['actors', 'edges']);
    expect(lc.done.required).toEqual(['scenarios', 'contract']);
  }, 15_000);

  test('completeness responds under 100ms', async () => {
    const res = await fetch(`${harness.baseUrl}/api/athena/subdomains/alerts-monitors-domain/completeness`);
    const body = await res.json();
    expect(body._meta.duration_ms).toBeLessThan(100);
  }, 15_000);

  test('404 for nonexistent subdomain', async () => {
    const res = await fetch(`${harness.baseUrl}/api/athena/subdomains/nonexistent-xyz/completeness`);
    expect(res.status).toBe(404);
  }, 15_000);
});
