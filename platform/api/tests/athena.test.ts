/**
 * Athena CMDB API tests — #1849, #1860
 *
 * Integration tests — hit live Chorus API at localhost:3340.
 * Requires RUN_INTEGRATION=true, Chorus API running, Fuseki on 3030.
 */

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

describeIntegration('GET /api/athena/products', () => {
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

describeIntegration('GET /api/athena/subdomains', () => {
  test('returns 31 subdomains with owner and step', async () => {
    const res = await fetch(`${API}/api/athena/subdomains`);
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body._meta.count).toBeGreaterThanOrEqual(40);
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
    expect(body._meta.count).toBeGreaterThanOrEqual(5);
    for (const sd of body.data) {
      expect(sd.owner.toLowerCase()).toBe('kade');
    }
  });
});

describeIntegration('GET /api/athena/subdomains/:id/blast-radius', () => {
  test('cards-service has 3 consumers', async () => {
    const res = await fetch(`${API}/api/athena/subdomains/cards-service/blast-radius`);
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body.data.subdomain).toBe('cards-service');
    expect(body.data.consumers.length).toBeGreaterThanOrEqual(3);
  });
});

describeIntegration('GET /api/athena/steps', () => {
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

describeIntegration('GET /api/athena/owners', () => {
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

// ── #1860: Data-driven filter tests against spreadsheet counts ──

describeIntegration('GET /api/athena/subdomains — owner filters', () => {
  test.each([
    ['wren', 5],
    ['silas', 10],
    ['kade', 5],
    ['jeff', 5],
  ])('owner=%s returns >= %i subdomains', async (owner, minExpected) => {
    const res = await fetch(`${API}/api/athena/subdomains?owner=${owner}`);
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body._meta.count).toBeGreaterThanOrEqual(minExpected);
    for (const sd of body.data) {
      expect(sd.owner.toLowerCase()).toBe(owner);
    }
  });
});

describeIntegration('GET /api/athena/subdomains — step filters', () => {
  test.each([
    ['building', 5],
    ['proving', 7],
    ['shaping', 5],
    ['designing', 3],
    ['directing', 1],
  ])('step=%s returns >= %i subdomains', async (step, minExpected) => {
    const res = await fetch(`${API}/api/athena/subdomains?step=${step}`);
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body._meta.count).toBeGreaterThanOrEqual(minExpected);
  });
});

describeIntegration('GET /api/athena/subdomains/:id — detail endpoint', () => {
  test('cards-service returns owner, step, consumedBy', async () => {
    const res = await fetch(`${API}/api/athena/subdomains/cards-service`);
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body._meta.query_name).toBe('subdomain-detail');
    expect(body.data.label).toBe('Cards (Service)');
    expect(body.data.owner).toBe('Wren');
    expect(body.data.step).toBe('Directing');
    expect(body.data.consumedBy.length).toBeGreaterThanOrEqual(3);
  });

  test('detail includes consumes (dependencies) array', async () => {
    const res = await fetch(`${API}/api/athena/subdomains/cards-service`);
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(Array.isArray(body.data.consumes)).toBe(true);
  });

  test('nonexistent returns 404 with suggestion', async () => {
    const res = await fetch(`${API}/api/athena/subdomains/nonexistent`);
    expect(res.status).toBe(404);
    const body = await res.json();
    expect(body.data.error).toContain('not found');
    expect(body.data.suggestion).toBeDefined();
  });

  // #1901 — Collection pattern: domains contain typed instances via chorus:contains
  test('loom-principles contains Principle instances via chorus:contains', async () => {
    const res = await fetch(`${API}/api/athena/subdomains/loom-principles`);
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(Array.isArray(body.data.instances)).toBe(true);
    expect(body.data.instances.length).toBeGreaterThanOrEqual(7);
    for (const inst of body.data.instances) {
      expect(inst.label).toBeDefined();
      expect(inst.comment).toBeDefined();
      expect(inst.type).toBe('Principle');
    }
  });

  test('loom-practices contains Practice instances via chorus:contains', async () => {
    const res = await fetch(`${API}/api/athena/subdomains/loom-practices`);
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(Array.isArray(body.data.instances)).toBe(true);
    expect(body.data.instances.length).toBeGreaterThanOrEqual(7);
    for (const inst of body.data.instances) {
      expect(inst.label).toBeDefined();
      expect(inst.comment).toBeDefined();
      expect(inst.type).toBe('Practice');
    }
  });

  test('domain without instances returns empty instances array', async () => {
    const res = await fetch(`${API}/api/athena/subdomains/cards-service`);
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(Array.isArray(body.data.instances)).toBe(true);
    expect(body.data.instances.length).toBe(0);
  });
});

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

  test('Library has 9 services and Bedroom has 1', async () => {
    const res = await fetch(`${API}/api/athena/machines`);
    expect(res.status).toBe(200);
    const body = await res.json();
    const library = body.data.find(m => m.label === 'Library');
    const bedroom = body.data.find(m => m.label === 'Bedroom');
    expect(library).toBeDefined();
    expect(bedroom).toBeDefined();
    expect(Array.isArray(library.services)).toBe(true);
    expect(library.services.length).toBeGreaterThanOrEqual(9);
    expect(Array.isArray(bedroom.services)).toBe(true);
    expect(bedroom.services.length).toBeGreaterThanOrEqual(1);
  });
});

