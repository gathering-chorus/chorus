// @test-type: integration:api — in-process TestApp harness
/**
 * Session Replay endpoints + page — #2099
 *
 * Final per-page migration (9/9). Reads rrweb sessions from Gathering's
 * data/sessions/ directory; /borg/replay/ viewer uses rrweb-player from CDN.
 */

import { startTestApp, type TestApp } from './lib/test-app';

describe('#2099: /api/chorus/sessions', () => {


  let harness: TestApp;

  beforeAll(async () => { harness = await startTestApp(); });
  afterAll(async () => { if (harness) await harness.close(); });
  test('list returns 200 with sessions array', async () => {
    const res = await fetch(`${harness.baseUrl}/api/chorus/sessions`);
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(Array.isArray(body.sessions)).toBe(true);
  });

  test('invalid session id returns 400', async () => {
    const res = await fetch(`${harness.baseUrl}/api/chorus/sessions/not-a-valid-id`);
    expect(res.status).toBe(400);
  });

  test('nonexistent valid id returns 404', async () => {
    const res = await fetch(`${harness.baseUrl}/api/chorus/sessions/ses_0000000000_xxxxx`);
    expect(res.status).toBe(404);
  });
});

describe('#2099: /borg/replay/ static page', () => {


  let harness: TestApp;

  beforeAll(async () => { harness = await startTestApp(); });
  afterAll(async () => { if (harness) await harness.close(); });
  test('GET /borg/replay/ returns 200', async () => {
    const res = await fetch(`${harness.baseUrl}/borg/replay/`);
    expect(res.status).toBe(200);
  });

  test('page references rrweb player CDN and chorus-api sessions endpoint', async () => {
    const res = await fetch(`${harness.baseUrl}/borg/replay/`);
    const html = await res.text();
    expect(html).toContain('rrweb-player');
    expect(html).toContain('/api/chorus/sessions');
  });

  test('page has session-list and player-wrapper containers', async () => {
    const res = await fetch(`${harness.baseUrl}/borg/replay/`);
    const html = await res.text();
    expect(html).toContain('id="session-list"');
    expect(html).toContain('id="player-wrapper"');
  });
});
