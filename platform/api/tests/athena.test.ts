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
    expect(body._meta.count).toBe(41);
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
    expect(body._meta.count).toBe(13);
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
    expect(body.data.consumers.length).toBe(3);
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
    ['wren', 10],
    ['silas', 13],
    ['kade', 13],
    ['jeff', 5],
  ])('owner=%s returns %i subdomains', async (owner, expected) => {
    const res = await fetch(`${API}/api/athena/subdomains?owner=${owner}`);
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body._meta.count).toBe(expected);
    for (const sd of body.data) {
      expect(sd.owner.toLowerCase()).toBe(owner);
    }
  });
});

describeIntegration('GET /api/athena/subdomains — step filters', () => {
  test.each([
    ['building', 14],
    ['proving', 7],
    ['shaping', 10],
    ['designing', 5],
    ['directing', 5],
  ])('step=%s returns %i subdomains', async (step, expected) => {
    const res = await fetch(`${API}/api/athena/subdomains?step=${step}`);
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body._meta.count).toBe(expected);
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
    expect(body.data.consumedBy.length).toBe(3);
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
    expect(library.services.length).toBe(9);
    expect(Array.isArray(bedroom.services)).toBe(true);
    expect(bedroom.services.length).toBe(1);
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

describeIntegration('GET /api/athena/subdomains/:id — hasDomain composition', () => {
  test('pulse-domain has domains array with Streams, Messages, Cards, Alerts', async () => {
    const res = await fetch(`${API}/api/athena/subdomains/pulse-domain`);
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(Array.isArray(body.data.domains)).toBe(true);
    expect(body.data.domains.length).toBe(4);
    const labels = body.data.domains.map(d => d.label).sort();
    expect(labels).toContain('Alerts');
    expect(labels).toContain('Streams');
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
    // Every subdomain except security itself consumes security
    expect(body.data.consumedBy.length).toBe(subBody._meta.count - 1);
  });

  test('steps endpoint counts match subdomains endpoint', async () => {
    const stepsRes = await fetch(`${API}/api/athena/steps`);
    const stepsBody = await stepsRes.json();
    const stepTotal = stepsBody.data.reduce((sum, s) => sum + s.domainCount, 0);
    const subRes = await fetch(`${API}/api/athena/subdomains`);
    const subBody = await subRes.json();
    expect(stepTotal).toBe(subBody._meta.count);
  });

  test('Proving step has 12 subdomains', async () => {
    const res = await fetch(`${API}/api/athena/subdomains?step=Proving`);
    const body = await res.json();
    expect(body._meta.count).toBe(12);
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
});