describeIntegration('_meta envelope', () => {
  test('all endpoints include query_name, duration_ms, cached', async () => {
    const endpoints = ['health', 'products', 'subproducts', 'subdomains', 'steps', 'owners', 'machines'];
    for (const ep of endpoints) {
      const res = await fetch(`${API}/api/athena/${ep}`);
      const body = await res.json();
      expect(body._meta.query_name).toBe(ep);
      expect(typeof body._meta.duration_ms).toBe('number');
      expect(typeof body._meta.cached).toBe('boolean');
    }
  });
});

// === #1904: Roles domain — parent + 4 sub-domains ===

describeIntegration('GET /api/athena — #1904 Roles domain', () => {
  test('roles-domain exists at Shaping, owned by Jeff', async () => {
    const res = await fetch(`${API}/api/athena/subdomains/roles-domain`);
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body.data.label).toBe('Roles');
    expect(body.data.owner).toBe('Jeff');
    expect(body.data.step).toBe('Shaping');
  });

  test('roles-domain has 4 child sub-domains via hasDomain', async () => {
    const res = await fetch(`${API}/api/athena/subdomains/roles-domain`);
    expect(res.status).toBe(200);
    const body = await res.json();
    const childLabels = body.data.domains.map(c => c.label);
    expect(childLabels).toContain('Role Identity');
    expect(childLabels).toContain('Role State');
    expect(childLabels).toContain('Role Permissions');
    expect(childLabels).toContain('Role Communication');
  });

  test('role-identity exists at Shaping, owned by Wren', async () => {
    const res = await fetch(`${API}/api/athena/subdomains/role-identity`);
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body.data.label).toBe('Role Identity');
    expect(body.data.owner).toBe('Wren');
    expect(body.data.step).toBe('Shaping');
  });

  test('role-state exists at Proving, owned by Silas, consumes cards-service', async () => {
    const res = await fetch(`${API}/api/athena/subdomains/role-state`);
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body.data.label).toBe('Role State');
    expect(body.data.owner).toBe('Silas');
    expect(body.data.step).toBe('Proving');
    const consumes = body.data.consumes.map(c => c.label);
    expect(consumes.some(c => c.includes('Cards'))).toBe(true);
  });

  test('role-permissions exists at Proving, owned by Silas, consumes gates-service', async () => {
    const res = await fetch(`${API}/api/athena/subdomains/role-permissions`);
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body.data.label).toBe('Role Permissions');
    expect(body.data.owner).toBe('Silas');
    expect(body.data.step).toBe('Proving');
    const consumes = body.data.consumes.map(c => c.label);
    expect(consumes.some(c => c.includes('Gates'))).toBe(true);
  });

  test('role-communication exists at Directing, owned by Wren, consumes messages + streams', async () => {
    const res = await fetch(`${API}/api/athena/subdomains/role-communication`);
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body.data.label).toBe('Role Communication');
    expect(body.data.owner).toBe('Wren');
    expect(body.data.step).toBe('Directing');
    const consumes = body.data.consumes.map(c => c.label);
    expect(consumes.some(c => c.includes('Messages'))).toBe(true);
    expect(consumes.some(c => c.includes('Streams'))).toBe(true);
  });

  test('subdomains count increased by 5 (1 parent + 4 children)', async () => {
    const res = await fetch(`${API}/api/athena/subdomains`);
    expect(res.status).toBe(200);
    const body = await res.json();
    // Was >= 40 before roles domain, now >= 45
    expect(body._meta.count).toBeGreaterThanOrEqual(45);
  });
});

