/**
 * RCA domain tests — #1795
 *
 * Integration tests — hit live Chorus API at localhost:3340.
 * Requires RUN_INTEGRATION=true, Chorus API running, Fuseki on 3030.
 */

import { startTestApp, type TestApp } from './lib/test-app';


describe('POST /api/chorus/rca', () => {

  let harness: TestApp;

  beforeAll(async () => { harness = await startTestApp(); });
  afterAll(async () => { if (harness) await harness.close(); });
  test('creates RCA entry with required fields', async () => {
    const res = await fetch(`${harness.baseUrl}/api/chorus/rca`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        title: 'Test RCA — integration test',
        trigger: 'Test trigger event',
        timeline: '10:00 test started, 10:01 test failed',
        root_cause: 'Missing integration test coverage',
        contributing_factors: ['Time pressure', 'No CI gate'],
        corrective_actions: ['Add test', 'Add gate'],
        cards: [9999],
        spine_events: ['test.event.123'],
      }),
    });
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body.ok).toBe(true);
    expect(typeof body.id).toBe('number');
    expect(body.status).toBe('open');
  });

  test('rejects RCA missing required fields', async () => {
    const res = await fetch(`${harness.baseUrl}/api/chorus/rca`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ title: 'Incomplete RCA' }),
    });
    expect(res.status).toBe(400);
    const body = await res.json();
    expect(body.error).toBeDefined();
  });

  test('links RCA to card IDs and spine events', async () => {
    const createRes = await fetch(`${harness.baseUrl}/api/chorus/rca`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        title: 'Linked RCA test',
        trigger: 'Card #1665 demo took 5 rounds',
        timeline: 'Round 1-5 with Jeff as live tester',
        root_cause: 'No test suite — Jeff was the test suite',
        contributing_factors: ['Missing TDD discipline'],
        corrective_actions: ['Write tests first'],
        cards: [1665, 1674],
        spine_events: ['demo.failed.1665'],
      }),
    });
    expect(createRes.status).toBe(200);
    const created = await createRes.json();

    // Verify the links are queryable via GET
    const listRes = await fetch(`${harness.baseUrl}/api/chorus/rcas`);
    const list = await listRes.json();
    const rca = list.results.find(r => r.id === created.id);
    expect(rca).toBeDefined();
    expect(rca.cards).toContain(1665);
    expect(rca.cards).toContain(1674);
    expect(rca.spine_events).toContain('demo.failed.1665');
  });
});

describe('GET /api/chorus/rcas', () => {

  let harness: TestApp;

  beforeAll(async () => { harness = await startTestApp(); });
  afterAll(async () => { if (harness) await harness.close(); });
  test('returns list with status field', async () => {
    const res = await fetch(`${harness.baseUrl}/api/chorus/rcas`);
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(Array.isArray(body.results)).toBe(true);
    for (const rca of body.results) {
      expect(rca.id).toBeDefined();
      expect(rca.title).toBeDefined();
      expect(rca.status).toBeDefined();
      expect(['open', 'verified', 'closed']).toContain(rca.status);
      expect(rca.trigger).toBeDefined();
      expect(rca.root_cause).toBeDefined();
    }
  });

  test('filters by status query param', async () => {
    const res = await fetch(`${harness.baseUrl}/api/chorus/rcas?status=open`);
    expect(res.status).toBe(200);
    const body = await res.json();
    for (const rca of body.results) {
      expect(rca.status).toBe('open');
    }
  });

  test('returns total count', async () => {
    const res = await fetch(`${harness.baseUrl}/api/chorus/rcas`);
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(typeof body.total).toBe('number');
    expect(body.total).toBe(body.results.length);
  });
});

describe('RCA sub-domain in Athena', () => {

  let harness: TestApp;

  beforeAll(async () => { harness = await startTestApp(); });
  afterAll(async () => { if (harness) await harness.close(); });
  test('loom-rcas sub-domain exists with actors and scenarios', async () => {
    const res = await fetch(`${harness.baseUrl}/api/athena/subdomains/loom-rcas/completeness`);
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body.data.label).toMatch(/rca/i);
    // Should have actors populated
    expect(body.data.sections.actors).toBe(true);
  });
});

describe('Real RCA entries populated', () => {

  let harness: TestApp;

  beforeAll(async () => { harness = await startTestApp(); });
  afterAll(async () => { if (harness) await harness.close(); });
  test('at least 2 RCAs exist from real incidents', async () => {
    const res = await fetch(`${harness.baseUrl}/api/chorus/rcas`);
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body.total).toBeGreaterThanOrEqual(2);

    // Verify one links to #1665 (5-round demo)
    const demoRca = body.results.find(r => r.cards?.includes(1665));
    expect(demoRca).toBeDefined();
    expect(demoRca.title).toBeTruthy();
  });
});
