// @test-type: unit
// @domain: messages
// @card: #4571
// @owner: wren
// also proves #4966 (AC5, no sign-in, no room): its cases are the last block in this file
// #4571 — Silas 2026-10-16 19:59: no second sign-in. On the LAN address v2 has no
// session, so join and send are refused with a 401 that names why and where to sign in
// (the common door's v2 host); a signed-in browser is not sent away.
import http from 'http';
import { createApp } from '../src/server';

const DOOR = 'https://clearing-v2.lightlifeurbangardens.com/';

async function withApp(signInUrl: string | undefined, fn: (port: number) => Promise<void>) {
  const { app } = createApp({ relayUrl: '', pulseUrl: 'http://127.0.0.1:9', relaySigner: null, signInUrl });
  const server = http.createServer(app);
  await new Promise<void>((r) => server.listen(0, '127.0.0.1', () => r()));
  try { await fn((server.address() as { port: number }).port); } finally { await new Promise<void>((r) => server.close(() => r())); }
}

const post = (port: number, p: string, body: unknown) =>
  fetch(`http://127.0.0.1:${port}${p}`, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body) });

test('send with no session: 401 naming why and the door to sign in at', async () => {
  await withApp(DOOR, async (port) => {
    const res = await post(port, '/api/send', { event: { id: 'x', sig: 'y' } });
    expect(res.status).toBe(401);
    const j = (await res.json()) as { error?: string; signInUrl?: string; detail?: string };
    expect(j.error).toBe('not-signed-in');
    expect(j.signInUrl).toBe(DOOR);
    expect(j.detail).toContain(DOOR);
  });
});

test('join (bind) with no session: same 401 and door', async () => {
  await withApp(DOOR, async (port) => {
    const res = await post(port, '/api/room/bind', { pubkey: 'a'.repeat(64) });
    expect(res.status).toBe(401);
    const j = (await res.json()) as { error?: string; signInUrl?: string; detail?: string };
    expect(j.signInUrl).toBe(DOOR);
  });
});

test('negative proof: with no door configured the refusal still names why, and invents no URL', async () => {
  await withApp(undefined, async (port) => {
    const j = (await (await post(port, "/api/send", { event: { id: "x", sig: "y" } })).json()) as { error?: string; signInUrl?: string; detail?: string };
    expect(j.error).toBe('not-signed-in');
    expect(j.signInUrl).toBeUndefined();
    expect(j.detail).toMatch(/sign in/i);
  });
});

