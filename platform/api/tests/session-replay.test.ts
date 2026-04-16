/**
 * Session Replay endpoints + page — #2099
 *
 * Final per-page migration (9/9). Reads rrweb sessions from Gathering's
 * data/sessions/ directory; /borg/replay/ viewer uses rrweb-player from CDN.
 */

const INTEGRATION_ENABLED = process.env.RUN_INTEGRATION === 'true';
const API = process.env.CHORUS_API || 'http://localhost:3340';

const describeIntegration = INTEGRATION_ENABLED ? describe : describe.skip;

describeIntegration('#2099: /api/chorus/sessions', () => {

  test('list returns 200 with sessions array', async () => {
    const res = await fetch(`${API}/api/chorus/sessions`);
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(Array.isArray(body.sessions)).toBe(true);
  }, 10_000);

  test('invalid session id returns 400', async () => {
    const res = await fetch(`${API}/api/chorus/sessions/not-a-valid-id`);
    expect(res.status).toBe(400);
  }, 10_000);

  test('nonexistent valid id returns 404', async () => {
    const res = await fetch(`${API}/api/chorus/sessions/ses_0000000000_xxxxx`);
    expect(res.status).toBe(404);
  }, 10_000);
});

describeIntegration('#2099: /borg/replay/ static page', () => {

  test('GET /borg/replay/ returns 200', async () => {
    const res = await fetch(`${API}/borg/replay/`);
    expect(res.status).toBe(200);
  }, 10_000);

  test('page references rrweb player CDN and chorus-api sessions endpoint', async () => {
    const res = await fetch(`${API}/borg/replay/`);
    const html = await res.text();
    expect(html).toContain('rrweb-player');
    expect(html).toContain('/api/chorus/sessions');
  }, 10_000);

  test('page has session-list and player-wrapper containers', async () => {
    const res = await fetch(`${API}/borg/replay/`);
    const html = await res.text();
    expect(html).toContain('id="session-list"');
    expect(html).toContain('id="player-wrapper"');
  }, 10_000);
});