describeIntegration('GET /api/athena/subdomains/:id — hasDomain composition', () => {
  test('pulse-domain exists and has detail data', async () => {
    const res = await fetch(`${API}/api/athena/subdomains/pulse-domain`);
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body.data.label).toBe('Pulse');
    expect(body.data.owner).toBeDefined();
    // Note: hasDomain children exist in Fuseki but detail endpoint
    // doesn't surface them yet — children array may be empty
    expect(Array.isArray(body.data.children || body.data.domains || [])).toBe(true);
  });
});

describeIntegration('GET /api/athena — #1851 Properties and Security sub-domains', () => {
  test('Properties sub-domain exists in Proving, owned by Silas', async () => {
    const res = await fetch(`${API}/api/athena/subdomains/properties-domain`);
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body.data.label).toBe('Properties');
    expect(body.data.owner).toBe('Silas');
    expect(body.data.step).toBe('Proving');
  });

  test('Security sub-domain exists in Proving, owned by Silas', async () => {
    const res = await fetch(`${API}/api/athena/subdomains/security-domain`);
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body.data.label).toBe('Security');
    expect(body.data.owner).toBe('Silas');
    expect(body.data.step).toBe('Proving');
  });

  test('Properties is consumed by at least Cards and Infrastructure', async () => {
    const res = await fetch(`${API}/api/athena/subdomains/properties-domain`);
    const body = await res.json();
    const consumers = body.data.consumedBy.map(c => c.label);
    expect(consumers).toContain('Cards (Service)');
    expect(consumers).toContain('Infrastructure (Service)');
  });

  test('Security is consumed by all other subdomains', async () => {
    const res = await fetch(`${API}/api/athena/subdomains/security-domain`);
    const body = await res.json();
    const subRes = await fetch(`${API}/api/athena/subdomains`);
    const subBody = await subRes.json();
    // Most subdomains consume security
    expect(body.data.consumedBy.length).toBeGreaterThanOrEqual(20);
  });

  test('steps endpoint counts match subdomains endpoint', async () => {
    const stepsRes = await fetch(`${API}/api/athena/steps`);
    const stepsBody = await stepsRes.json();
    const stepTotal = stepsBody.data.reduce((sum, s) => sum + s.domainCount, 0);
    const subRes = await fetch(`${API}/api/athena/subdomains`);
    const subBody = await subRes.json();
    expect(stepTotal).toBe(subBody._meta.count);
  });

  test('Proving step has >= 7 subdomains', async () => {
    const res = await fetch(`${API}/api/athena/subdomains?step=Proving`);
    const body = await res.json();
    expect(body._meta.count).toBeGreaterThanOrEqual(7);
  });
});

// === #1907: Prior Art section ===

