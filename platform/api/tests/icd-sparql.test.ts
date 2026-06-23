// @test-type: unit — injects a fake fetch; no Fuseki, no live service, brings its own world.
import { createIcdSparqlClient, createIcdDomainResolver, fusekiWriteAuthFromEnv } from '../src/icd-sparql';

function okRes(body: any): Response {
  return {
    ok: true,
    status: 200,
    json: async () => body,
    text: async () => JSON.stringify(body),
  } as unknown as Response;
}

function errRes(status: number, body: string = ''): Response {
  return {
    ok: false,
    status,
    json: async () => ({}),
    text: async () => body,
  } as unknown as Response;
}

describe('createIcdSparqlClient', () => {
  it('GETs the query URL with encoded query parameter', async () => {
    const fetchFn = jest.fn(async () => okRes({ results: { bindings: [] } }));
    const client = createIcdSparqlClient({ queryUrl: 'http://fuseki/q', updateUrl: 'http://fuseki/u', fetchFn });
    await client.query('SELECT * WHERE { ?s ?p ?o }');
    const [url, init] = fetchFn.mock.calls[0];
    expect(String(url).startsWith('http://fuseki/q?query=')).toBe(true);
    expect((init as any).headers.Accept).toBe('application/sparql-results+json');
  });

  it('returns parsed JSON on a successful query', async () => {
    const fetchFn = jest.fn(async () => okRes({ results: { bindings: [{ x: { value: '1' } }] } }));
    const client = createIcdSparqlClient({ queryUrl: 'http://x', updateUrl: 'http://y', fetchFn });
    const r = await client.query('ASK { ?s ?p ?o }');
    expect(r.results.bindings).toHaveLength(1);
  });

  it('throws with status code on non-ok query', async () => {
    const fetchFn = jest.fn(async () => errRes(500));
    const client = createIcdSparqlClient({ queryUrl: 'http://x', updateUrl: 'http://y', fetchFn });
    await expect(client.query('x')).rejects.toThrow(/SPARQL query failed: 500/);
  });

  it('POSTs to updateUrl with sparql-update content type', async () => {
    const fetchFn = jest.fn(async () => okRes({}));
    const client = createIcdSparqlClient({ queryUrl: 'http://x', updateUrl: 'http://u', fetchFn });
    await client.update('INSERT DATA { <a> <b> <c> }');
    const [url, init] = fetchFn.mock.calls[0];
    expect(url).toBe('http://u');
    expect((init as any).method).toBe('POST');
    expect((init as any).headers['Content-Type']).toBe('application/sparql-update');
  });

  // #3566 LOCK — the write door carries the credential; reads stay open.
  it('adds Authorization: Basic to update() when auth is provided', async () => {
    const fetchFn = jest.fn(async () => okRes({}));
    const client = createIcdSparqlClient({ queryUrl: 'http://x', updateUrl: 'http://u', fetchFn, auth: { user: 'admin', password: 'secret' } });
    await client.update('INSERT DATA { <a> <b> <c> }');
    const [, init] = fetchFn.mock.calls[0];
    expect((init as any).headers['Authorization']).toBe('Basic ' + Buffer.from('admin:secret').toString('base64'));
  });

  it('does NOT add Authorization to update() when no auth (current behavior preserved)', async () => {
    const fetchFn = jest.fn(async () => okRes({}));
    const client = createIcdSparqlClient({ queryUrl: 'http://x', updateUrl: 'http://u', fetchFn });
    await client.update('INSERT DATA { <a> <b> <c> }');
    const [, init] = fetchFn.mock.calls[0];
    expect((init as any).headers['Authorization']).toBeUndefined();
  });

  it('does NOT add Authorization to query() even when auth is set (reads stay open)', async () => {
    const fetchFn = jest.fn(async () => okRes({ results: { bindings: [] } }));
    const client = createIcdSparqlClient({ queryUrl: 'http://x', updateUrl: 'http://u', fetchFn, auth: { user: 'admin', password: 'secret' } });
    await client.query('ASK { ?s ?p ?o }');
    const [, init] = fetchFn.mock.calls[0];
    expect((init as any)?.headers?.Authorization).toBeUndefined();
  });

  it('throws with status + body on non-ok update', async () => {
    const fetchFn = jest.fn(async () => errRes(400, 'malformed'));
    const client = createIcdSparqlClient({ queryUrl: 'http://x', updateUrl: 'http://u', fetchFn });
    await expect(client.update('DELETE { ?s ?p ?o }')).rejects.toThrow(/SPARQL update failed: 400/);
  });
});

