// @test-type: unit — hermetic. Brings its own owl-api stub (http.Server on an
// ephemeral port) + tempdir cache; no live :3360, no $HOME.
/**
 * #3657 — quality summary is a projection of the tests domain.
 *
 * The summary must be built from the owl-api V2 /tests collection (paginated),
 * not from a filesystem scan. These tests pin:
 *   - pagination follows links.next, including the /v1/tests → /tests path
 *     rewrite (owl-api self-links carry a /v1 prefix its router doesn't serve)
 *   - pyramid aggregates by the model's pyramidLayer vocabulary
 *   - hermeticity counts surface per layer
 *   - per-domain filter reads the model's `covers`, not a filename heuristic
 *   - cache honors TTL and QUALITY_CACHE_PATH
 *   - source block declares the projection (kind, api, generatedFrom)
 */

import * as http from 'http';
import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';
import { runQualityScan, getQualityScan, getQualityByDomain } from '../src/quality-summary';

interface StubRow {
  name: string;
  filePath: string;
  pyramidLayer: string;
  hermeticity: string;
  covers: string;
  testName: string;
  inFile: string;
}

const ROWS: StubRow[] = [
  { name: 't1', filePath: 'platform/api/tests/a.test.ts', pyramidLayer: 'unit', hermeticity: 'hermetic', covers: 'senses', testName: 'a1', inFile: 'sf-a' },
  { name: 't2', filePath: 'platform/api/tests/a.test.ts', pyramidLayer: 'unit', hermeticity: 'hermetic', covers: 'senses', testName: 'a2', inFile: 'sf-a' },
  { name: 't3', filePath: 'platform/api/tests/b.test.ts', pyramidLayer: 'integration', hermeticity: 'needs-stack', covers: 'cicd', testName: 'b1', inFile: 'sf-b' },
  { name: 't4', filePath: 'platform/tests/c.feature', pyramidLayer: 'bdd', hermeticity: 'hermetic', covers: 'cicd', testName: 'c1', inFile: 'sf-c' },
  { name: 't5', filePath: 'e2e/d.spec.ts', pyramidLayer: 'e2e', hermeticity: 'needs-stack', covers: 'senses', testName: 'd1', inFile: 'sf-d' },
];

const GENERATED_FROM = {
  graph: 'urn:chorus:ontology',
  shape: 'chorus:TestShape',
  shapeVersion: 'stub-shape-1',
  commit: 'stub-commit-1',
};

let server: http.Server;
let baseUrl: string;
let requestedPaths: string[];
let tmpDir: string;

beforeAll(async () => {
  tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'quality-summary-test-'));
  requestedPaths = [];
  server = http.createServer((req, res) => {
    requestedPaths.push(req.url || '');
    const url = new URL(req.url || '/', 'http://localhost');
    if (!url.pathname.startsWith('/tests')) {
      res.writeHead(404).end();
      return;
    }
    const cursor = Number(url.searchParams.get('cursor') || '0');
    const limit = Number(url.searchParams.get('limit') || '100');
    const page = ROWS.slice(cursor, cursor + Math.min(limit, 2)); // force pagination
    const nextCursor = cursor + page.length;
    const body = {
      apiVersion: 'v1',
      kind: 'Test',
      self: '/v1/tests',
      generatedFrom: GENERATED_FROM,
      data: page,
      // Real owl-api emits /v1/tests here although only /tests is routed —
      // the client must rewrite or pagination dies on page 2.
      links: { next: nextCursor < ROWS.length ? `/v1/tests?cursor=${nextCursor}&limit=${limit}` : null },
      count: ROWS.length,
    };
    res.writeHead(200, { 'content-type': 'application/json' }).end(JSON.stringify(body));
  });
  await new Promise<void>((resolve) => server.listen(0, resolve));
  const addr = server.address();
  baseUrl = `http://localhost:${(addr as { port: number }).port}`;
  process.env.OWL_API_BASE = baseUrl;
  process.env.QUALITY_CACHE_PATH = path.join(tmpDir, 'quality-cache.json');
});

