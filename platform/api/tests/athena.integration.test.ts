// @test-type: integration:api — reads the live chorus-api door (RUN_INTEGRATION=true); no writes (#4142)
import { withServiceAuth } from './lib/service-token';
// #3619 — secured surfaces are envelope-gated; this suite is a real consumer
// and carries a scoped token. #4142 — the suite READS the live door only:
// every create/update/delete moved to hermetic worlds (tests/handlers/*).
withServiceAuth();

const INTEGRATION_ENABLED = process.env.RUN_INTEGRATION === 'true';
const API = process.env.CHORUS_API || 'http://localhost:3340';

let apiUp = false;

beforeAll(async () => {
  if (!INTEGRATION_ENABLED) return;
  try {
    const res = await fetch(`${API}/api/athena/health`);
    apiUp = res.ok;
  } catch {
    apiUp = false;
  }
});

const describeIntegration = INTEGRATION_ENABLED ? describe : describe.skip;

describeIntegration('GET /api/athena/health', () => {
  test('returns 200 with status ok and triple count', async () => {
    const res = await fetch(`${API}/api/athena/health`);
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body._meta.source).toBe('athena');
    expect(body.data.status).toBe('ok');
    expect(typeof body.data.tripleCount).toBe('number');
    expect(body.data.tripleCount).toBeGreaterThan(0);
  });
});

describeIntegration('retired product endpoints (#3603)', () => {
  test('GET /api/athena/products is gone — athena-make :3360/products is the product API', async () => {
    const res = await fetch(`${API}/api/athena/products`);
    expect(res.status).toBe(404);
  });
  test('GET /api/athena/subproducts is gone', async () => {
    const res = await fetch(`${API}/api/athena/subproducts`);
    expect(res.status).toBe(404);
  });
  test('GET /api/chorus/products is gone', async () => {
    const res = await fetch(`${API}/api/chorus/products`);
    expect(res.status).toBe(404);
  });
});

// #4265 — DELETED: the list, owner-filter and step-filter cases asserted
// chorus:Domain itself. The class is retired (#4216/#4237) and the routes'
// query files (domains.sparql, domain-detail.sparql) were deleted with
// it, so these 500'd on ENOENT. Wren's call 2026-09-21: don't bring them back
// as "returns 88 domains" — that is the count-the-data shape Jeff called
// brittle. The list surface is GET :3360/domains/domains.

describeIntegration('GET /api/athena/steps (retired #3702)', () => {
  test('v1 steps endpoint is gone — 410 pointing at /owl/valuestreams', async () => {
    const res = await fetch(`${API}/api/athena/steps`);
    expect(res.status).toBe(410);
    const body = await res.json();
    expect(body.message).toContain('valuestreams');
  });
});

describeIntegration('GET /api/athena/owners', () => {
  test('returns owners with domain counts', async () => {
    const res = await fetch(`${API}/api/athena/owners`);
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body.data.length).toBeGreaterThan(0);
    for (const o of body.data) {
      expect(o.label).toBeDefined();
      expect(typeof o.domainCount).toBe('number');
    }
  });
});

// ── #1860: Data-driven filter tests against spreadsheet counts ──

// #4265 — DELETED with the list cases above: step filters and the whole
// :id detail block (detail, consumes, 404-with-suggestion, the two loom
// contains cases, empty-instances). All read domain-detail.sparql, which
// was deleted with the class. Wren measured the replacement 2026-09-21:
// GET :3360/domains/domains/<id> serves iri, label, comment only — owner,
// step, consumes and consumedBy are no longer exposed, so repointing these
// would assert fields nobody serves. The lost four are content, not tests;
// Wren is naming that separately.

describeIntegration('GET /api/athena/machines', () => {
  test('returns machines with labels', async () => {
    const res = await fetch(`${API}/api/athena/machines`);
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body._meta.query_name).toBe('machines');
    expect(typeof body._meta.duration_ms).toBe('number');
    expect(body.data.length).toBeGreaterThan(0);
    for (const m of body.data) {
      expect(m.label).toBeDefined();
    }
  });

  // #4079: the literal counts (9 and 1) were a snapshot from spring; the model is the
  // source, so assert shape and presence, and that the two machines together hold
  // every service the graph places on a machine.
  test('both machines exist and each hosts at least one service', async () => {
    const res = await fetch(`${API}/api/athena/machines`);
    expect(res.status).toBe(200);
    const body = await res.json();
    const library = body.data.find(m => m.label === 'Library');
    const bedroom = body.data.find(m => m.label === 'Bedroom');
    expect(library).toBeDefined();
    expect(bedroom).toBeDefined();
    expect(Array.isArray(library.services)).toBe(true);
    expect(library.services.length).toBeGreaterThanOrEqual(1);
    expect(Array.isArray(bedroom.services)).toBe(true);
    expect(bedroom.services.length).toBeGreaterThanOrEqual(1);
  });
});

describeIntegration('_meta envelope', () => {
  test('all endpoints include query_name, duration_ms, cached', async () => {
    // #4274: 'domains' (the list route) retired — gone with chorus:Domain (#4265);
    // Jeff 2026-09-23 "we are retiring domains". The /:id/* facets still serve and keep their tests.
    const endpoints = ['health', 'owners', 'machines']; // products/subproducts retired #3603; steps retired #3702
    for (const ep of endpoints) {
      const res = await fetch(`${API}/api/athena/${ep}`);
      const body = await res.json();
      expect(body._meta.query_name).toBe(ep);
      expect(typeof body._meta.duration_ms).toBe('number');
      expect(typeof body._meta.cached).toBe('boolean');
    }
  });
});

// === #1904: Roles domain — parent + 4 domains ===




// === #1907: Prior Art section ===

describeIntegration('404 handler', () => {
  test('unknown path returns 404 with available endpoints', async () => {
    const res = await fetch(`${API}/api/athena/bogus`);
    expect(res.status).toBe(404);
    const body = await res.json();
    expect(body.data.suggestion).toBeDefined();
    expect(Array.isArray(body.data.available)).toBe(true);
  });
});

// #1892 — new read endpoints
describeIntegration('POST /api/athena/validate', () => {
  test('validates existing predicates as valid', async () => {
    const res = await fetch(`${API}/api/athena/validate`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ predicates: ['chorus:ownedBy', 'rdfs:label'] }),
    });
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body._meta.query_name).toBe('validate');
    expect(body.data.valid).toContain('chorus:ownedBy');
    expect(body.data.valid).toContain('rdfs:label');
    expect(body.data.missing).toEqual([]);
    expect(body.data.valid_count).toBe(2);
    expect(body.data.missing_count).toBe(0);
  });

  test('detects missing predicates', async () => {
    const res = await fetch(`${API}/api/athena/validate`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ predicates: ['chorus:ownedBy', 'chorus:doesNotExist'] }),
    });
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body.data.valid).toContain('chorus:ownedBy');
    expect(body.data.missing).toContain('chorus:doesNotExist');
    expect(body.data.valid_count).toBe(1);
    expect(body.data.missing_count).toBe(1);
  });

  test('rejects empty predicates', async () => {
    const res = await fetch(`${API}/api/athena/validate`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ predicates: [] }),
    });
    expect(res.status).toBe(400);
    const body = await res.json();
    expect(body.data.error).toBeDefined();
  });

  test('rejects missing body', async () => {
    const res = await fetch(`${API}/api/athena/validate`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({}),
    });
    expect(res.status).toBe(400);
  });
});

// === #1899: Domain detail sections ===