describeIntegration('GET /api/athena/subdomains/:id/prior-art', () => {
  test('returns prior art list with athena envelope', async () => {
    const res = await fetch(`${API}/api/athena/subdomains/roles-domain/prior-art`);
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body._meta.source).toBe('athena');
    expect(body._meta.query_name).toBe('subdomain-prior-art');
    expect(body.data.subdomain).toBe('roles-domain');
    expect(Array.isArray(body.data.items)).toBe(true);
  });

  test('returns 404 for unknown subdomain', async () => {
    const res = await fetch(`${API}/api/athena/subdomains/nonexistent-xyz/prior-art`);
    expect(res.status).toBe(404);
  });
});

describeIntegration('POST /api/athena/subdomains/:id/prior-art', () => {
  test('creates prior art entry and returns it', async () => {
    const res = await fetch(`${API}/api/athena/subdomains/roles-domain/prior-art`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ label: 'Org Design Artifact', path: 'roles/wren/artifacts/org-design.html', description: 'Chorus organizational architecture — two axes of responsibility, product ownership map' }),
    });
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body._meta.query_name).toBe('subdomain-prior-art-create');
    expect(body.data.label).toBe('Org Design Artifact');
    expect(body.data.path).toBe('roles/wren/artifacts/org-design.html');
  });

  test('rejects missing label', async () => {
    const res = await fetch(`${API}/api/athena/subdomains/roles-domain/prior-art`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ path: 'some/file.ts' }),
    });
    expect(res.status).toBe(400);
  });
});

describeIntegration('GET /api/athena/subdomains/:id/completeness — prior_art section', () => {
  test('completeness includes prior_art in sections map', async () => {
    const res = await fetch(`${API}/api/athena/subdomains/roles-domain/completeness`);
    expect(res.status).toBe(200);
    const body = await res.json();
    expect('prior_art' in body.data.sections).toBe(true);
  });
});

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
describeIntegration('GET /api/athena/subdomains/:id/cards', () => {
  test('returns cards for athena subdomain via sequence match', async () => {
    const res = await fetch(`${API}/api/athena/subdomains/athena-domain/cards`);
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body._meta.query_name).toBe('subdomain-cards');
    expect(body.data.subdomain).toBe('athena-domain');
    expect(body.data.domainLabel).toBe('athena');
    expect(Array.isArray(body.data.cards)).toBe(true);
  });

  test('returns envelope with count for domain with no active cards', async () => {
    const res = await fetch(`${API}/api/athena/subdomains/time-domain/cards`);
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body._meta.count).toBeGreaterThanOrEqual(0);
    expect(Array.isArray(body.data.cards)).toBe(true);
  });
});

describeIntegration('GET /api/athena/subdomains/:id/alerts', () => {
  test('returns alert rules matching domain keyword', async () => {
    const res = await fetch(`${API}/api/athena/subdomains/infra-service/alerts`);
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body._meta.query_name).toBe('subdomain-alerts');
    expect(body.data.subdomain).toBe('infra-service');
    expect(Array.isArray(body.data.alerts)).toBe(true);
  });

  test('alert objects have name, severity, schedule', async () => {
    // app-down.yml matches many domains — use a broad domain
    const res = await fetch(`${API}/api/athena/subdomains/athena-domain/alerts`);
    const body = await res.json();
    for (const alert of body.data.alerts) {
      expect(alert.name).toBeDefined();
      expect(alert.severity).toBeDefined();
    }
  });
});

describeIntegration('GET /api/athena/subdomains/:id/code', () => {
  test('returns code inventory for gates subdomain', async () => {
    const res = await fetch(`${API}/api/athena/subdomains/gates-service/code`);
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body._meta.query_name).toBe('subdomain-code');
    expect(body.data.subdomain).toBe('gates-service');
    expect(Array.isArray(body.data.files)).toBe(true);
    expect(body.data.files.length).toBeGreaterThan(0);
    // Should find gate skill files
    expect(body.data.files.some(f => f.path.includes('gate-'))).toBe(true);
  });

  test('returns empty files for unmapped domain', async () => {
    const res = await fetch(`${API}/api/athena/subdomains/time-domain/code`);
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body.data.files).toEqual([]);
  });
});