// ==========================================================================================
// #4966 — AC5, no sign-in, no room
// (kept in this file: the layout policy counts new files in a package not yet migrated)
// ==========================================================================================
/**
 * #4966 AC5 (Jeff 2026-11-14 20:18: "i should be forced to login / not see your messsages hut
 * have to login yo reply") and AC2. Measured 2026-11-15: the tunnel sends
 * lightlifeurbangardens.com/clearing-v2/* straight to this server, past the sign-in guard, and
 * the page and read APIs asked for no sign-in, so the room always looked signed in.
 *
 * One decision for every request (src/share-session gateDecision): a request that came through
 * the tunnel, or from any address but this machine, needs a signed-in person. A page gets the
 * sign-in page (nothing of the room); an API gets 401 not-signed-in. This machine's own callers
 * (probes, pulse, the roles' tools) and /health are let through.
 */
{
  const crypto = require('crypto');
  const http = require('http');
  const { gateDecision } = require('../src/share-session');
  const { createApp } = require('../src/server');

  const SIGN_IN = 'https://chorus.example/_auth/?next=https%3A%2F%2Fexample%2Fclearing-v2%2F';
  const TUNNELED = { host: 'lightlifeurbangardens.com', 'cf-connecting-ip': '203.0.113.9', 'cf-ray': '8f0-BOS' };
  const LOCAL = { host: '127.0.0.1:3472' };
  const JEFF = { id: 'principal-jeff', name: 'jeff', webid: 'https://id.example/jeff/profile/card#me' };

  describe('#4966 no sign-in, no room', () => {
    test('a tunneled page request with no sign-in is sent to the sign-in page', () => {
      expect(gateDecision({ headers: TUNNELED, ip: '127.0.0.1', method: 'GET', path: '/', principal: null, signInUrl: SIGN_IN }))
        .toEqual({ allow: false, kind: 'redirect', location: SIGN_IN });
    });

    test.each(['/api/messages', '/api/events', '/api/rooms/team', '/api/board', '/uploads/a.jpg'])(
      'a tunneled read of %s with no sign-in is refused 401 not-signed-in',
      (path) => {
        const d = gateDecision({ headers: TUNNELED, ip: '127.0.0.1', method: 'GET', path, principal: null, signInUrl: SIGN_IN });
        expect(d).toMatchObject({ allow: false, kind: 'refuse', status: 401, body: { ok: false, error: 'not-signed-in', signInUrl: SIGN_IN } });
      },
    );

    test.each(['/counts', '/sync', '/today', '/rooms/team', '/card/4966'])(
      'every page, not only the room, goes to the sign-in page: %s',
      (path) => {
        expect(gateDecision({ headers: TUNNELED, ip: '127.0.0.1', method: 'GET', path, principal: null, signInUrl: SIGN_IN }))
          .toMatchObject({ allow: false, kind: 'redirect' });
      },
    );

    // NEGATIVE PROOF (AC2, AC5): the same requests with a valid sign-in are let through untouched
    test.each(['/', '/api/messages', '/api/events', '/counts'])('signed in, %s is let through', (path) => {
      expect(gateDecision({ headers: TUNNELED, ip: '127.0.0.1', method: 'GET', path, principal: JEFF, signInUrl: SIGN_IN }))
        .toEqual({ allow: true });
    });

    test("this machine's own callers need no sign-in (probes, pulse, the roles' tools)", () => {
      expect(gateDecision({ headers: LOCAL, ip: '127.0.0.1', method: 'GET', path: '/api/messages', principal: null, signInUrl: SIGN_IN }))
        .toEqual({ allow: true });
      expect(gateDecision({ headers: LOCAL, ip: '::1', method: 'GET', path: '/', principal: null, signInUrl: SIGN_IN }))
        .toEqual({ allow: true });
    });

    test('/health answers anyone: the tunnel and the probes ask it with no sign-in', () => {
      expect(gateDecision({ headers: TUNNELED, ip: '127.0.0.1', method: 'GET', path: '/health', principal: null, signInUrl: SIGN_IN }))
        .toEqual({ allow: true });
    });

    // NEGATIVE PROOF: the tunnel arrives from 127.0.0.1 too, so the address alone must not open the room
    test('a tunneled request is gated although it arrives from 127.0.0.1', () => {
      const d = gateDecision({ headers: TUNNELED, ip: '127.0.0.1', method: 'GET', path: '/api/messages', principal: null, signInUrl: SIGN_IN });
      expect(d.allow).toBe(false);
    });

    // NEGATIVE PROOF: a caller cannot talk its way in by naming a local Host
    test('a tunneled request that names a local Host is still gated', () => {
      const d = gateDecision({
        headers: { ...TUNNELED, host: '127.0.0.1:3472' }, ip: '127.0.0.1', method: 'GET', path: '/', principal: null, signInUrl: SIGN_IN,
      });
      expect(d.allow).toBe(false);
    });

    test('another machine on the network is not this machine: it needs a sign-in', () => {
      const d = gateDecision({ headers: { host: '192.168.86.36:3472' }, ip: '192.168.86.50', method: 'GET', path: '/', principal: null, signInUrl: SIGN_IN });
      expect(d.allow).toBe(false);
    });

    // The guard sends a good session straight back here (#4966). If this server cannot read the
    // key the sign-in is signed with, it must say so loudly, never redirect: the phone would
    // bounce between the two for ever.
    test('a server that cannot read the sign-in key answers 503, for a page too, and never redirects', () => {
      for (const path of ['/', '/api/messages']) {
        const d = gateDecision({ headers: TUNNELED, ip: '127.0.0.1', method: 'GET', path, principal: null, signInUrl: SIGN_IN, keyUnreadable: true });
        expect(d).toMatchObject({ allow: false, kind: 'refuse', status: 503, body: { ok: false, error: 'sign-in-unreadable' } });
      }
    });

    // NEGATIVE PROOF: with the key readable, the same request is the ordinary redirect to sign-in
    test('with the key readable the same page request is the redirect', () => {
      const d = gateDecision({ headers: TUNNELED, ip: '127.0.0.1', method: 'GET', path: '/', principal: null, signInUrl: SIGN_IN, keyUnreadable: false });
      expect(d).toMatchObject({ allow: false, kind: 'redirect' });
    });

    test('with no sign-in address configured the page is refused, never served', () => {
      const d = gateDecision({ headers: TUNNELED, ip: '127.0.0.1', method: 'GET', path: '/', principal: null, signInUrl: undefined });
      expect(d).toMatchObject({ allow: false, kind: 'refuse', status: 401 });
    });
  });

  describe('#4966 the refusal log is bounded: this endpoint is public', () => {
    const { refusalLogLimiter } = require('../src/share-session');
    test('at most the cap reaches the log in a minute; the rest are counted and said once in the next', () => {
      const lines: string[] = [];
      let t = 1_000_000;
      const limited = refusalLogLimiter(3, (l: string) => lines.push(l), () => t);
      for (let i = 0; i < 10; i++) limited(`line ${i}`);
      expect(lines).toEqual(['line 0', 'line 1', 'line 2']);
      t += 60_000;
      limited('next minute');
      expect(lines).toHaveLength(5);
      expect(lines[3]).toMatch(/gate\.refused\.suppressed count=7/);
      expect(lines[4]).toBe('next minute');
    });
    // NEGATIVE PROOF: under the cap nothing is dropped and nothing is said about dropping
    test('under the cap every line is written', () => {
      const lines: string[] = [];
      let t = 5;
      const limited = refusalLogLimiter(3, (l: string) => lines.push(l), () => t);
      limited('a'); limited('b'); t += 60_000; limited('c');
      expect(lines).toEqual(['a', 'b', 'c']);
    });
  });

  // The decision is only worth something if the real app asks it on every route.
  describe('#4966 the real app asks the gate on every route', () => {
    const KEY = Buffer.from('test-secret-cookie-key-for-session-32b');
    const b64u = (b: Buffer) => b.toString('base64').replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
    const cookieFor = (expInSecs: number) => {
      const now = Math.floor(Date.now() / 1000);
      const payload = b64u(Buffer.from(JSON.stringify({ webid: JEFF.webid, exp: now + expInSecs, iat: now })));
      return `chorus_share_session=${payload}.${b64u(crypto.createHmac('sha256', KEY).update(payload).digest())}`;
    };
    const CF = { 'cf-connecting-ip': '203.0.113.9', 'cf-ray': '8f0-BOS' };

    async function up() {
      const { app, relay } = createApp({ relayUrl: 'ws://127.0.0.1:1', pulseUrl: 'http://127.0.0.1:9', cookieKey: KEY, signInUrl: SIGN_IN });
      const server = http.createServer(app);
      await new Promise<void>((r) => server.listen(0, '127.0.0.1', () => r()));
      const base = `http://127.0.0.1:${(server.address() as { port: number }).port}`;
      return { base, close: async () => { relay.close(); await new Promise<void>((r) => server.close(() => r())); } };
    }

    test('through the tunnel with no sign-in: the room page is a redirect to sign-in and carries no room', async () => {
      const s = await up();
      try {
        const res = await fetch(`${s.base}/`, { headers: CF, redirect: 'manual' });
        expect(res.status).toBe(302);
        expect(res.headers.get('location')).toBe(SIGN_IN);
        expect(res.headers.get('cache-control')).toBe('no-store');
        expect(await res.text()).not.toMatch(/room|message|composer/i);
      } finally { await s.close(); }
    });

    test.each(['/api/messages', '/api/events'])('through the tunnel with no sign-in: %s is 401 not-signed-in', async (path) => {
      const s = await up();
      try {
        const res = await fetch(`${s.base}${path}`, { headers: CF });
        expect(res.status).toBe(401);
        expect(await res.json()).toMatchObject({ ok: false, error: 'not-signed-in', signInUrl: SIGN_IN });
      } finally { await s.close(); }
    });

    // Round 3 (gate-code, run 2): the guard sends a session IT accepts straight back here. If this
    // server still refuses that cookie, a second redirect is a loop.
    test('sent to sign in, back within moments with a cookie this server still refuses: told so, never redirected again', async () => {
      const s = await up();
      try {
        const first = await fetch(`${s.base}/`, { headers: { ...CF, cookie: cookieFor(-60) }, redirect: 'manual' });
        expect(first.status).toBe(302);
        const marker = String(first.headers.get('set-cookie')).split(';')[0];
        expect(marker).toBe('cv2_signin_bounce=1');
        const second = await fetch(`${s.base}/`, { headers: { ...CF, cookie: `${cookieFor(-60)}; ${marker}` }, redirect: 'manual' });
        expect(second.status).toBe(503);
        expect(await second.json()).toMatchObject({ ok: false, error: 'sign-in-not-accepted' });
        expect(second.headers.get('location')).toBeNull();
        // the marker is spent, so the next visit is sent to sign in afresh
        expect(String(second.headers.get('set-cookie'))).toMatch(/cv2_signin_bounce=;.*Max-Age=0/);
      } finally { await s.close(); }
    });

    // NEGATIVE PROOF: the marker alone refuses nobody
    test('the marker with a good sign-in is let in; the marker with no cookie is sent to sign in; an API call still gets 401', async () => {
      const s = await up();
      try {
        const good = await fetch(`${s.base}/`, { headers: { ...CF, cookie: `${cookieFor(600)}; cv2_signin_bounce=1` }, redirect: 'manual' });
        expect(good.status).toBe(200);
        const none = await fetch(`${s.base}/`, { headers: { ...CF, cookie: 'cv2_signin_bounce=1' }, redirect: 'manual' });
        expect(none.status).toBe(302);
        const api = await fetch(`${s.base}/api/messages`, { headers: { ...CF, cookie: `${cookieFor(-60)}; cv2_signin_bounce=1` } });
        expect(api.status).toBe(401);
      } finally { await s.close(); }
    });

    test('a sign-in that has run out is no sign-in: the page goes to sign-in again', async () => {
      const s = await up();
      try {
        const res = await fetch(`${s.base}/`, { headers: { ...CF, cookie: cookieFor(-60) }, redirect: 'manual' });
        expect(res.status).toBe(302);
      } finally { await s.close(); }
    });

    // NEGATIVE PROOF (AC2, AC5): the same tunneled requests with a valid sign-in get the room, no notice
    test('through the tunnel, signed in: the room page is served', async () => {
      const s = await up();
      try {
        const res = await fetch(`${s.base}/`, { headers: { ...CF, cookie: cookieFor(3600) }, redirect: 'manual' });
        expect(res.status).toBe(200);
        // the signed-in room is never kept by the browser or the edge (Abby 2026-11-15)
        expect(res.headers.get('cache-control')).toBe('no-store');
        expect(await res.text()).toMatch(/<html/i);
      } finally { await s.close(); }
    });

    test('through the tunnel, a server with no readable key answers 503 and no redirect', async () => {
      const { app, relay } = createApp({ relayUrl: 'ws://127.0.0.1:1', pulseUrl: 'http://127.0.0.1:9', cookieKey: null, signInUrl: SIGN_IN });
      const server = http.createServer(app);
      await new Promise<void>((r) => server.listen(0, '127.0.0.1', () => r()));
      try {
        const res = await fetch(`http://127.0.0.1:${(server.address() as { port: number }).port}/`, { headers: CF, redirect: 'manual' });
        expect(res.status).toBe(503);
        expect(res.headers.get('location')).toBeNull();
      } finally { relay.close(); await new Promise<void>((r) => server.close(() => r())); }
    });

    // AC1's unnamed case: a refused request now says why, so the next one names itself
    test('each refused request is recorded with why and which browser, and never the cookie', async () => {
      const lines: string[] = [];
      const { app, relay } = createApp({ relayUrl: 'ws://127.0.0.1:1', pulseUrl: 'http://127.0.0.1:9', cookieKey: KEY, signInUrl: SIGN_IN, gateLog: (l: string) => lines.push(l) });
      const server = http.createServer(app);
      await new Promise<void>((r) => server.listen(0, '127.0.0.1', () => r()));
      const base = `http://127.0.0.1:${(server.address() as { port: number }).port}`;
      try {
        const UA = { 'user-agent': 'Mozilla/5.0 (iPhone) Version/19 Mobile Safari' };
        await fetch(`${base}/`, { headers: { ...CF, ...UA }, redirect: 'manual' });
        const stale = cookieFor(-60);
        await fetch(`${base}/api/tiles`, { headers: { ...CF, ...UA, cookie: stale } });
        await fetch(`${base}/api/tiles`, { headers: { ...CF, ...UA, cookie: 'chorus_share_session=abc.def' } });
        expect(lines[0]).toMatch(/^\[clearing-v2\] gate\.refused \S+ status=302 why=no-cookie GET \/ ua="Mozilla\/5\.0 \(iPhone\)/);
        expect(lines[1]).toMatch(/status=401 why=expired GET \/api\/tiles/);
        expect(lines[2]).toMatch(/status=401 why=bad-signature/);
        expect(lines.join('\n')).not.toContain(stale.split('=')[1]);
        // NEGATIVE PROOF: a served request records nothing
        const before = lines.length;
        await fetch(`${base}/api/tiles`, { headers: { ...CF, ...UA, cookie: cookieFor(3600) } });
        expect(lines.length).toBe(before);
      } finally { relay.close(); await new Promise<void>((r) => server.close(() => r())); }
    });

    test("this machine's own callers still read with no sign-in", async () => {
      const s = await up();
      try {
        expect((await fetch(`${s.base}/api/messages`)).status).toBe(200);
        expect((await fetch(`${s.base}/health`, { headers: CF })).status).toBe(200);
      } finally { await s.close(); }
    });
  });
}
