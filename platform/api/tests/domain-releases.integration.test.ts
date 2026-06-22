/**
 * @test-type: api
 *
 * Domain release history — #1910
 *
 * Domain-scoped deploy history from git log + card domain tags.
 * Each release has card ID, title, commit, timestamp, role.
 *
 * #3559: CONTRACT-ONLY. "releases.length > 0" was data-coupled (invariant #4) —
 * it false-red'd whenever the chorus domain happened to have no recorded
 * releases in the live graph (e.g. mid data-recovery). We now assert the
 * endpoint returns a releases ARRAY with the right item shape WHEN populated.
 * Whether deploy history is currently present is an alert-layer question.
 */

import { startTestApp, type TestApp } from './lib/test-app';

describe('#1910: domain release history', () => {


  let harness: TestApp;

  beforeAll(async () => { harness = await startTestApp(); });
  afterAll(async () => { if (harness) await harness.close(); });
  test('GET /api/chorus/domain/:name/releases returns a release list', async () => {
    const res = await fetch(`${harness.baseUrl}/api/chorus/domain/chorus/releases`);
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body.data).toBeDefined();
    expect(Array.isArray(body.data.releases)).toBe(true);
  }, 15_000);

  test('each release, when present, has card, commit, timestamp, and title', async () => {
    const res = await fetch(`${harness.baseUrl}/api/chorus/domain/chorus/releases`);
    const body = await res.json();
    const releases = body.data.releases;
    /* eslint-disable jest/no-conditional-expect */
    if (releases.length > 0) {
      const release = releases[0];
      expect(release).toHaveProperty('cardId');
      expect(release).toHaveProperty('title');
      expect(release).toHaveProperty('commit');
      expect(release).toHaveProperty('timestamp');
    }
    /* eslint-enable jest/no-conditional-expect */
  }, 15_000);

  test('releases are ordered newest first', async () => {
    const res = await fetch(`${harness.baseUrl}/api/chorus/domain/chorus/releases`);
    const body = await res.json();
    var releases = body.data.releases;
    // eslint-disable-next-line jest/no-conditional-expect -- ordering check only when ≥2 releases observed
    if (releases.length >= 2) expect(new Date(releases[0].timestamp).getTime()).toBeGreaterThanOrEqual(new Date(releases[1].timestamp).getTime());
  }, 15_000);

  test('uses athena envelope', async () => {
    const res = await fetch(`${harness.baseUrl}/api/chorus/domain/chorus/releases`);
    const body = await res.json();
    expect(body._meta).toBeDefined();
    expect(body._meta.source).toBe('athena');
  }, 15_000);

  test('unknown domain returns empty releases', async () => {
    const res = await fetch(`${harness.baseUrl}/api/chorus/domain/nonexistent-xyz/releases`);
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body.data.releases).toEqual([]);
  }, 15_000);
});