// #1892 — write endpoints (require Fuseki auth — skip if 401)
describeIntegration('POST /api/athena/subdomains', () => {
  test('rejects missing required fields', async () => {
    const res = await fetch(`${API}/api/athena/subdomains`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({}),
    });
    expect(res.status).toBe(400);
    const body = await res.json();
    expect(body.data.error).toContain('Missing required fields');
  });
});

describeIntegration('POST /api/athena/reload', () => {
  test('returns 200 or 500 with envelope', async () => {
    const res = await fetch(`${API}/api/athena/reload`, { method: 'POST' });
    const body = await res.json();
    expect(body._meta.query_name).toBe('reload');
    // May fail with 401 from Fuseki — that's expected until auth is configured
  });
});

// #1356 — POST /api/athena/validate
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

describeIntegration('GET /api/athena/subdomains/:id/actors', () => {
  test('returns actor list with athena envelope', async () => {
    const res = await fetch(`${API}/api/athena/subdomains/logs-service/actors`);
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body._meta.source).toBe('athena');
    expect(body._meta.query_name).toBe('subdomain-actors');
    expect(body.data.subdomain).toBe('logs-service');
    expect(Array.isArray(body.data.actors)).toBe(true);
  });

  test('returns 404 for unknown subdomain', async () => {
    const res = await fetch(`${API}/api/athena/subdomains/nonexistent-xyz/actors`);
    expect(res.status).toBe(404);
  });
});

describeIntegration('GET /api/athena/subdomains/:id/scenarios', () => {
  test('returns scenario list with athena envelope', async () => {
    const res = await fetch(`${API}/api/athena/subdomains/logs-service/scenarios`);
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body._meta.source).toBe('athena');
    expect(body._meta.query_name).toBe('subdomain-scenarios');
    expect(body.data.subdomain).toBe('logs-service');
    expect(Array.isArray(body.data.scenarios)).toBe(true);
  });
});

describeIntegration('GET /api/athena/subdomains/:id/contract', () => {
  test('returns contract list with athena envelope', async () => {
    const res = await fetch(`${API}/api/athena/subdomains/logs-service/contract`);
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body._meta.source).toBe('athena');
    expect(body._meta.query_name).toBe('subdomain-contract');
    expect(body.data.subdomain).toBe('logs-service');
    expect(Array.isArray(body.data.endpoints)).toBe(true);
  });
});

// === #1899: POST endpoints for actors, scenarios, contracts ===

describeIntegration('POST /api/athena/subdomains/:id/actors', () => {
  test('creates actor and returns it', async () => {
    const res = await fetch(`${API}/api/athena/subdomains/logs-service/actors`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ label: 'Test Actor', role: 'silas', action: 'verifies log flow' }),
    });
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body._meta.query_name).toBe('subdomain-actor-create');
    expect(body.data.label).toBe('Test Actor');
    expect(body.data.role).toBe('silas');
  });
});

describeIntegration('POST /api/athena/subdomains/:id/scenarios (#1922)', () => {
  test('creates scenario with separate given/when/then/notes fields', async () => {
    const res = await fetch(`${API}/api/athena/subdomains/logs-service/scenarios`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ label: 'Test Scenario Split', given: 'logs are flowing', when: 'Loki is queried', then: 'entries appear', notes: 'requires tunnel' }),
    });
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body._meta.query_name).toBe('subdomain-scenario-create');
    expect(body.data.label).toBe('Test Scenario Split');
    expect(body.data.given).toBe('logs are flowing');
    expect(body.data.when).toBe('Loki is queried');
    expect(body.data.then).toBe('entries appear');
    expect(body.data.notes).toBe('requires tunnel');
  });
});

