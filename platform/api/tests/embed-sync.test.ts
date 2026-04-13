/**
 * Embed sync tests (#1920)
 * Integration — hit live Chorus API. Requires RUN_INTEGRATION=true.
 */

const INTEGRATION_ENABLED = process.env.RUN_INTEGRATION === 'true';
const API = process.env.CHORUS_API || 'http://localhost:3340';

let apiUp = false;

beforeAll(async () => {
  if (!INTEGRATION_ENABLED) return;
  try {
    const res = await fetch(`${API}/api/chorus/health`);
    apiUp = res.ok;
  } catch {
    apiUp = false;
  }
});

const describeIntegration = INTEGRATION_ENABLED ? describe : describe.skip;

describeIntegration('Embed sync — no in-process timer (#1978)', () => {
  test('API responds within 2s even when embed backlog exists', async () => {
    // If embed timer is running in-process, sequential Ollama calls (~1.2s each)
    // cause the API to become intermittently unavailable.
    // 10 rapid health checks should all complete within 2s each.
    const times = [];
    for (let i = 0; i < 10; i++) {
      const start = Date.now();
      const res = await fetch(`${API}/api/chorus/health`);
      const elapsed = Date.now() - start;
      times.push(elapsed);
      expect(res.ok).toBe(true);
      expect(elapsed).toBeLessThan(2000);
    }
    const maxTime = Math.max(...times);
    console.log(`[embed-timer-test] 10 health checks: max=${maxTime}ms, avg=${Math.round(times.reduce((a,b)=>a+b,0)/times.length)}ms`);
  });

  test('health endpoint exposes unembedded count for external worker', async () => {
    // The embed backlog is now drained by an external worker (chorus-embed-worker.sh),
    // not an in-process timer. The health endpoint must expose unembedded count
    // so deep-health and the worker can monitor drift.
    const res = await fetch(`${API}/api/chorus/health`);
    const data = await res.json();
    expect(typeof data.unembedded).toBe('number');
    expect(typeof data.vectors).toBe('number');
  });
});

describeIntegration('Health endpoint performance (#1978)', () => {
  test('health endpoint responds under 500ms consistently', async () => {
    // AC2: API health endpoint responds consistently under 500ms
    const times = [];
    for (let i = 0; i < 5; i++) {
      const start = Date.now();
      const res = await fetch(`${API}/api/chorus/health`);
      const elapsed = Date.now() - start;
      times.push(elapsed);
      expect(res.ok).toBe(true);
      expect(elapsed).toBeLessThan(500);
    }
    const maxTime = Math.max(...times);
    console.log(`[health-perf] 5 checks: max=${maxTime}ms, avg=${Math.round(times.reduce((a,b)=>a+b,0)/times.length)}ms`);
  });
});

describeIntegration('Embed sync (#1920)', () => {
  test('health endpoint reports vector drift', async () => {
    const res = await fetch(`${API}/api/chorus/health`);
    const data = await res.json();
    expect(data.db.rows).toBeDefined();
    expect(data.vectors).toBeDefined();
    const drift = data.db.rows - data.vectors;
    expect(typeof drift).toBe('number');
  });

  test('embed endpoint processes messages and returns count', async () => {
    const res = await fetch(`${API}/api/chorus/embed`, { method: 'POST' });
    const data = await res.json();
    expect(data.embedded).toBeDefined();
    expect(data.skipped).toBeDefined();
    expect(typeof data.embedded).toBe('number');
  });

  test('embed reduces drift toward zero', async () => {
    const before = await fetch(`${API}/api/chorus/health`).then(r => r.json());
    const driftBefore = before.db.rows - before.vectors;

    const embedRes = await fetch(`${API}/api/chorus/embed`, { method: 'POST' }).then(r => r.json());

    const after = await fetch(`${API}/api/chorus/health`).then(r => r.json());
    const driftAfter = after.db.rows - after.vectors;

    if (embedRes.embedded > 0) {
      expect(driftAfter).toBeLessThan(driftBefore);
    } else {
      expect(driftAfter).toBeLessThanOrEqual(driftBefore);
    }
  });
});
