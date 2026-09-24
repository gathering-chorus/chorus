// @test-type: integration — NEGATIVE PROOF (#3734) that a retired catch-all writer answers 410; starts the in-process app, writes nothing
/**
 * #4187 — NEGATIVE PROOF that the two discovery writers are RETIRED.
 *
 * POST /api/athena/discover-pages and /discover-endpoints scanned files on disk
 * and INSERTed Page / Endpoint rows into urn:chorus:instances — 448 + 15 rows
 * on 2026-09-24, none of them the crawler's (chorus-crawl writes
 * urn:chorus:domains:code with its own IRIs). The catch-all is retired, so the
 * routes answer 410 and name where the rows live. Restoring either route's
 * write body turns this red: the status would be 200 and the store would gain
 * rows from a test.
 *
 * No store write, no store gate: this suite must be RED, never UNMEASURED,
 * when a write path comes back.
 */
import { startTestApp, TestApp } from './lib/test-app';
import { withServiceAuth } from './lib/service-token';

withServiceAuth();

describe('#4187 — the discovery writers are retired', () => {
  let harness: TestApp;
  beforeAll(async () => { harness = await startTestApp(); });
  afterAll(async () => { if (harness) await harness.close(); });

  for (const [route, home] of [
    ['/api/athena/discover-pages', '/pages'],
    ['/api/athena/discover-endpoints', '/services'],
  ] as const) {
    test(`POST ${route} answers 410 and names the crawler's graph and the read route`, async () => {
      const res = await fetch(`${harness.baseUrl}${route}`, { method: 'POST' });
      expect(res.status).toBe(410);
      const body = await res.json();
      expect(body.data.error).toBe('retired');
      expect(body.data.message).toContain('urn:chorus:domains:code');
      expect(body.data.message).toContain(home);
      expect(body._meta.retired_by).toBe(4187);
    }, 30_000);
  }
});
