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

  // #4292 — one literal name per case. A name built at run time
  // (`POST ${route} ...`) cannot be read by the crawler, so these cases had no
  // Test row and their results joined nothing (crawler-validate, #4290).
  const hit = async (route: string) => {
    const res = await fetch(`${harness.baseUrl}${route}`, { method: 'POST' });
    const body = await res.json();
    return { status: res.status, error: body.data.error, retiredBy: body._meta.retired_by, message: String(body.data.message) };
  };
  test("POST /api/athena/discover-pages answers 410 and names the crawler's graph and the read route", async () => {
    const r = await hit('/api/athena/discover-pages');
    expect(r).toMatchObject({ status: 410, error: 'retired', retiredBy: 4187 });
    expect(r.message).toContain('urn:chorus:domains:code');
    expect(r.message).toContain('/pages');
  }, 30_000);
  test("POST /api/athena/discover-endpoints answers 410 and names the crawler's graph and the read route", async () => {
    const r = await hit('/api/athena/discover-endpoints');
    expect(r).toMatchObject({ status: 410, error: 'retired', retiredBy: 4187 });
    expect(r.message).toContain('urn:chorus:domains:code');
    expect(r.message).toContain('/services');
  }, 30_000);
});