describe('createIcdDomainResolver', () => {
  it('returns the URI from the first binding when domainId matches', async () => {
    const client = {
      query: jest.fn(async () => ({
        results: { bindings: [{ d: { value: 'urn:domain:chorus' } }] },
      })),
      update: jest.fn(),
    };
    const resolve = createIcdDomainResolver({ client, pfx: 'PREFIX icd:', graph: 'urn:icd:current' });
    const uri = await resolve('chorus');
    expect(uri).toBe('urn:domain:chorus');
    expect(client.query).toHaveBeenCalledTimes(1);
  });

  it('falls back to domainName lookup when first query has no hits', async () => {
    let call = 0;
    const client = {
      query: jest.fn(async () => {
        call++;
        if (call === 1) return { results: { bindings: [] } };
        return { results: { bindings: [{ d: { value: 'urn:domain:match-by-name' } }] } };
      }),
      update: jest.fn(),
    };
    const resolve = createIcdDomainResolver({ client, pfx: 'p', graph: 'g' });
    const uri = await resolve('Chorus');
    expect(uri).toBe('urn:domain:match-by-name');
    expect(client.query).toHaveBeenCalledTimes(2);
  });

  it('returns null when both queries miss', async () => {
    const client = {
      query: jest.fn(async () => ({ results: { bindings: [] } })),
      update: jest.fn(),
    };
    const resolve = createIcdDomainResolver({ client, pfx: 'p', graph: 'g' });
    expect(await resolve('missing')).toBeNull();
    expect(client.query).toHaveBeenCalledTimes(2);
  });

  it('passes prefix + graph into the query text', async () => {
    const client = {
      query: jest.fn(async () => ({ results: { bindings: [] } })),
      update: jest.fn(),
    };
    const resolve = createIcdDomainResolver({ client, pfx: 'PFX_TOKEN', graph: 'GRAPH_TOKEN' });
    await resolve('anything');
    const firstQ = client.query.mock.calls[0][0] as string;
    expect(firstQ).toContain('PFX_TOKEN');
    expect(firstQ).toContain('GRAPH_TOKEN');
  });

  it('lowercases the domainId in the name-match fallback', async () => {
    const client = {
      query: jest.fn(async () => ({ results: { bindings: [] } })),
      update: jest.fn(),
    };
    const resolve = createIcdDomainResolver({ client, pfx: 'p', graph: 'g' });
    await resolve('MixedCase');
    const secondQ = client.query.mock.calls[1][0] as string;
    expect(secondQ).toContain('"mixedcase"');
  });
});

// #3566 LOCK — the env helper that activates the write credential at the composition root.
describe('fusekiWriteAuthFromEnv', () => {
  it('returns undefined when FUSEKI_ADMIN_PASSWORD is unset (writers stay unauthenticated)', () => {
    expect(fusekiWriteAuthFromEnv({} as NodeJS.ProcessEnv)).toBeUndefined();
  });

  it('returns {admin, password} when FUSEKI_ADMIN_PASSWORD is set', () => {
    expect(fusekiWriteAuthFromEnv({ FUSEKI_ADMIN_PASSWORD: 'sekret' } as NodeJS.ProcessEnv)).toEqual({ user: 'admin', password: 'sekret' });
  });

  it('honors FUSEKI_ADMIN_USER override', () => {
    expect(fusekiWriteAuthFromEnv({ FUSEKI_ADMIN_PASSWORD: 'p', FUSEKI_ADMIN_USER: 'chorus-writer' } as NodeJS.ProcessEnv))
      .toEqual({ user: 'chorus-writer', password: 'p' });
  });
});
