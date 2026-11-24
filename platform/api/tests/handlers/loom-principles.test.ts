// @test-type: unit — injected fake fetch; no athena-make, no Fuseki, brings its own world.
// @domain: principles — placed by chorus-crawl's domain rules (#4538)
/**
 * fetchLoomPrinciples — #3749 rewrite. The handler now sources the generated
 * athena-make surface (list + entity reads) instead of the retired SPARQL path
 * that read the wrong graph for months (the 2-of-29 split). Legacy envelope
 * shape preserved for loom/principles.html + the session-boot injector.
 */
import { fetchLoomPrinciples } from '../../src/handlers/loom-principles';
import { resetGeneratedApisCache, setSkippedBindLog } from '../../src/handlers/owl-generated-apis';

const list = (names: string[]) => ({
  ok: true, status: 200,
  json: async () => ({ data: names.map((n) => ({ name: n, label: '', comment: '', status: '' })) }),
}) as unknown as Response;

const entity = (name: string, fields: Record<string, string>) => ({
  ok: true, status: 200,
  json: async () => ({ data: { iri: `https://jeffbridwell.com/chorus#${name}`, ...fields } }),
}) as unknown as Response;

describe('#3749 fetchLoomPrinciples via principles-api (#5017)', () => {
  test('folds list + entity reads into the legacy envelope shape', async () => {
    const fetchFn = jest.fn(async (url: RequestInfo | URL) => {
      const u = String(url);
      if (u.endsWith('/principles/principles')) return list(['hemenway-observe', 'hemenway-connect']);
      if (u.includes('hemenway-observe')) return entity('hemenway-observe', { label: 'Observe', comment: 'Watch first.', techReading: 'Gemba.', jeffReading: 'Reads the board.', order: '1' });
      return entity('hemenway-connect', { label: 'Connect', comment: 'Relative location.', order: '2' });
    });
    const r = await fetchLoomPrinciples({ fetchFn: fetchFn as unknown as typeof fetch, owlApiUrl: 'http://owl' });
    expect(r.status).toBe(200);
    const body = r.body as { data: { principles: Array<Record<string, unknown>> } };
    expect(body.data.principles).toHaveLength(2);
    const obs = body.data.principles.find((p) => p.id === 'hemenway-observe')!;
    expect(obs.label).toBe('Observe');
    expect(obs.techReading).toBe('Gemba.');
    expect(obs.order).toBe(1);
    // #4358 — one family, no kind flag and no parent edges on the wire
    expect(obs).not.toHaveProperty('isPermacultureParent');
    expect(obs).not.toHaveProperty('parents');
    expect(body.data.principles.map((p) => p.id)).toEqual(['hemenway-observe', 'hemenway-connect']); // book order
  });

  test('NEGATIVE: athena-make unreachable → 502 error envelope, NEVER an empty principle list', async () => {
    const fetchFn = jest.fn(async () => { throw new Error('ECONNREFUSED'); });
    const r = await fetchLoomPrinciples({ fetchFn: fetchFn as unknown as typeof fetch, owlApiUrl: 'http://owl' });
    expect(r.status).toBe(502);
    const body = r.body as { data: { error?: string; principles?: unknown[] } };
    expect(body.data.error).toBeTruthy();
    expect(body.data.principles).toBeUndefined();
  });

  test('NEGATIVE: athena-make non-200 on the list → 502, the status is named', async () => {
    const fetchFn = jest.fn(async () => ({ ok: false, status: 503, json: async () => ({}) }) as unknown as Response);
    const r = await fetchLoomPrinciples({ fetchFn: fetchFn as unknown as typeof fetch, owlApiUrl: 'http://owl' });
    expect(r.status).toBe(502);
    expect(JSON.stringify(r.body)).toContain('503');
  });

  test('a single vanished entity mid-walk is skipped; the count tells the truth', async () => {
    const fetchFn = jest.fn(async (url: RequestInfo | URL) => {
      const u = String(url);
      if (u.endsWith('/principles/principles')) return list(['hemenway-observe', 'hemenway-gone']);
      if (u.includes('hemenway-observe')) return entity('hemenway-observe', { label: 'Observe', comment: 'c' });
      return { ok: false, status: 404, json: async () => ({}) } as unknown as Response;
    });
    const r = await fetchLoomPrinciples({ fetchFn: fetchFn as unknown as typeof fetch, owlApiUrl: 'http://owl' });
    const body = r.body as { data: { principles: unknown[] }; _meta: { count: number } };
    expect(body.data.principles).toHaveLength(1);
    expect(body._meta.count).toBe(1);
  });
});

