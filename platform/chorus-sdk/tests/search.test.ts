import * as http from 'http';
import { search } from '../src/search';

describe('search', () => {
  let server: http.Server;
  const PORT = 13340;

  beforeAll(async () => {
    server = http.createServer((req, res) => {
      const url = new URL(req.url!, `http://localhost:${PORT}`);
      const q = url.searchParams.get('q') ?? '';
      res.writeHead(200, { 'Content-Type': 'application/json' });
      res.end(JSON.stringify({
        results: [
          { source: 'claude', role: 'silas', timestamp: '2026-03-07T12:00:00Z', content: `Match for: ${q}` },
        ],
        total: 1,
      }));
    });
    await new Promise<void>((resolve) => server.listen(PORT, () => resolve()));
  });

  afterAll(async () => {
    await new Promise<void>((resolve) => server.close(() => resolve()));
  });

  beforeEach(() => {
    process.env.CHORUS_API_URL = `http://localhost:${PORT}`;
    // Re-import to pick up env change
    jest.resetModules();
  });

  it('queries the Chorus API and returns results', async () => {
    const { search: s } = require('../src/search');
    const result = await s('spine events');

    expect(result.query).toBe('spine events');
    expect(result.total).toBe(1);
    expect(result.results).toHaveLength(1);
    expect(result.results[0].content).toContain('spine events');
  });

  it('respects limit parameter', async () => {
    const { search: s } = require('../src/search');
    const result = await s('test', 5);
    expect(result.results).toBeDefined();
  });
});
