// @test-type: integration:api — HTTP against its own Clearing (lib/own-clearing.cjs), signed in through its temp world
/**
 * #3827 — the room's key bindings: POST /api/room/bind and GET /api/room/bindings.
 * #4417 — neither route was named by any test. The page's side (autoJoin) runs
 * in clearing-room-key-3865; this file holds the server's answers.
 */
const { test, expect } = require('@playwright/test');
const { ownClearing } = require('./lib/own-clearing.cjs');

const WEBID = 'https://pods.example/jeff/profile/card#me';
const CLEARING_TARGET = ownClearing(test, { signedInAs: WEBID });
const CLEARING = CLEARING_TARGET.url;
const PUBKEY = 'ab'.repeat(32);

test.skip(!CLEARING_TARGET.own, 'needs the session this spec brings');
test.describe.configure({ mode: 'serial' });

const bind = (request, pubkey, signedIn = true) => request.post(`${CLEARING}/api/room/bind`, {
  data: { pubkey },
  headers: signedIn ? { cookie: `clearing_session=${CLEARING_TARGET.session()}` } : {},
});
const bindings = async (request) => (await request.get(`${CLEARING}/api/room/bindings`)).json();

test.describe('#3827 binding a browser key to a person', () => {
  test('NEGATIVE PROOF: not signed in → 401 not-signed-in, and nothing is bound', async ({ request }) => {
    const r = await bind(request, PUBKEY, false);
    expect(r.status()).toBe(401);
    expect((await r.json()).error).toBe('not-signed-in');
    expect(await bindings(request)).toEqual([]);
  });

  test('a key that is not 64 hex → 400 bad-pubkey, nothing bound', async ({ request }) => {
    const r = await bind(request, 'not-a-key');
    expect(r.status()).toBe(400);
    expect((await r.json()).error).toBe('bad-pubkey');
    expect(await bindings(request)).toEqual([]);
  });

  test('signed in: the key binds to the session\'s WebID, never one the body names, and the list shows it', async ({ request }) => {
    const r = await request.post(`${CLEARING}/api/room/bind`, {
      data: { pubkey: PUBKEY, webid: 'https://pods.example/impostor/profile/card#me' },
      headers: { cookie: `clearing_session=${CLEARING_TARGET.session()}` },
    });
    expect(r.status()).toBe(200);
    const body = await r.json();
    expect(body.webid).toBe(WEBID);
    expect(body.pubkey).toBe(PUBKEY);
    const list = await bindings(request);
    expect(list).toHaveLength(1);
    expect(JSON.stringify(list)).toContain(WEBID);
    expect(JSON.stringify(list)).not.toContain('impostor');
  });
});
