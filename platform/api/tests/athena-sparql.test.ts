import { createAthenaSparqlClient, createEnvelopeBuilder, createSparqlLoader } from '../src/athena-sparql';

function okRes(body: any): Response {
  return {
    ok: true,
    status: 200,
    json: async () => body,
    text: async () => JSON.stringify(body),
  } as unknown as Response;
}

function errRes(status: number, text: string): Response {
  return {
    ok: false,
    status,
    json: async () => ({}),
    text: async () => text,
  } as unknown as Response;
}

describe('createAthenaSparqlClient', () => {
  it('posts a SPARQL query to the sparqlUrl and returns parsed JSON', async () => {
    const fetchFn = jest.fn(async () => okRes({ results: { bindings: [] } }));
    const client = createAthenaSparqlClient({ sparqlUrl: 'http://fuseki/q', updateUrl: 'http://fuseki/u', fetchFn });
    const r = await client.query('SELECT * WHERE { ?s ?p ?o }');
    expect(r).toEqual({ results: { bindings: [] } });
    expect(fetchFn).toHaveBeenCalledTimes(1);
    const [url, init] = fetchFn.mock.calls[0];
    expect(url).toBe('http://fuseki/q');
    expect((init as RequestInit).method).toBe('POST');
    expect((init as any).headers['Content-Type']).toBe('application/sparql-query');
  });

  it('throws on non-ok query with status and body prefix', async () => {
    const fetchFn = jest.fn(async () => errRes(400, 'bad query: unexpected token xyz'));
    const client = createAthenaSparqlClient({ sparqlUrl: 'http://x', updateUrl: 'http://y', fetchFn });
    await expect(client.query('nonsense')).rejects.toThrow(/Fuseki 400/);
  });

  it('posts a SPARQL update to the updateUrl with correct content-type', async () => {
    const fetchFn = jest.fn(async () => okRes({}));
    const client = createAthenaSparqlClient({ sparqlUrl: 'http://q', updateUrl: 'http://u', fetchFn });
    await client.update('INSERT DATA { <a> <b> <c> }');
    const [url, init] = fetchFn.mock.calls[0];
    expect(url).toBe('http://u');
    expect((init as any).headers['Content-Type']).toBe('application/sparql-update');
    expect((init as RequestInit).body).toBe('INSERT DATA { <a> <b> <c> }');
  });

  it('throws on non-ok update', async () => {
    const fetchFn = jest.fn(async () => errRes(500, 'graph locked'));
    const client = createAthenaSparqlClient({ sparqlUrl: 'http://q', updateUrl: 'http://u', fetchFn });
    await expect(client.update('DELETE {?s ?p ?o}')).rejects.toThrow(/Fuseki update 500/);
  });
});

describe('createEnvelopeBuilder', () => {
  it('wraps data in a _meta + data envelope', () => {
    const build = createEnvelopeBuilder({ graph: 'urn:test', now: () => '2026-04-18 10:00:00' });
    const env = build('my-query', { count: 3 }, 42);
    expect(env.data).toEqual({ count: 3 });
    expect(env._meta.source).toBe('athena');
    expect(env._meta.query_name).toBe('my-query');
    expect(env._meta.graph).toBe('urn:test');
    expect(env._meta.duration_ms).toBe(42);
    expect(env._meta.cached).toBe(false);
    expect(env._meta.timestamp).toBe('2026-04-18 10:00:00');
  });

  it('merges extra fields into _meta', () => {
    const build = createEnvelopeBuilder({ graph: 'g', now: () => 't' });
    const env = build('q', null, 1, { custom: 'value', cached: true });
    expect(env._meta.custom).toBe('value');
    expect(env._meta.cached).toBe(true);
  });

  it('preserves null data without wrapping', () => {
    const build = createEnvelopeBuilder({ graph: 'g', now: () => 't' });
    const env = build('q', null, 0);
    expect(env.data).toBeNull();
  });
});

describe('createSparqlLoader', () => {
  it('reads a .sparql file from the configured dir and trims whitespace', () => {
    const fakeFs = {
      readFileSync: jest.fn(() => '  SELECT * WHERE { ?s ?p ?o }  \n'),
    };
    const load = createSparqlLoader({ fs: fakeFs as any, sparqlDir: '/tmp/sparql' });
    const q = load('my-query');
    expect(q).toBe('SELECT * WHERE { ?s ?p ?o }');
    expect(fakeFs.readFileSync).toHaveBeenCalledWith('/tmp/sparql/my-query.sparql', 'utf-8');
  });

  it('lets filesystem errors propagate (no defensive swallow)', () => {
    const fakeFs = {
      readFileSync: jest.fn(() => { throw new Error('ENOENT'); }),
    };
    const load = createSparqlLoader({ fs: fakeFs as any, sparqlDir: '/x' });
    expect(() => load('missing')).toThrow(/ENOENT/);
  });
});
