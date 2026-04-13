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

const INTEGRATION_ENABLED = process.env.RUN_INTEGRATION === 'true';
const API = process.env.CHORUS_API || 'http://localhost:3340';

const describeIntegration = INTEGRATION_ENABLED ? describe : describe.skip;

describeIntegration('Scheduled reindex — index_freshness (#1960)', () => {
  test('freshness endpoint returns sources with levels', async () => {
    const res = await fetch(`${API}/api/chorus/freshness`);
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body.sources).toBeDefined();
    expect(body.sources.length).toBeGreaterThan(0);
    expect(body.sources[0].level).toBeDefined();
  });

  test('no sources are stale or dead under normal operation', async () => {
    const res = await fetch(`${API}/api/chorus/freshness`);
    const body = await res.json();
    const bad = body.sources.filter(function(s) { return s.level === 'stale' || s.level === 'dead'; });
    expect(bad.length).toBe(0);
  });

  test('last_indexed is within 20 minutes (proves scheduled reindex fires)', async () => {
    const res = await fetch(`${API}/api/chorus/freshness`);
    const body = await res.json();
    // Find a high-cadence source (spine or claude) — these go stale fastest
    const spine = body.sources.find(function(s) { return s.source === 'spine'; });
    if (spine) {
      // age_seconds should be under 20 min (1200s) if the 15-min timer is running
      expect(spine.age_seconds).toBeLessThan(1200);
    }
  });

  test('POST /api/chorus/reindex succeeds (manual trigger still works)', async () => {
    const res = await fetch(`${API}/api/chorus/reindex`, { method: 'POST' });
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body.status).toBe('ok');
    expect(body.timestamp).toBeDefined();
  }, 30_000);

  test('after reindex, spine and claude sources have staleness_ratio <= 1', async () => {
    await fetch(`${API}/api/chorus/reindex`, { method: 'POST' });
    const res = await fetch(`${API}/api/chorus/freshness`);
    const body = await res.json();
    const critical = body.sources.filter(function(s) {
      return s.source === 'spine' || s.source === 'claude';
    });
    for (const src of critical) {
      expect(src.staleness_ratio).toBeLessThanOrEqual(1.0);
    }
  }, 30_000);
});
