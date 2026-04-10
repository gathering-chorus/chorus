/**
 * Athena CMDB API tests — #1849
 *
 * Tests hit the live Chorus API at localhost:3340.
 * Requires: Chorus API running, Fuseki running on 3030 with ontology loaded.
 */

const API = process.env.CHORUS_API || 'http://localhost:3340';

let apiUp = false;

beforeAll(async () => {
  try {
    const res = await fetch(`${API}/api/athena/health`);
    apiUp = res.ok;
  } catch {
    apiUp = false;
  }
});

describe('GET /api/athena/health', () => {
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

describe('GET /api/athena/products', () => {
  test('returns product list with uri and label', async () => {
    const res = await fetch(`${API}/api/athena/products`);
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body._meta.source).toBe('athena');
    expect(Array.isArray(body.data)).toBe(true);
    expect(body.data.length).toBeGreaterThan(0);
    for (const p of body.data) {
      expect(p.uri).toBeDefined();
      expect(p.label).toBeDefined();
    }
  });
});

describe('GET /api/athena/subdomains', () => {
  test('returns 31 subdomains with owner and step', async () => {
    const res = await fetch(`${API}/api/athena/subdomains`);
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body._meta.count).toBe(31);
    for (const sd of body.data) {
      expect(sd.label).toBeDefined();
      expect(sd.owner).toBeDefined();
      expect(sd.step).toBeDefined();
    }
  });

  test('filters by owner', async () => {
    const res = await fetch(`${API}/api/athena/subdomains?owner=kade`);
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body._meta.count).toBe(5);
    for (const sd of body.data) {
      expect(sd.owner.toLowerCase()).toBe('kade');
    }
  });
});

describe('GET /api/athena/subdomains/:id/blast-radius', () => {
  test('cards-service has 3 consumers', async () => {
    const res = await fetch(`${API}/api/athena/subdomains/cards-service/blast-radius`);
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body.data.subdomain).toBe('cards-service');
    expect(body.data.consumers.length).toBe(3);
  });
});

describe('GET /api/athena/steps', () => {
  test('returns value stream steps with subdomains', async () => {
    const res = await fetch(`${API}/api/athena/steps`);
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body.data.length).toBeGreaterThan(0);
    for (const step of body.data) {
      expect(step.label).toBeDefined();
      expect(Array.isArray(step.subdomains)).toBe(true);
    }
  });
});

describe('GET /api/athena/owners', () => {
  test('returns owners with subdomain counts', async () => {
    const res = await fetch(`${API}/api/athena/owners`);
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body.data.length).toBeGreaterThan(0);
    for (const o of body.data) {
      expect(o.label).toBeDefined();
      expect(typeof o.subdomainCount).toBe('number');
    }
  });
});
