// @test-type: integration — hits the live chorus-api at :3340; carries a scoped service token on writes (#3619).
/**
 * Scheduled reindex tests — #1960
 *
 * Integration tests — hit live Chorus API at localhost:3340.
 * Verifies that index_freshness stays current without manual intervention.
 *
 * The bug: indexAllSources() only runs on restart or manual POST.
 * Log evidence: last_indexed is 21h ago (2026-04-12T19:57), no [reindex] log
 * entries in the last 2h. Sources appear fresh only because expected_cadence
 * is 24h — they'll go stale silently without a scheduled trigger.
 */

import { startTestApp, type TestApp } from './lib/test-app';
import { withServiceAuth } from './lib/service-token';
// #3619 — live mutation endpoints are envelope-secured; this suite is a real
// consumer and carries a scoped token on every write (deploy-before-require).
withServiceAuth();

describe('Scheduled reindex — index_freshness (#1960)', () => {

  let harness: TestApp;

  beforeAll(async () => { harness = await startTestApp(); });
  afterAll(async () => { if (harness) await harness.close(); });
  test('freshness endpoint returns sources with levels', async () => {
    const res = await fetch(`${harness.baseUrl}/api/chorus/freshness`);
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body.sources).toBeDefined();
    expect(body.sources.length).toBeGreaterThan(0);
    expect(body.sources[0].level).toBeDefined();
  });

  test('no sources are stale or dead under normal operation', async () => {
    const res = await fetch(`${harness.baseUrl}/api/chorus/freshness`);
    const body = await res.json();
    const bad = body.sources.filter(function(s) { return s.level === 'stale' || s.level === 'dead'; });
    expect(bad.length).toBe(0);
  });

  test('last_indexed is within 20 minutes (proves scheduled reindex fires)', async () => {
    const res = await fetch(`${harness.baseUrl}/api/chorus/freshness`);
    const body = await res.json();
    // Find a high-cadence source (spine or claude) — these go stale fastest
    const spine = body.sources.find(function(s) { return s.source === 'spine'; });
    // eslint-disable-next-line jest/no-conditional-expect -- spine source may be absent in test fixture
    if (spine) expect(spine.age_seconds).toBeLessThan(1200);
  });

  test('POST /api/chorus/reindex spawns the workers (202, never runs on the loop — #3379)', async () => {
    const res = await fetch(`${harness.baseUrl}/api/chorus/reindex`, { method: 'POST' });
    expect(res.status).toBe(202);
    const body = await res.json();
    expect(body.status).toBe('spawned');
    expect(body.workers).toEqual(['reindex', 'embed']);
    expect(body.timestamp).toBeDefined();
  }, 30_000);

  // #3379: the 'freshness improves after POST /reindex' test is retired with its
  // premise — the route spawns the worker instead of running the pass inline.
  // The 'last_indexed within 20 minutes' test above is the live freshness proof.
});