describe('#5034 fetchLoomPrinciples finds principles-api from the graph rows, the way the /owl proxy does', () => {
  const MAKE = 'http://athena-make.test';
  const RULE = [
    { name: 'prop-owl-proxy-allowed-hosts', propertyKey: 'owl.proxy.allowed_hosts', propertyValue: '127.0.0.1' },
    { name: 'prop-owl-proxy-port-range', propertyKey: 'owl.proxy.port_range', propertyValue: '1024-65535' },
  ];
  const reply = (body: unknown, status = 200) => new Response(JSON.stringify(body), { status });

  /** athena-make's two listings, and a principles-api answering wherever it is asked */
  function world(domains: unknown[], properties: unknown[]) {
    const asked: string[] = [];
    const fetchFn = async (url: RequestInfo | URL): Promise<Response> => {
      const u = String(url);
      asked.push(u);
      if (u === `${MAKE}/domains`) return reply({ data: domains });
      if (u.startsWith(`${MAKE}/properties/properties`)) return reply({ data: properties });
      if (u.endsWith('/principles/principles')) return reply({ data: [{ name: 'hemenway-observe' }] });
      if (u.includes('/principles/principles/')) return reply({ data: { label: 'Observe', order: '1' } });
      return reply({ error: 'not found' }, 404);
    };
    return { asked, fetchFn: fetchFn as unknown as typeof fetch };
  }

  const PRINCIPLES_DOMAIN = { name: 'principles', hasProperty: ['property-principles-server-bind'] };
  const bindRow = (bind: string) => ({ name: 'principles-server-bind', propertyKey: 'server.bind', propertyValue: bind });

  beforeEach(() => {
    resetGeneratedApisCache();
    setSkippedBindLog(() => { /* the refusals are asserted by status here; owl-bind-rule-5034 reads the lines */ });
  });
  afterAll(() => {
    setSkippedBindLog((line) => console.warn(line));
    resetGeneratedApisCache();
  });

  test('the bind in the principles Domain row is where the principles are read', async () => {
    const w = world([PRINCIPLES_DOMAIN], [...RULE, bindRow('127.0.0.1:4777')]);
    const r = await fetchLoomPrinciples({ fetchFn: w.fetchFn, owlBase: MAKE, ownPort: 3340 });
    expect(r.status).toBe(200);
    expect((r.body as { data: { principles: Array<{ id: string }> } }).data.principles.map((p) => p.id)).toEqual(['hemenway-observe']);
    expect(w.asked).toContain('http://127.0.0.1:4777/principles/principles');
    expect(w.asked.some((u) => u.includes(':3393'))).toBe(false);
  });

  test('NEGATIVE: no server.bind row for principles → 502 naming the cause, and no address is guessed', async () => {
    const w = world([{ name: 'principles', hasProperty: [] }], RULE);
    const r = await fetchLoomPrinciples({ fetchFn: w.fetchFn, owlBase: MAKE, ownPort: 3340 });
    expect(r.status).toBe(502);
    const body = r.body as { data: { error?: string; principles?: unknown[] }; _meta: { error?: boolean } };
    expect(body.data.error).toContain('the generated-API map has no principles entry');
    expect(body.data.principles).toBeUndefined();
    expect(body._meta.error).toBe(true);
    // only athena-make's two listings were asked: nothing went to a principles-api
    expect(w.asked.every((u) => u.startsWith(MAKE))).toBe(true);
  });

  test('NEGATIVE: PRINCIPLES_UPSTREAM is no longer a way in — set, it is ignored', async () => {
    process.env.PRINCIPLES_UPSTREAM = 'http://127.0.0.1:3393';
    try {
      const w = world([{ name: 'principles', hasProperty: [] }], RULE);
      const r = await fetchLoomPrinciples({ fetchFn: w.fetchFn, owlBase: MAKE, ownPort: 3340 });
      expect(r.status).toBe(502);
      expect(w.asked.some((u) => u.includes(':3393'))).toBe(false);
    } finally {
      delete process.env.PRINCIPLES_UPSTREAM;
    }
  });

  test('NEGATIVE: a bind the rule refuses, and an unreadable athena-make, are both 502 — never a fallback port', async () => {
    const w = world([PRINCIPLES_DOMAIN], [...RULE, bindRow('10.0.0.5:3393')]);
    const refused = await fetchLoomPrinciples({ fetchFn: w.fetchFn, owlBase: MAKE, ownPort: 3340 });
    expect(refused.status).toBe(502);
    expect(w.asked.every((u) => u.startsWith(MAKE))).toBe(true);
    resetGeneratedApisCache();
    const down = (async () => { throw new Error('ECONNREFUSED'); }) as unknown as typeof fetch;
    const unreadable = await fetchLoomPrinciples({ fetchFn: down, owlBase: MAKE, ownPort: 3340 });
    expect(unreadable.status).toBe(502);
    expect(JSON.stringify(unreadable.body)).toContain('could not be located');
  });
});
