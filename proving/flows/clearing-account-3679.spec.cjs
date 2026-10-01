// @test-type: integration:api — HTTP against its own Clearing (lib/own-clearing.cjs) with the identity server (CSS) stubbed; never the live CSS
/**
 * #3679 — changing your password from the Clearing's account page.
 *
 * #4417 — the route had no test (0 files named it). The Clearing is signed in
 * as WEBID through its temp world; CSS_LOCAL_BASE points at a stub that plays
 * the identity server's account API, so no request can reach a real account.
 */
const http = require('http');
const { test, expect } = require('@playwright/test');
const { ownClearing } = require('./lib/own-clearing.cjs');

const WEBID = 'https://pods.example/jeff/profile/card#me';
const OTHER = 'https://pods.example/someone-else/profile/card#me';
const EMAIL = 'jeff@example.test';
const STUB_PORT = 30000 + Math.floor(Math.random() * 20000);
const CSS = `http://127.0.0.1:${STUB_PORT}`;

const seen = [];
let accountOwner = WEBID;
let cssMode = 'up'; // 'up' | 'error' (500s) | 'gone' (drops the connection)
const stub = http.createServer((req, res) => {
  let raw = '';
  req.on('data', (c) => { raw += c; });
  req.on('end', () => {
    const body = raw ? JSON.parse(raw) : {};
    seen.push({ method: req.method, url: req.url, body });
    const json = (code, o) => { res.statusCode = code; res.setHeader('content-type', 'application/json'); res.end(JSON.stringify(o)); };
    if (cssMode === 'gone') return req.socket.destroy();
    if (cssMode === 'error') return json(500, { error: 'down' });
    if (req.url === '/.account/login/password/') return body.password === 'right-old-pw' ? json(200, { authorization: 'acct-token' }) : json(401, {});
    if (req.url === '/.account/') return json(200, { controls: { account: { webId: `${CSS}/.account/webid` }, password: { create: `${CSS}/.account/pw` } } });
    if (req.url === '/.account/webid') return json(200, { webIdLinks: { [accountOwner]: `${CSS}/.account/link/1` } });
    if (req.url === '/.account/pw') return json(200, { passwordLogins: { [EMAIL]: `${CSS}/.account/login/1` } });
    if (req.url === '/.account/login/1' && req.method === 'POST') return json(200, {});
    return json(404, {});
  });
});
test.beforeAll(() => new Promise((r) => stub.listen(STUB_PORT, '127.0.0.1', r)));
test.afterAll(() => new Promise((r) => stub.close(r)));

const CLEARING_TARGET = ownClearing(test, { signedInAs: WEBID, env: { CSS_LOCAL_BASE: CSS } });
const CLEARING = CLEARING_TARGET.url;

test.skip(!CLEARING_TARGET.own, 'needs the session and stub this spec brings');
test.describe.configure({ mode: 'serial' });
test.beforeEach(() => { seen.length = 0; accountOwner = WEBID; cssMode = 'up'; });

const change = (request, data, signedIn = true) => request.post(`${CLEARING}/api/account/password`, {
  data,
  headers: signedIn ? { cookie: `clearing_session=${CLEARING_TARGET.session()}` } : {},
});

test.describe('#3679 change password', () => {
  test('NEGATIVE PROOF: not signed in → 401, and the identity server is never asked', async ({ request }) => {
    const r = await change(request, { email: EMAIL, oldPassword: 'right-old-pw', newPassword: 'a-new-long-pw' }, false);
    expect(r.status()).toBe(401);
    expect((await r.json()).message).toBe('Please sign in again.');
    expect(seen).toEqual([]);
  });

  test('a weak new password → 400 before any call to the identity server', async ({ request }) => {
    const r = await change(request, { email: EMAIL, oldPassword: 'right-old-pw', newPassword: 'short' });
    expect(r.status()).toBe(400);
    expect(seen).toEqual([]);
  });

  test('the wrong current password → 403 with the generic message', async ({ request }) => {
    const r = await change(request, { email: EMAIL, oldPassword: 'wrong', newPassword: 'a-new-long-pw' });
    expect(r.status()).toBe(403);
    expect((await r.json()).message).toBe('That email or current password is incorrect.');
    expect(seen.some((s) => s.url === '/.account/login/1')).toBe(false);
  });

  test('an account that is not the signed-in identity → 403, nothing changed', async ({ request }) => {
    accountOwner = OTHER;
    const r = await change(request, { email: EMAIL, oldPassword: 'right-old-pw', newPassword: 'a-new-long-pw' });
    expect(r.status()).toBe(403);
    expect(seen.some((s) => s.url === '/.account/login/1')).toBe(false);
  });

  // Found writing this case: a failing identity server used to answer "That email
  // or current password is incorrect", sending Jeff to reset a password that was right.
  for (const mode of ['error', 'gone']) {
    test(`the identity server ${mode === 'error' ? 'erroring' : 'unreachable'} → 502 that says so, never "incorrect password"`, async ({ request }) => {
      cssMode = mode;
      const r = await change(request, { email: EMAIL, oldPassword: 'right-old-pw', newPassword: 'a-new-long-pw' });
      expect(r.status()).toBe(502);
      expect((await r.json()).message).toMatch(/isn’t answering/);
    });
  }

  test('the happy path → 200 {ok:true}; the change carries the old and new password for this email only', async ({ request }) => {
    const r = await change(request, { email: EMAIL, oldPassword: 'right-old-pw', newPassword: 'a-new-long-pw' });
    expect(r.status()).toBe(200);
    expect(await r.json()).toEqual({ ok: true });
    const changed = seen.filter((s) => s.url === '/.account/login/1');
    expect(changed).toEqual([{ method: 'POST', url: '/.account/login/1', body: { oldPassword: 'right-old-pw', newPassword: 'a-new-long-pw' } }]);
    expect(seen.find((s) => s.url === '/.account/login/password/').body.email).toBe(EMAIL);
  });
});