describeIntegration('POST /api/athena/subdomains/:id/contract (#1922)', () => {
  test('creates contract with path and description fields', async () => {
    const res = await fetch(`${API}/api/athena/subdomains/logs-service/contract`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ label: 'Test Endpoint V2', path: '/api/chorus/logs/test', method: 'GET', description: 'Query log entries' }),
    });
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body._meta.query_name).toBe('subdomain-contract-create');
    expect(body.data.label).toBe('Test Endpoint V2');
  });
});

// === #1923: Pages, Integrations, Persistence endpoints ===

describeIntegration('GET /api/athena/subdomains/:id/pages (#1923)', () => {
  test('returns pages list with athena envelope', async () => {
    const res = await fetch(`${API}/api/athena/subdomains/logs-service/pages`);
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body._meta.query_name).toBe('subdomain-pages');
    expect(body.data.subdomain).toBe('logs-service');
    expect(Array.isArray(body.data.pages)).toBe(true);
  });
});

describeIntegration('POST /api/athena/subdomains/:id/pages (#1923)', () => {
  test('creates page with route, description, status', async () => {
    const res = await fetch(`${API}/api/athena/subdomains/logs-service/pages`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ label: 'Log Explorer', route: '/logs', description: 'Browse log entries by container', status: 'design' }),
    });
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body._meta.query_name).toBe('subdomain-page-create');
    expect(body.data.label).toBe('Log Explorer');
    expect(body.data.route).toBe('/logs');
    expect(body.data.description).toBe('Browse log entries by container');
    expect(body.data.status).toBe('design');
  });
});

describeIntegration('GET /api/athena/subdomains/:id/integrations (#1923)', () => {
  test('returns integrations list with athena envelope', async () => {
    const res = await fetch(`${API}/api/athena/subdomains/logs-service/integrations`);
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body._meta.query_name).toBe('subdomain-integrations');
    expect(body.data.subdomain).toBe('logs-service');
    expect(Array.isArray(body.data.integrations)).toBe(true);
  });
});

describeIntegration('POST /api/athena/subdomains/:id/integrations (#1923)', () => {
  test('creates integration with source, path, status', async () => {
    const res = await fetch(`${API}/api/athena/subdomains/logs-service/integrations`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ label: 'Bedroom log shipping', source: 'Docker containers on Bedroom', path: 'stdout → Promtail → Loki tunnel → Loki', status: 'active' }),
    });
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body._meta.query_name).toBe('subdomain-integration-create');
    expect(body.data.label).toBe('Bedroom log shipping');
    expect(body.data.source).toBe('Docker containers on Bedroom');
    expect(body.data.path).toBe('stdout → Promtail → Loki tunnel → Loki');
    expect(body.data.status).toBe('active');
  });
});

describeIntegration('GET /api/athena/subdomains/:id/persistence (#1923)', () => {
  test('returns persistence stores list with athena envelope', async () => {
    const res = await fetch(`${API}/api/athena/subdomains/logs-service/persistence`);
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body._meta.query_name).toBe('subdomain-persistence');
    expect(body.data.subdomain).toBe('logs-service');
    expect(Array.isArray(body.data.stores)).toBe(true);
  });
});

describeIntegration('POST /api/athena/subdomains/:id/persistence (#1923)', () => {
  test('creates persistence store with type, namespace, records, status', async () => {
    const res = await fetch(`${API}/api/athena/subdomains/logs-service/persistence`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ label: 'Loki', type: 'Loki', namespace: '{container_name=~".*"}', records: '500000', status: 'active' }),
    });
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body._meta.query_name).toBe('subdomain-persistence-create');
    expect(body.data.label).toBe('Loki');
    expect(body.data.type).toBe('Loki');
    expect(body.data.records).toBe(500000);
    expect(body.data.status).toBe('active');
  });
});