afterAll(async () => {
  await new Promise<void>((resolve) => server.close(() => resolve()));
  delete process.env.OWL_API_BASE;
  delete process.env.QUALITY_CACHE_PATH;
  fs.rmSync(tmpDir, { recursive: true, force: true });
});

beforeEach(() => {
  requestedPaths.length = 0;
  fs.rmSync(path.join(tmpDir, 'quality-cache.json'), { force: true });
});

describe('#3657: runQualityScan projects the tests domain', () => {
  test('total equals the collection count, layers use the model vocabulary', async () => {
    const scan = await runQualityScan();
    expect(scan.total).toBe(5);
    const keys = scan.pyramid.map((l) => l.key);
    expect(keys).toEqual(['e2e', 'bdd', 'integration', 'unit']);
    const unit = scan.pyramid.find((l) => l.key === 'unit')!;
    expect(unit.count).toBe(2);
    expect(unit.fileCount).toBe(1); // t1+t2 share a file
  });

  test('pagination follows links.next with the /v1 path rewrite', async () => {
    await runQualityScan();
    expect(requestedPaths.length).toBeGreaterThan(1); // stub pages at 2 rows
    for (const p of requestedPaths) {
      expect(p.startsWith('/tests')).toBe(true); // never the broken /v1/tests
    }
  });

  test('hermeticity counts surface per layer', async () => {
    const scan = await runQualityScan();
    const unit = scan.pyramid.find((l) => l.key === 'unit')!;
    expect(unit.hermeticCount).toBe(2);
    expect(unit.needsStackCount).toBe(0);
    const integration = scan.pyramid.find((l) => l.key === 'integration')!;
    expect(integration.needsStackCount).toBe(1);
  });

  test('repos declare the domain scope honestly — chorus only, no fabricated Gathering rollup', async () => {
    const scan = await runQualityScan();
    expect(scan.repos.map((r) => r.name)).toEqual(['Chorus Platform']);
    expect(scan.repos[0].total).toBe(5);
  });

  test('source block names the projection', async () => {
    const scan = await runQualityScan();
    expect(scan.source.kind).toBe('tests-domain');
    expect(scan.source.api).toContain('/tests');
    expect(scan.source.generatedFrom).toEqual(GENERATED_FROM);
  });

  test('files carry the model covers as domain', async () => {
    const scan = await runQualityScan();
    const unit = scan.pyramid.find((l) => l.key === 'unit')!;
    expect(unit.files[0]).toMatchObject({ name: 'platform/api/tests/a.test.ts', count: 2, domain: 'senses' });
  });
});

describe('#3657: getQualityByDomain filters on the model covers', () => {
  test('senses returns only senses-covering tests', async () => {
    const byDomain = await getQualityByDomain('senses');
    expect(byDomain.domain).toBe('senses');
    expect(byDomain.total).toBe(3); // t1, t2 (unit) + t5 (e2e)
    const layerKeys = byDomain.layers.map((l) => l.key);
    expect(layerKeys).toContain('unit');
    expect(layerKeys).toContain('e2e');
    expect(layerKeys).not.toContain('bdd');
  });

  test('unknown domain returns empty, not an error', async () => {
    const byDomain = await getQualityByDomain('no-such-domain');
    expect(byDomain.total).toBe(0);
    expect(byDomain.layers).toEqual([]);
  });
});

describe('#3657: cache', () => {
  test('second getQualityScan within TTL serves the cache (no second fetch)', async () => {
    await getQualityScan();
    const fetchesAfterFirst = requestedPaths.length;
    await getQualityScan();
    expect(requestedPaths.length).toBe(fetchesAfterFirst);
  });

  test('cache file lands at QUALITY_CACHE_PATH', async () => {
    await getQualityScan();
    expect(fs.existsSync(path.join(tmpDir, 'quality-cache.json'))).toBe(true);
  });
});
