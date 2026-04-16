/**
 * Crawl API response shape tests — #1884
 *
 * Integration tests — hit live Chorus API at localhost:3340.
 * Validates the 16-key structure per domain so breaking changes
 * (removed/renamed keys) are caught before they degrade crawler snapshots.
 */

const INTEGRATION_ENABLED = process.env.RUN_INTEGRATION === 'true';
const API = process.env.CHORUS_API || 'http://localhost:3340';

const describeIntegration = INTEGRATION_ENABLED ? describe : describe.skip;

const EXPECTED_KEYS = [
  'alerts', 'cards', 'code', 'codeScan', 'count',
  'domain', 'history', 'infra', 'links', 'logs',
  'mentions', 'owl', 'rdf', 'related', 'spine', 'timeline',
];

const ARRAY_KEYS = ['alerts', 'cards', 'links', 'logs', 'mentions', 'related', 'spine', 'timeline'];
const OBJECT_KEYS = ['code', 'codeScan', 'history', 'infra', 'owl', 'rdf'];

const TEST_DOMAINS = ['seeds', 'music', 'blog'];

describeIntegration('Crawl API response shape (#1884)', () => {
  for (const domain of TEST_DOMAINS) {
    describe(`GET /api/chorus/crawl/${domain}`, () => {
      let body;

      beforeAll(async () => {
        const res = await fetch(`${API}/api/chorus/crawl/${domain}`);
        expect(res.status).toBe(200);
        body = await res.json();
      }, 60_000);

      test('returns all 16 keys', () => {
        const keys = Object.keys(body).sort();
        expect(keys).toEqual(EXPECTED_KEYS);
      });

      test('array keys are arrays', () => {
        for (const key of ARRAY_KEYS) {
          expect(Array.isArray(body[key])).toBe(true);
        }
      });

      test('object keys are objects', () => {
        for (const key of OBJECT_KEYS) {
          expect(typeof body[key]).toBe('object');
          expect(body[key]).not.toBeNull();
          expect(Array.isArray(body[key])).toBe(false);
        }
      });

      test('domain is a string matching requested domain', () => {
        expect(body.domain).toBe(domain);
      });

      test('count is a number', () => {
        expect(typeof body.count).toBe('number');
      });
    });
  }

  test('unknown domain returns 404, not 500', async () => {
    const res = await fetch(`${API}/api/chorus/crawl/nonexistent-test-domain`);
    expect(res.status).toBe(404);
    const body = await res.json();
    expect(body.error).toBeDefined();
  }, 20_000);
});