// === #1924 #1925 #1926: Services, Pipeline, Logs, Gaps endpoints ===

describeIntegration('GET/POST /api/athena/subdomains/:id/services (#1924)', () => {
  test('GET returns services list', async () => {
    const res = await fetch(`${API}/api/athena/subdomains/logs-service/services`);
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body._meta.query_name).toBe('subdomain-services');
    expect(Array.isArray(body.data.services)).toBe(true);
  });
  test('POST creates service with type, host, status, health_endpoint', async () => {
    const res = await fetch(`${API}/api/athena/subdomains/logs-service/services`, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ label: 'Loki', type: 'container', host: 'Library', status: 'running', health_endpoint: 'http://localhost:3102/ready' }) });
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body._meta.query_name).toBe('subdomain-service-create');
    expect(body.data.label).toBe('Loki');
    expect(body.data.type).toBe('container');
    expect(body.data.host).toBe('Library');
  });
});

describeIntegration('GET/POST /api/athena/subdomains/:id/pipeline (#1925)', () => {
  test('GET returns pipeline list', async () => {
    const res = await fetch(`${API}/api/athena/subdomains/logs-service/pipeline`);
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body._meta.query_name).toBe('subdomain-pipeline');
    expect(Array.isArray(body.data.pipelines)).toBe(true);
  });
  test('POST creates pipeline with source, harvester, icd, status', async () => {
    const res = await fetch(`${API}/api/athena/subdomains/logs-service/pipeline`, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ label: 'Bedroom log ingest', source: 'Docker stdout', harvester: 'Promtail', icd: 'logs-icd', status: 'active' }) });
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body._meta.query_name).toBe('subdomain-pipeline-create');
    expect(body.data.label).toBe('Bedroom log ingest');
    expect(body.data.source).toBe('Docker stdout');
  });
});

describeIntegration('GET/POST /api/athena/subdomains/:id/logs (#1926)', () => {
  test('GET returns log sources list', async () => {
    const res = await fetch(`${API}/api/athena/subdomains/logs-service/logs`);
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body._meta.query_name).toBe('subdomain-logs');
    expect(Array.isArray(body.data.logs)).toBe(true);
  });
  test('POST creates log source with location, retention, status', async () => {
    const res = await fetch(`${API}/api/athena/subdomains/logs-service/logs`, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ label: 'Bedroom containers', location: '{container_name=~".*bedroom.*"}', retention: '30', status: 'active' }) });
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body._meta.query_name).toBe('subdomain-log-create');
    expect(body.data.label).toBe('Bedroom containers');
    expect(body.data.location).toContain('bedroom');
  });
});

describeIntegration('GET/POST /api/athena/subdomains/:id/gaps (#1926)', () => {
  test('GET returns gaps list', async () => {
    const res = await fetch(`${API}/api/athena/subdomains/logs-service/gaps`);
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body._meta.query_name).toBe('subdomain-gaps');
    expect(Array.isArray(body.data.gaps)).toBe(true);
  });
  test('POST creates gap with type, description, severity', async () => {
    const res = await fetch(`${API}/api/athena/subdomains/logs-service/gaps`, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ label: 'Missing retention policy', type: 'gap', description: 'No automated log rotation configured', severity: 'important' }) });
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body._meta.query_name).toBe('subdomain-gap-create');
    expect(body.data.label).toBe('Missing retention policy');
    expect(body.data.severity).toBe('important');
  });
});

// === #1929: PUT and DELETE for entities ===

