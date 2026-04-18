/**
 * #2174 — Chorus response quality for AX.
 *
 * Integration tests — hit the live Chorus API at localhost:3340.
 * Hits a real populated index; gated by RUN_INTEGRATION=true so CI can skip.
 */

const INTEGRATION_ENABLED = process.env.RUN_INTEGRATION === 'true';
const API = process.env.CHORUS_API || 'http://localhost:3340';
const d = INTEGRATION_ENABLED ? describe : describe.skip;

d('GET /api/chorus/search — #2174 AX response quality', () => {
  // AC-5 — versioned schema
  test('_meta.schema_version is present and semver-shaped', async () => {
    const res = await fetch(`${API}/api/chorus/search?q=test&limit=5`);
    const body = await res.json() as any;
    expect(body._meta).toBeDefined();
    expect(typeof body._meta.schema_version).toBe('string');
    expect(body._meta.schema_version).toMatch(/^\d+\.\d+\.\d+$/);
  });

  // AC-1 — per-result freshness
  test('each result carries freshness_s (age in seconds)', async () => {
    const res = await fetch(`${API}/api/chorus/search?q=test&limit=5`);
    const body = await res.json() as any;
    if (!body.results?.length) return;
    for (const r of body.results) {
      expect(typeof r.freshness_s).toBe('number');
      expect(r.freshness_s).toBeGreaterThanOrEqual(0);
    }
  });

  // AC-2 — structured metadata
  test('each result has structured metadata fields, not just content', async () => {
    const res = await fetch(`${API}/api/chorus/search?q=test&limit=5`);
    const body = await res.json() as any;
    if (!body.results?.length) return;
    for (const r of body.results) {
      expect(r).toHaveProperty('timestamp');
      expect(r).toHaveProperty('source');
      if (r.role !== undefined) expect(typeof r.role).toBe('string');
    }
  });

  // AC-6 — default limit (token economy)
  test('default limit is <= 10', async () => {
    const res = await fetch(`${API}/api/chorus/search?q=test`);
    const body = await res.json() as any;
    expect(body.results.length).toBeLessThanOrEqual(10);
    expect(typeof body._meta.limit_applied).toBe('number');
    expect(body._meta.limit_applied).toBeLessThanOrEqual(10);
  });

  // AC-6 — caller override honored
  test('explicit ?limit=20 is honored', async () => {
    const res = await fetch(`${API}/api/chorus/search?q=test&limit=20`);
    const body = await res.json() as any;
    expect(body._meta.limit_applied).toBe(20);
  });

  // AC-6 — truncated flag
  test('_meta.truncated is a boolean', async () => {
    const res = await fetch(`${API}/api/chorus/search?q=test&limit=1`);
    const body = await res.json() as any;
    expect(typeof body._meta.truncated).toBe('boolean');
  });

  // AC-4 — mode=recency
  test('mode=recency sorts by timestamp desc', async () => {
    const res = await fetch(`${API}/api/chorus/search?q=session&mode=recency&limit=5`);
    const body = await res.json() as any;
    if (!body.results || body.results.length < 2) return;
    for (let i = 0; i < body.results.length - 1; i++) {
      const a = body.results[i].timestamp;
      const b = body.results[i + 1].timestamp;
      if (a && b) expect(a >= b).toBe(true);
    }
  });

  // AC-4 — mode echo
  test('response echoes the requested mode', async () => {
    const res = await fetch(`${API}/api/chorus/search?q=test&mode=recency&limit=3`);
    const body = await res.json() as any;
    expect(body.mode).toBe('recency');
  });

  // AC-9 — response shape locked. If any of these keys disappear or rename,
  // schema_version must bump. Lock the contract.
  test('response shape contract (AC-9 regression guard)', async () => {
    const res = await fetch(`${API}/api/chorus/search?q=test&limit=3`);
    const body = await res.json() as any;
    // Top-level
    expect(body).toHaveProperty('results');
    expect(body).toHaveProperty('total');
    expect(body).toHaveProperty('mode');
    expect(body).toHaveProperty('_meta');
    // _meta shape
    expect(body._meta).toHaveProperty('schema_version');
    expect(body._meta).toHaveProperty('domain_coverage');
    expect(body._meta).toHaveProperty('newest_result_age_s');
    expect(body._meta).toHaveProperty('stale');
    expect(body._meta).toHaveProperty('sources');
    expect(body._meta).toHaveProperty('limit_applied');
    expect(body._meta).toHaveProperty('limit_default');
    expect(body._meta).toHaveProperty('truncated');
    // Per-hit shape (if any)
    if (body.results.length > 0) {
      const r = body.results[0];
      expect(r).toHaveProperty('timestamp');
      expect(r).toHaveProperty('source');
      expect(r).toHaveProperty('freshness_s');
      expect(r).toHaveProperty('content');
    }
    // Schema version must be 1.0.0 until a bump is intentional
    expect(body._meta.schema_version).toBe('1.0.0');
  });
});
