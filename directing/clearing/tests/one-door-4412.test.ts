// @test-type: unit:security — drives the real gate over HTTP in-process; temp guard key, stub allow-set store, no live guard or identity server.
// @card: #4412
// @owner: wren
/**
 * #4412 — one sign-in for Chorus. Jeff, 2026-10-12: "only 1 login / sign in
 * flow", "no matter what page i need to reach", "its for chorus not a page".
 *
 * The door (the share guard, /_auth/) signs a person in; the Clearing only
 * trusts its cookie. So the Clearing has no sign-in routes, no cookie of its
 * own, no local fallback when the door is not configured, and every Sign out
 * goes to the door's sign-out. A home-network browser (Host an IP or a .local
 * name) can never carry the door's parent-domain cookie, so it is sent to the
 * public Clearing URL instead of being let in unsigned.
 */
import fs from 'fs';
import path from 'path';
import crypto from 'crypto';
import { useInProcessClearing } from './lib/in-process-clearing'; // first: sets the temp guard key path
import { isLocalConnection, lanRedirect } from '../src/connection-auth';

const gate = useInProcessClearing();
const DOOR = 'https://door.example/_auth/login';
const DOOR_OUT = 'https://chorus.lightlifeurbangardens.com/_auth/logout';

afterEach(() => { delete process.env.CHORUS_SIGNIN_URL; });

describe('#4412 the Clearing has no sign-in of its own', () => {
  for (const route of ['/auth/login', '/auth/callback', '/auth/signout']) {
    test(`NEGATIVE PROOF: GET ${route} is 404 for a signed-in visitor — the route is gone`, async () => {
      const r = await gate.visit('GET', route, gate.guardCookie(gate.ALLOWED));
      expect(r.status).toBe(404);
    });

    test(`NEGATIVE PROOF: GET ${route} unsigned goes to the door, never to an identity server`, async () => {
      process.env.CHORUS_SIGNIN_URL = DOOR;
      const r = await gate.visit('GET', route);
      expect(r.status).toBe(302);
      expect(r.location).toContain(`${DOOR}?next=`);
    });
  }

  test('NEGATIVE PROOF: a request carrying only a clearing_session cookie is signed out — sent to the door', async () => {
    // A cookie the old code admitted: signed with the Clearing's own secret,
    // for an allowed WebID, fresh, with its person-session row present.
    const home = process.env.CLEARING_CHORUS_HOME!;
    const secretFile = path.join(home, 'clearing-session-secret');
    if (!fs.existsSync(secretFile)) fs.writeFileSync(secretFile, crypto.randomBytes(32).toString('hex'));
    const secret = fs.readFileSync(secretFile, 'utf-8').trim();
    const psk = `test-psk-${process.pid}`;
    fs.writeFileSync(path.join(home, 'clearing-person-sessions.json'), JSON.stringify({
      [psk]: { key: psk, principal: 'jeff', rowName: psk, row: {}, idToken: '', exp: Math.floor(Date.now() / 1000) + 3600, lastSeenWrite: Date.now() },
    }));
    const b64u = (b: Buffer): string => b.toString('base64').replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
    const body = b64u(Buffer.from(JSON.stringify({ typ: 'session', webid: gate.ALLOWED, iat: Date.now(), psk })));
    const cookie = `clearing_session=${body}.${b64u(crypto.createHmac('sha256', secret).update(body).digest())}`;

    process.env.CHORUS_SIGNIN_URL = DOOR;
    const r = await gate.visit('GET', '/', cookie);
    expect(r.status).toBe(302);
    expect(r.location).toContain(`${DOOR}?next=`);
    expect(r.body).not.toContain('window.BRIDGE_USER');
  });

  test('the guard cookie still lets him in (the one door works)', async () => {
    const r = await gate.visit('GET', '/', gate.guardCookie(gate.ALLOWED));
    expect(r.status).toBe(200);
    expect(r.body).toContain('window.BRIDGE_USER="jeff"');
  });

  test('NEGATIVE PROOF: CHORUS_SIGNIN_URL unset → 503 naming the missing door, never a sign-in page', async () => {
    delete process.env.CHORUS_SIGNIN_URL;
    const r = await gate.visit('GET', '/');
    expect(r.status).toBe(503);
    expect(r.body).toMatch(/sign-in door isn.t configured/i);
    expect(r.body).not.toMatch(/>Sign in</);
    expect(r.body).not.toContain('/auth/login');
  });

  test('an unsigned non-GET is refused with 401, not redirected', async () => {
    process.env.CHORUS_SIGNIN_URL = DOOR;
    const r = await gate.visit('POST', '/api/message');
    expect(r.status).toBe(401);
  });
});

