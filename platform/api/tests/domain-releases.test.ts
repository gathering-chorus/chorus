/**
 * Domain release history — #1910
 *
 * Domain-scoped deploy history from git log + card domain tags.
 * Each release has card ID, title, commit, timestamp, role.
 */

const INTEGRATION_ENABLED = process.env.RUN_INTEGRATION === 'true';
const API = process.env.CHORUS_API || 'http://localhost:3340';

const describeIntegration = INTEGRATION_ENABLED ? describe : describe.skip;

describeIntegration('#1910: domain release history', () => {

  test('GET /api/chorus/domain/:name/releases returns release list', async () => {
    const res = await fetch(`${API}/api/chorus/domain/chorus/releases`);
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body.data).toBeDefined();
    expect(Array.isArray(body.data.releases)).toBe(true);
    expect(body.data.releases.length).toBeGreaterThan(0);
  }, 15_000);

  test('each release has card, commit, timestamp, and title', async () => {
    const res = await fetch(`${API}/api/chorus/domain/chorus/releases`);
    const body = await res.json();
    var release = body.data.releases[0];
    expect(release).toHaveProperty('cardId');
    expect(release).toHaveProperty('title');
    expect(release).toHaveProperty('commit');
    expect(release).toHaveProperty('timestamp');
  }, 15_000);

  test('releases are ordered newest first', async () => {
    const res = await fetch(`${API}/api/chorus/domain/chorus/releases`);
    const body = await res.json();
    var releases = body.data.releases;
    if (releases.length >= 2) {
      expect(new Date(releases[0].timestamp).getTime()).toBeGreaterThanOrEqual(
        new Date(releases[1].timestamp).getTime()
      );
    }
  }, 15_000);

  test('uses athena envelope', async () => {
    const res = await fetch(`${API}/api/chorus/domain/chorus/releases`);
    const body = await res.json();
    expect(body._meta).toBeDefined();
    expect(body._meta.source).toBe('athena');
  }, 15_000);

  test('unknown domain returns empty releases', async () => {
    const res = await fetch(`${API}/api/chorus/domain/nonexistent-xyz/releases`);
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body.data.releases).toEqual([]);
  }, 15_000);
});
