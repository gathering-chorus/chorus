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
