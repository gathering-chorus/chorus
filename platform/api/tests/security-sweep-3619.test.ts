// @test-type: unit — pure sweep logic (route parsing + gap classification) on
// string/array fixtures; no live chorus-api, no fuseki, no filesystem beyond
// the fixtures declared here.
/**
 * #3619 AC4 — the sweep test's core: the done-gate that proves "zero
 * unauthenticated mutation endpoints" instead of us tracking it by hand.
 *
 * Jeff's experience under test: when the last class flips, the sweep says
 * DONE and means it; until then every unflipped endpoint is either covered
 * by a secured surface, or sits in the exemptions ledger with a reason and
 * a card — and a NEW mutation endpoint added without either turns the sweep
 * red (the ratchet). The live prober (platform/scripts/security-sweep) walks
 * exactly this classification against the running system.
 */
import {
  parseMutationRoutes,
  classifyEndpoints,
  type SweepExemption,
} from '../src/security-sweep';
import type { SecuredSurface } from '../src/security-envelope';

const SERVER_FIXTURE = `
app.get('/api/chorus/health', (_req, res) => res.json({ ok: true }));
app.post('/api/cards/add', async (req: Request, res: Response) => {});
app.post('/api/chorus/reindex', (_req: Request, res: Response) => {});
app.put('/api/icd/domains/:id/fields', async (req, res) => {});
app.delete('/api/athena/subdomains/:id', async (req, res) => {});
app.patch('/api/athena/subdomains/:id/meta', async (req, res) => {});
// app.post('/api/commented/out', ...) — must not count
`;

describe('parseMutationRoutes', () => {
  test('finds post/put/delete/patch and skips get', () => {
    const routes = parseMutationRoutes(SERVER_FIXTURE);
    const paths = routes.map(r => `${r.method} ${r.path}`);
    expect(paths).toContain('POST /api/cards/add');
    expect(paths).toContain('POST /api/chorus/reindex');
    expect(paths).toContain('PUT /api/icd/domains/:id/fields');
    expect(paths).toContain('DELETE /api/athena/subdomains/:id');
    expect(paths).toContain('PATCH /api/athena/subdomains/:id/meta');
    expect(paths.find(p => p.startsWith('GET'))).toBeUndefined();
  });

  test('ignores commented-out routes', () => {
    const routes = parseMutationRoutes(SERVER_FIXTURE);
    expect(routes.map(r => r.path)).not.toContain('/api/commented/out');
  });

  test('parses the real 64-endpoint shape (smoke on count stability)', () => {
    // Three routes on one line each, varied whitespace/quotes.
    const src = [
      `app.post("/api/a", h)`,
      `app.put( '/api/b' , h)`,
      `app.delete('/api/c',h)`,
    ].join('\n');
    expect(parseMutationRoutes(src)).toHaveLength(3);
  });
});

describe('classifyEndpoints', () => {
  const surfaces: SecuredSurface[] = [
    { method: 'POST', pathPrefix: '/api/chorus/reindex', requiresScope: 'urn:chorus:index', surface: 's-reindex' },
    { method: '*', pathPrefix: '/api/icd/', requiresScope: 'urn:chorus:icd', surface: 's-icd' },
  ];
  const exemptions: SweepExemption[] = [
    { method: 'POST', path: '/api/cards/add', reason: 'Class A held — Wren round-trip check', card: 3619 },
  ];
  const routes = parseMutationRoutes(SERVER_FIXTURE);

  test('secured-surface prefix match covers the endpoint', () => {
    const gap = classifyEndpoints(routes, surfaces, exemptions);
    expect(gap.secured.map(r => r.path)).toContain('/api/chorus/reindex');
    expect(gap.secured.map(r => r.path)).toContain('/api/icd/domains/:id/fields');
  });

  test('exempted endpoint is neither secured nor unprotected', () => {
    const gap = classifyEndpoints(routes, surfaces, exemptions);
    expect(gap.exempted.map(r => r.path)).toContain('/api/cards/add');
    expect(gap.unprotected.map(r => r.path)).not.toContain('/api/cards/add');
  });

  test('everything else is unprotected — the ratchet catches new endpoints', () => {
    const gap = classifyEndpoints(routes, surfaces, exemptions);
    const paths = gap.unprotected.map(r => r.path);
    expect(paths).toContain('/api/athena/subdomains/:id');
    expect(paths).toContain('/api/athena/subdomains/:id/meta');
  });

  test('done-state: no exemptions + full surface coverage → unprotected and exempted both empty', () => {
    const all: SecuredSurface[] = [
      { method: '*', pathPrefix: '/api/', requiresScope: 'urn:chorus:ops', surface: 's-all' },
    ];
    const gap = classifyEndpoints(routes, all, []);
    expect(gap.unprotected).toHaveLength(0);
    expect(gap.exempted).toHaveLength(0);
    expect(gap.secured.length).toBe(routes.length);
  });

  test('method-specific surface does not cover other methods on the same prefix', () => {
    const gap = classifyEndpoints(
      parseMutationRoutes(`app.post('/api/x', h)\napp.delete('/api/x', h)`),
      [{ method: 'POST', pathPrefix: '/api/x', requiresScope: 'u', surface: 's-x' }],
      [],
    );
    expect(gap.secured.map(r => r.method)).toEqual(['POST']);
    expect(gap.unprotected.map(r => r.method)).toEqual(['DELETE']);
  });

  test('stale exemption (no matching route) is reported so the ledger cannot rot', () => {
    const gap = classifyEndpoints(routes, surfaces, [
      { method: 'POST', path: '/api/retired/endpoint', reason: 'gone', card: 3619 },
    ]);
    expect(gap.staleExemptions.map(e => e.path)).toContain('/api/retired/endpoint');
  });
});
