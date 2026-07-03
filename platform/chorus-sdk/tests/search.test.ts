// @test-type: unit — spins its own http server on port 0; no live chorus-api, brings its own world.
//
// #3606 — search.ts and index.ts were 0% covered, dragging the sdk below both
// its own jest gate (functions 75%) and the Jeff-floor (statements 85 in
// coverage-floors.yml) — the nightly's "chorus-sdk coverage run errored rc=1".
// Real contract tests: search() against a real (local) HTTP server exercising
// shape normalization and both error paths.
import * as http from 'http';
import type { AddressInfo } from 'net';
import { search } from '../src/search';
import * as sdk from '../src/index';

describe('search (sdk client)', () => {
  let server: http.Server;
  let handler: (req: http.IncomingMessage, res: http.ServerResponse) => void;
  let prevUrl: string | undefined;

  beforeAll(async () => {
    prevUrl = process.env.CHORUS_API_URL;
    server = http.createServer((req, res) => handler(req, res));
    await new Promise<void>((resolve) => server.listen(0, '127.0.0.1', resolve));
    const { port } = server.address() as AddressInfo;
    process.env.CHORUS_API_URL = `http://127.0.0.1:${port}`;
  });

  afterAll(async () => {
    if (prevUrl === undefined) delete process.env.CHORUS_API_URL;
    else process.env.CHORUS_API_URL = prevUrl;
    await new Promise<void>((resolve) => server.close(() => resolve()));
  });

  it('returns results/total/query from a {results,total} response', async () => {
    let seenUrl = '';
    handler = (req, res) => {
      seenUrl = req.url ?? '';
      res.setHeader('Content-Type', 'application/json');
      res.end(JSON.stringify({ results: [{ source: 'claude', role: 'kade', timestamp: 't', content: 'c' }], total: 1 }));
    };
    const r = await search('hello world', 5);
    expect(seenUrl).toBe('/api/chorus/search?q=hello%20world&limit=5');
    expect(r.total).toBe(1);
    expect(r.results).toHaveLength(1);
    expect(r.results[0].role).toBe('kade');
    expect(r.query).toBe('hello world');
  });

  it('normalizes the legacy {messages,count} shape', async () => {
    handler = (_req, res) => {
      res.end(JSON.stringify({ messages: [{ source: 's', role: 'wren', timestamp: 't', content: 'm' }], count: 7 }));
    };
    const r = await search('term');
    expect(r.total).toBe(7);
    expect(r.results).toHaveLength(1);
  });

  it('defaults limit to 20 and encodes the query', async () => {
    let seenUrl = '';
    handler = (req, res) => { seenUrl = req.url ?? ''; res.end('{"results":[],"total":0}'); };
    const r = await search('a&b=c');
    expect(r.results).toEqual([]);
    expect(seenUrl).toBe('/api/chorus/search?q=a%26b%3Dc&limit=20');
  });

  it('rejects with a parse error on non-JSON body', async () => {
    handler = (_req, res) => { res.end('<html>gateway error</html>'); };
    await expect(search('x')).rejects.toThrow(/Failed to parse Chorus API response/);
  });

  it('rejects with a request error when the server is unreachable', async () => {
    // Dead port: nothing listens on port 1.
    const prev = process.env.CHORUS_API_URL;
    process.env.CHORUS_API_URL = 'http://127.0.0.1:1';
    try {
      await expect(search('x')).rejects.toThrow(/Chorus API request failed/);
    } finally {
      process.env.CHORUS_API_URL = prev;
    }
  });
});

describe('index (sdk barrel)', () => {
  it('exports the full public surface', () => {
    expect(typeof sdk.emit).toBe('function');
    expect(typeof sdk.createSpineContext).toBe('function');
    expect(typeof sdk.search).toBe('function');
    expect(typeof sdk.subscribe).toBe('function');
  });
});
