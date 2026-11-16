// @test-type: integration — the real app on a free local port with a test cookie key and a stand-in guard on another free port; nothing public.
// @domain: messages
// @card: #4966
// @owner: abby-normal
/**
 * #4966 AC3 — a sign-in on the phone survives the page sitting in the background.
 *
 * Measured 2026-11-15: the tunnel sends Clearing v2's traffic straight to this server, so the
 * sign-in guard never saw Jeff use the page and never renewed his sign-in (zero guard lines from
 * the phone between sign-ins). v1 relays his cookie to the guard to renew it (#4500); v2 did not.
 * Now v2 does: on his requests it asks the guard, at most once per ten minutes per session, and
 * hands the renewed cookie back to his browser.
 */
import crypto from 'crypto';
import http from 'http';
import { createApp } from '../src/server';
import { relayRenew, verifyShareSession, RENEW_EVERY_MS } from '../src/share-session';

const KEY = Buffer.from('test-secret-cookie-key-for-session-32b');
const WEBID = 'https://id.example/jeff/profile/card#me';
const CF = { 'cf-connecting-ip': '203.0.113.9', 'cf-ray': '8f0-BOS' };
const b64u = (b: Buffer) => b.toString('base64').replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
function cookieFor(sid: string | undefined, expInSecs = 3600) {
  const now = Math.floor(Date.now() / 1000);
  const payload = b64u(Buffer.from(JSON.stringify({ webid: WEBID, exp: now + expInSecs, iat: now, ...(sid ? { sid } : {}) })));
  return `chorus_share_session=${payload}.${b64u(crypto.createHmac('sha256', KEY).update(payload).digest())}`;
}

/** A stand-in for the guard's /_auth/renew: 204 with a renewed cookie when it is sent one, else 401. */
async function fakeGuard() {
  const asked: Array<string | undefined> = [];
  const server = http.createServer((req, res) => {
    if (req.url !== '/_auth/renew') { res.writeHead(404); res.end(); return; }
    asked.push(req.headers.cookie);
    if (!req.headers.cookie) { res.writeHead(401); res.end(); return; }
    res.writeHead(204, { 'Set-Cookie': `chorus_share_session=renewed-${asked.length}; Path=/; HttpOnly; Secure; SameSite=Lax; Max-Age=43200` });
    res.end();
  });
  await new Promise<void>((r) => server.listen(0, '127.0.0.1', () => r()));
  return { asked, url: `http://127.0.0.1:${(server.address() as { port: number }).port}`, close: () => new Promise<void>((r) => server.close(() => r())) };
}

async function up(guardUrl: string, clock: { ms: number }) {
  const { app, relay } = createApp({
    relayUrl: 'ws://127.0.0.1:1', pulseUrl: 'http://127.0.0.1:9', cookieKey: KEY,
    signInUrl: 'https://chorus.example/_auth/', guardUrl, now: () => clock.ms,
  });
  const server = http.createServer(app);
  await new Promise<void>((r) => server.listen(0, '127.0.0.1', () => r()));
  const base = `http://127.0.0.1:${(server.address() as { port: number }).port}`;
  return { base, close: async () => { relay.close(); await new Promise<void>((r) => server.close(() => r())); } };
}

describe('#4966 the sign-in is renewed while he uses the page', () => {
  test('the session id the guard put in the cookie is read (without it nothing can be renewed)', () => {
    const v = verifyShareSession(cookieFor('jeff-browser-1').split('=')[1], KEY);
    expect(v).toMatchObject({ ok: true, webid: WEBID, sid: 'jeff-browser-1' });
  });

  test('a signed-in request gets a renewed cookie from the guard, and again after 30 minutes in the background', async () => {
    const guard = await fakeGuard();
    const clock = { ms: Date.now() };
    const s = await up(guard.url, clock);
    try {
      const cookie = cookieFor('jeff-browser-1');
      const first = await fetch(`${s.base}/api/tiles`, { headers: { ...CF, cookie } });
      expect(first.status).toBe(200);
      expect(first.headers.get('set-cookie')).toMatch(/^chorus_share_session=renewed-1;/);

      // the page polls every 5 s; the guard is not asked again for the same session inside ten minutes
      clock.ms += 5_000;
      const soon = await fetch(`${s.base}/api/tiles`, { headers: { ...CF, cookie } });
      expect(soon.headers.get('set-cookie')).toBeNull();
      expect(guard.asked.length).toBe(1);

      // 30 minutes with the page in the background, then he wakes the phone
      clock.ms += 30 * 60_000;
      const woke = await fetch(`${s.base}/api/tiles`, { headers: { ...CF, cookie } });
      expect(woke.status).toBe(200);
      expect(woke.headers.get('set-cookie')).toMatch(/^chorus_share_session=renewed-2;/);
      expect(guard.asked).toEqual([cookie, cookie]);
    } finally { await s.close(); await guard.close(); }
  });

  // NEGATIVE PROOF (AC3): with the cookie removed the same request is refused and nothing is renewed
  test('with the cookie removed, the request is not served and the guard is never asked', async () => {
    const guard = await fakeGuard();
    const s = await up(guard.url, { ms: Date.now() });
    try {
      const res = await fetch(`${s.base}/api/tiles`, { headers: CF });
      expect(res.status).toBe(401);
      expect(res.headers.get('set-cookie')).toBeNull();
      expect(guard.asked.length).toBe(0);
    } finally { await s.close(); await guard.close(); }
  });

  test('a guard that is down never refuses his request: he is served, with no renewed cookie', async () => {
    const s = await up('http://127.0.0.1:1', { ms: Date.now() });
    try {
      const res = await fetch(`${s.base}/api/tiles`, { headers: { ...CF, cookie: cookieFor('jeff-browser-1') } });
      expect(res.status).toBe(200);
      expect(res.headers.get('set-cookie')).toBeNull();
    } finally { await s.close(); }
  });
});

describe('#4966 relayRenew', () => {
  const guard200 = (async () => new Response(null, { status: 204, headers: { 'set-cookie': 'chorus_share_session=new' } })) as unknown as typeof fetch;

  test('asks once per session per ten minutes', async () => {
    const asked = new Map<string, number>();
    expect(await relayRenew('c=1', 'sid-1', asked, 'http://g', 1_000, guard200)).toBe('chorus_share_session=new');
    expect(await relayRenew('c=1', 'sid-1', asked, 'http://g', 1_000 + RENEW_EVERY_MS - 1, guard200)).toBeNull();
    expect(await relayRenew('c=1', 'sid-1', asked, 'http://g', 1_000 + RENEW_EVERY_MS, guard200)).toBe('chorus_share_session=new');
  });

  test('no session id or no cookie: nothing is asked', async () => {
    let calls = 0;
    const counting = (async () => { calls += 1; return new Response(null, { status: 204 }); }) as unknown as typeof fetch;
    expect(await relayRenew('c=1', undefined, new Map(), 'http://g', 1, counting)).toBeNull();
    expect(await relayRenew(undefined, 'sid-1', new Map(), 'http://g', 1, counting)).toBeNull();
    expect(calls).toBe(0);
  });

  test('a guard that refuses, or cannot be reached, renews nothing', async () => {
    const refuses = (async () => new Response(null, { status: 401 })) as unknown as typeof fetch;
    const down = (async () => { throw new Error('ECONNREFUSED'); }) as unknown as typeof fetch;
    expect(await relayRenew('c=1', 'a', new Map(), 'http://g', 1, refuses)).toBeNull();
    expect(await relayRenew('c=1', 'b', new Map(), 'http://g', 1, down)).toBeNull();
  });
});