describe('#4412 every Sign out goes to the door', () => {
  test('the room tells the page where sign-out is', async () => {
    const r = await gate.visit('GET', '/', gate.guardCookie(gate.ALLOWED));
    expect(r.body).toContain(`window.CHORUS_SIGNOUT_URL="${DOOR_OUT}"`);
  });

  test('the refused page signs out at the door', async () => {
    const r = await gate.visit('GET', '/', gate.guardCookie(gate.STRANGER));
    expect(r.status).toBe(403);
    expect(r.body).toContain(`href="${DOOR_OUT}"`);
    expect(r.body).toContain('Sign out');
  });

  test('the page itself has no local sign-out link', () => {
    const html = fs.readFileSync(path.join(__dirname, '..', 'public', 'index.html'), 'utf-8');
    expect(html).not.toMatch(/href="\/logout"/);
    expect(html).toContain('window.CHORUS_SIGNOUT_URL');
  });
});

describe('#4412 the home network is not an identity', () => {
  const PUBLIC = 'https://clearing.lightlifeurbangardens.com';

  test('NEGATIVE PROOF: a LAN address is no longer local (it used to skip every check)', () => {
    expect(isLocalConnection({ host: '192.168.86.23:3470' }, '192.168.86.42')).toBe(false);
    expect(isLocalConnection({ host: 'jeffs-mac-mini-m1-3.local:3470' }, '::ffff:192.168.86.42')).toBe(false);
  });

  test('loopback stays local (agents, local tools)', () => {
    expect(isLocalConnection({ host: 'localhost:3470' }, '127.0.0.1')).toBe(true);
    expect(isLocalConnection({ host: 'localhost:3470' }, '::1')).toBe(true);
  });

  test('NEGATIVE PROOF: a LAN-host person request is sent to the public Clearing, same path', () => {
    expect(lanRedirect({ host: '192.168.86.23:3470' }, '192.168.86.42', '/account?x=1', PUBLIC))
      .toBe(`${PUBLIC}/account?x=1`);
    expect(lanRedirect({ host: 'jeffs-mac-mini-m1-3.local:3470' }, '192.168.86.42', '/', PUBLIC))
      .toBe(`${PUBLIC}/`);
  });

  test('loopback, the tunnel, and a lightlifeurbangardens.com Host are not redirected', () => {
    expect(lanRedirect({ host: 'localhost:3470' }, '127.0.0.1', '/', PUBLIC)).toBeNull();
    expect(lanRedirect({ host: '192.168.86.23:3470' }, '::1', '/', PUBLIC)).toBeNull();
    expect(lanRedirect({ host: 'clearing.lightlifeurbangardens.com', 'cf-ray': 'x' }, '127.0.0.1', '/', PUBLIC)).toBeNull();
    expect(lanRedirect({ host: 'clearing.lightlifeurbangardens.com' }, '192.168.86.42', '/', PUBLIC)).toBeNull();
  });

  test('an open-redirect path never leaves the public host', () => {
    expect(lanRedirect({ host: '192.168.86.23:3470' }, '192.168.86.42', '//evil.example/x', PUBLIC)).toBe(`${PUBLIC}/`);
  });
});