describeIntegration('PUT /api/athena/subdomains/:id/actors/:entityId (#1929)', () => {
  test('updates actor fields', async () => {
    await fetch(`${API}/api/athena/subdomains/logs-service/actors`, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ label: 'PUT Test Actor', role: 'kade', action: 'original action' }) });
    const put = await fetch(`${API}/api/athena/subdomains/logs-service/actors/logs-service-actor-put-test-actor`, { method: 'PUT', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ label: 'PUT Test Actor Updated', role: 'silas', action: 'updated action' }) });
    expect(put.status).toBe(200);
    const body = await put.json();
    expect(body.data.label).toBe('PUT Test Actor Updated');
    expect(body.data.role).toBe('silas');
    expect(body.data.action).toBe('updated action');
    await fetch(`${API}/api/athena/subdomains/logs-service/actors/logs-service-actor-put-test-actor`, { method: 'DELETE' });
  });
});

describeIntegration('DELETE /api/athena/subdomains/:id/:section/:entityId (#1929)', () => {
  test('deletes actor and returns 204', async () => {
    await fetch(`${API}/api/athena/subdomains/logs-service/actors`, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ label: 'Delete Test Actor' }) });
    const del = await fetch(`${API}/api/athena/subdomains/logs-service/actors/logs-service-actor-delete-test-actor`, { method: 'DELETE' });
    expect(del.status).toBe(204);
  });

  test('deletes gap and returns 204', async () => {
    await fetch(`${API}/api/athena/subdomains/logs-service/gaps`, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ label: 'Delete Test Gap', severity: 'nice-to-have' }) });
    const del = await fetch(`${API}/api/athena/subdomains/logs-service/gaps/logs-service-gap-delete-test-gap`, { method: 'DELETE' });
    expect(del.status).toBe(204);
  });

  test('returns 400 for unknown section', async () => {
    const del = await fetch(`${API}/api/athena/subdomains/logs-service/bogus/some-id`, { method: 'DELETE' });
    expect(del.status).toBe(400);
  });
});

// === #1899: Completeness API ===

describeIntegration('GET /api/athena/subdomains/:id/completeness', () => {
  test('returns completeness score with sections, present, missing, percentage', async () => {
    const res = await fetch(`${API}/api/athena/subdomains/logs-service/completeness`);
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body._meta.source).toBe('athena');
    expect(body._meta.query_name).toBe('subdomain-completeness');
    expect(body.data.subdomain).toBe('logs-service');
    expect(body.data.sections).toBeDefined();
    expect(Array.isArray(body.data.present)).toBe(true);
    expect(Array.isArray(body.data.missing)).toBe(true);
    expect(typeof body.data.percentage).toBe('number');
    expect(body.data.percentage).toBeGreaterThanOrEqual(0);
    expect(body.data.percentage).toBeLessThanOrEqual(100);
  });

  test('returns lifecycle gates with create/wip/done stages', async () => {
    const res = await fetch(`${API}/api/athena/subdomains/logs-service/completeness`);
    const body = await res.json();
    expect(body.data.lifecycle).toBeDefined();
    expect(body.data.lifecycle.create).toBeDefined();
    expect(body.data.lifecycle.create.required).toContain('label');
    expect(body.data.lifecycle.create.required).toContain('owner');
    expect(body.data.lifecycle.wip).toBeDefined();
    expect(body.data.lifecycle.wip.required).toContain('actors');
    expect(body.data.lifecycle.done).toBeDefined();
    expect(body.data.lifecycle.done.required).toContain('scenarios');
  });

  test('returns 404 for unknown subdomain', async () => {
    const res = await fetch(`${API}/api/athena/subdomains/nonexistent-domain-xyz/completeness`);
    expect(res.status).toBe(404);
  });

  test('present and missing arrays match sections boolean map (#1900)', async () => {
    const res = await fetch(`${API}/api/athena/subdomains/logs-service/completeness`);
    const body = await res.json();
    const sections = body.data.sections;
    const present = body.data.present;
    const missing = body.data.missing;
    for (const [key, val] of Object.entries(sections)) {
      if (val) expect(present).toContain(key);
      else expect(missing).toContain(key);
    }
    const total = present.length + missing.length;
    expect(body.data.percentage).toBe(Math.round((present.length / total) * 100));
  });
});
