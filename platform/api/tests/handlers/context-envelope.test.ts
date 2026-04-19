/**
 * context-envelope unit tests (#2234 Step 3).
 *
 * Describes what a Context handler sees when it calls stampHeader with a
 * stubbed SPARQL client. No live Fuseki — fixture bindings returned from
 * client.query(). Mirrors the DI pattern used by every Athena handler test.
 */

import {
  stampHeader,
  buildEnvelope,
  type StampSparqlClient,
  type SparqlSelectResult,
} from '../../src/lib/context-envelope';

function clientReturning(bindings: SparqlSelectResult['results']): StampSparqlClient {
  return { query: async () => ({ results: bindings }) };
}

function okBinding(step: string, product: string): SparqlSelectResult['results'] {
  return { bindings: [{ step: { value: step }, product: { value: product } }] };
}

describe('stampHeader', () => {
  it('system-scoped (domainId=null) → timestamp only, no step/product/domain', async () => {
    const client = clientReturning({ bindings: [] });
    const h = await stampHeader(client, null);
    expect(h.timestamp).toMatch(/^\d{4}-\d{2}-\d{2}T/);
    expect(h).not.toHaveProperty('step');
    expect(h).not.toHaveProperty('product');
    expect(h).not.toHaveProperty('domain');
    expect(h).not.toHaveProperty('subdomain');
  });

  it('domain-scoped with graph match → step + product + domain + timestamp', async () => {
    const client = clientReturning(okBinding('building', 'chorus'));
    const h = await stampHeader(client, 'chorus');
    expect(h.step).toBe('building');
    expect(h.product).toBe('chorus');
    expect(h.domain).toBe('chorus');
    expect(h).not.toHaveProperty('subdomain');
  });

  it('subdomain-scoped with graph match → all four fields', async () => {
    const client = clientReturning(okBinding('building', 'chorus'));
    const h = await stampHeader(client, 'chorus', 'chorus-hooks');
    expect(h.subdomain).toBe('chorus-hooks');
    expect(h.domain).toBe('chorus');
    expect(h.step).toBe('building');
    expect(h.product).toBe('chorus');
  });

  it('domain given but no graph match → domain + timestamp only (graceful absence)', async () => {
    const client = clientReturning({ bindings: [] });
    const h = await stampHeader(client, 'chorus');
    expect(h.domain).toBe('chorus');
    expect(h).not.toHaveProperty('step');
    expect(h).not.toHaveProperty('product');
  });

  it('partial graph binding (step only) → step present, product absent', async () => {
    const client: StampSparqlClient = {
      query: async () => ({ results: { bindings: [{ step: { value: 'building' } }] } }),
    };
    const h = await stampHeader(client, 'chorus');
    expect(h.step).toBe('building');
    expect(h).not.toHaveProperty('product');
    expect(h.domain).toBe('chorus');
  });

  it('SPARQL client throws → envelope still returns with domain + timestamp', async () => {
    const client: StampSparqlClient = { query: async () => { throw new Error('fuseki down'); } };
    const h = await stampHeader(client, 'chorus');
    expect(h.domain).toBe('chorus');
    expect(h.timestamp).toBeTruthy();
    expect(h).not.toHaveProperty('step');
  });

  it('query body embeds the domain id', async () => {
    const captured: string[] = [];
    const client: StampSparqlClient = {
      query: async (q: string) => { captured.push(q); return { results: { bindings: [] } }; },
    };
    await stampHeader(client, 'photos');
    expect(captured[0]).toContain('chorus:name "photos"');
  });

  it('domain id with an embedded quote is escaped, not injected', async () => {
    const captured: string[] = [];
    const client: StampSparqlClient = {
      query: async (q: string) => { captured.push(q); return { results: { bindings: [] } }; },
    };
    await stampHeader(client, 'weird"name');
    expect(captured[0]).toContain('"weird\\"name"');
  });

  it('omitted fields are absent, not null (JSON-round-trip)', async () => {
    const client = clientReturning({ bindings: [] });
    const h = await stampHeader(client, null);
    const keys = Object.keys(JSON.parse(JSON.stringify(h)));
    expect(keys).toEqual(['timestamp']);
  });
});

describe('buildEnvelope', () => {
  it('composes header + source + data into a single object', async () => {
    const client = clientReturning(okBinding('building', 'chorus'));
    const header = await stampHeader(client, 'chorus');
    const env = buildEnvelope(header, '/api/chorus/context/board/wip', { total: 0, cards: [] });
    expect(env.domain).toBe('chorus');
    expect(env.step).toBe('building');
    expect(env.product).toBe('chorus');
    expect(env.source).toBe('/api/chorus/context/board/wip');
    expect(env.data).toEqual({ total: 0, cards: [] });
    expect(env.timestamp).toBeTruthy();
  });

  it('system-scoped envelope has timestamp + source + data but no header fields', async () => {
    const client = clientReturning({ bindings: [] });
    const header = await stampHeader(client, null);
    const env = buildEnvelope(header, '/api/chorus/context/health', { status: 'ok' });
    const keys = Object.keys(JSON.parse(JSON.stringify(env))).sort();
    expect(keys).toEqual(['data', 'source', 'timestamp']);
  });
});
